use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Parser)]
#[command(name = "mclaw")]
#[command(about = "MerlionClaw - Infrastructure Agent Runtime")]
#[command(version)]
struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "~/.merlionclaw/config.toml")]
    config: String,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway and agent
    Run {
        /// Run in sidecar mode (K8s)
        #[arg(long)]
        sidecar: bool,
    },
    /// Guided setup wizard
    Onboard,
    /// Show running status
    Status,
    /// Check config and connectivity
    Doctor,
}

fn init_tracing(log_level: &str) {
    let env_filter = std::env::var("MCLAW_LOG").unwrap_or_else(|_| log_level.to_string());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&env_filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .init();
}

/// App config loaded from TOML.
#[derive(Debug, Default, serde::Deserialize)]
struct AppConfig {
    #[serde(default)]
    gateway: mclaw_gateway::config::GatewayConfig,
    #[serde(default)]
    channels: ChannelsConfig,
    #[serde(default)]
    agent: AgentConfig,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ChannelsConfig {
    #[serde(default)]
    telegram: TelegramChannelConfig,
}

#[derive(Debug, Default, serde::Deserialize)]
struct TelegramChannelConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_telegram_token_env")]
    bot_token_env: String,
    #[serde(default)]
    allow_from: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AgentConfig {
    #[serde(default = "default_model")]
    default_model: String,
    #[serde(default)]
    providers: ProvidersConfig,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ProvidersConfig {
    #[serde(default)]
    anthropic: ProviderConfig,
}

#[derive(Debug, serde::Deserialize)]
struct ProviderConfig {
    #[serde(default = "default_anthropic_key_env")]
    api_key_env: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_anthropic_key_env(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            providers: ProvidersConfig::default(),
        }
    }
}

fn default_telegram_token_env() -> String {
    "TELEGRAM_BOT_TOKEN".to_string()
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_anthropic_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

fn load_config(path: &str) -> AppConfig {
    let expanded = shellexpand::tilde(path).to_string();
    match std::fs::read_to_string(&expanded) {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => {
                info!(path = %expanded, "config loaded");
                config
            }
            Err(e) => {
                tracing::warn!(path = %expanded, error = %e, "invalid config, using defaults");
                AppConfig::default()
            }
        },
        Err(_) => {
            tracing::warn!(path = %expanded, "config not found, using defaults");
            AppConfig::default()
        }
    }
}

/// Bridge between the agent and the gateway's MessageHandler trait.
struct AgentHandler {
    agent: mclaw_agent::agent::Agent,
}

#[async_trait::async_trait]
impl mclaw_gateway::server::MessageHandler for AgentHandler {
    async fn handle(
        &self,
        session_id: String,
        sender: String,
        content: String,
        _history: Vec<String>,
    ) -> mclaw_gateway::server::HandlerResponse {
        let chat = mclaw_agent::agent::InboundChat {
            session_id,
            sender,
            content,
            history: vec![], // Context managed by the agent in future
        };

        match self.agent.handle_message(chat).await {
            mclaw_agent::agent::AgentResponse::Reply {
                session_id,
                content,
            } => mclaw_gateway::server::HandlerResponse::Reply {
                session_id,
                content,
            },
            mclaw_agent::agent::AgentResponse::Error {
                session_id,
                message,
            } => mclaw_gateway::server::HandlerResponse::Error {
                session_id,
                message,
            },
            mclaw_agent::agent::AgentResponse::ContextCleared { session_id } => {
                mclaw_gateway::server::HandlerResponse::ContextCleared { session_id }
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level);

    match cli.command {
        Commands::Run { sidecar } => {
            if sidecar {
                info!("starting in sidecar mode");
            } else {
                info!("starting gateway and agent");
            }

            let config = load_config(&cli.config);

            // Create LLM provider
            let provider = mclaw_agent::llm::anthropic::AnthropicProvider::from_env(
                &config.agent.providers.anthropic.api_key_env,
            )?;

            // Discover skills
            let skills_dir = std::path::Path::new("skills");
            let mut registry = mclaw_skills::registry::SkillRegistry::discover(skills_dir)?;

            // Register K8s skill handler if available
            match mclaw_skills::k8s::K8sSkill::new().await {
                Ok(k8s) => {
                    registry.register_handler("k8s", Box::new(k8s));
                    info!("K8s skill handler registered");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "K8s skill not available (no cluster connection)");
                }
            }

            // Create agent
            let agent = mclaw_agent::agent::Agent::new(
                Box::new(provider),
                config.agent.default_model.clone(),
            )
            .with_dispatcher(Box::new(registry));

            let handler: Arc<dyn mclaw_gateway::server::MessageHandler> =
                Arc::new(AgentHandler { agent });

            let shutdown = tokio_util::sync::CancellationToken::new();

            // Start gateway with agent
            let gateway_config = config.gateway.clone();
            let handler_clone = handler.clone();
            let gateway_handle = tokio::spawn(async move {
                if let Err(e) =
                    mclaw_gateway::server::start_with_handler(gateway_config, Some(handler_clone))
                        .await
                {
                    tracing::error!(error = %e, "gateway error");
                }
            });

            // Give gateway a moment to bind
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Start Telegram adapter if enabled
            let telegram_handle = if config.channels.telegram.enabled {
                let bot_token = std::env::var(&config.channels.telegram.bot_token_env).map_err(
                    |_| anyhow::anyhow!("{} not set", config.channels.telegram.bot_token_env),
                )?;

                let telegram_config = mclaw_channels::telegram::TelegramConfig {
                    bot_token,
                    allow_from: config.channels.telegram.allow_from.clone(),
                };

                let adapter = mclaw_channels::telegram::TelegramAdapter::new(telegram_config);
                let gateway_url =
                    format!("ws://{}:{}", config.gateway.host, config.gateway.port);
                let shutdown_clone = shutdown.clone();

                Some(tokio::spawn(async move {
                    if let Err(e) = mclaw_channels::traits::ChannelAdapter::start(
                        &adapter,
                        gateway_url,
                        shutdown_clone,
                    )
                    .await
                    {
                        tracing::error!(error = %e, "Telegram adapter error");
                    }
                }))
            } else {
                info!("Telegram adapter disabled");
                None
            };

            // Wait for shutdown signal
            let shutdown_clone = shutdown.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                info!("received shutdown signal");
                shutdown_clone.cancel();
            });

            // Wait for gateway (runs forever until abort)
            tokio::select! {
                _ = gateway_handle => {}
                _ = shutdown.cancelled() => {
                    info!("shutting down");
                }
            }

            if let Some(handle) = telegram_handle {
                handle.abort();
            }
        }
        Commands::Onboard => {
            info!("starting onboard wizard");
            println!("MerlionClaw Setup Wizard");
            println!("========================");
            println!();
            println!("1. Set ANTHROPIC_API_KEY environment variable");
            println!("2. Set TELEGRAM_BOT_TOKEN environment variable (from @BotFather)");
            println!("3. Create config at ~/.merlionclaw/config.toml");
            println!("4. Run: mclaw doctor  (to verify setup)");
            println!("5. Run: mclaw run     (to start)");
        }
        Commands::Status => {
            let config = load_config(&cli.config);
            let url = format!(
                "http://{}:{}/health",
                config.gateway.host, config.gateway.port
            );
            match reqwest::get(&url).await {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().await?;
                    println!("Gateway: running");
                    println!("Sessions: {}", body["sessions"]);
                }
                Err(_) => {
                    println!("Gateway: not running");
                }
            }
        }
        Commands::Doctor => {
            let config = load_config(&cli.config);
            run_doctor(&cli.config, &config).await;
        }
    }

    Ok(())
}

async fn run_doctor(config_path: &str, config: &AppConfig) {
    println!("MerlionClaw Doctor");
    println!("==================");

    // Check config
    let expanded = shellexpand::tilde(config_path).to_string();
    if std::path::Path::new(&expanded).exists() {
        println!("  Config file: {expanded}");
    } else {
        println!("  Config file: not found ({expanded})");
    }

    // Check Anthropic API key
    let api_key_env = &config.agent.providers.anthropic.api_key_env;
    match std::env::var(api_key_env) {
        Ok(key) if !key.is_empty() => {
            println!("  Anthropic API: key set ({api_key_env})");
        }
        _ => {
            println!("  Anthropic API: {api_key_env} not set");
        }
    }

    // Check Telegram bot token
    let bot_token_env = &config.channels.telegram.bot_token_env;
    if config.channels.telegram.enabled {
        match std::env::var(bot_token_env) {
            Ok(token) if !token.is_empty() => {
                println!("  Telegram bot: token set ({bot_token_env})");
            }
            _ => {
                println!("  Telegram bot: {bot_token_env} not set");
            }
        }
    } else {
        println!("  Telegram bot: disabled");
    }

    // Check K8s connectivity
    match mclaw_skills::k8s::K8sSkill::new().await {
        Ok(_) => {
            println!("  Kubernetes: connected");
        }
        Err(e) => {
            println!("  Kubernetes: not available ({e})");
        }
    }

    // Check skills directory
    let skills_dir = std::path::Path::new("skills");
    match mclaw_skills::registry::SkillRegistry::discover(skills_dir) {
        Ok(registry) => {
            let defs = registry.tool_definitions();
            println!("  Skills: {} tools loaded", defs.len());
        }
        Err(e) => {
            println!("  Skills: error ({e})");
        }
    }
}
