mod tui;

use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};
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
    /// Launch terminal UI dashboard
    Tui,
    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// List all discovered skills and their tools
    Skills,
    /// Manage the memory store (search, list, add facts)
    Memory {
        #[command(subcommand)]
        action: MemoryCommands,
    },
    /// List configured channels and their status
    Channels,
    /// Show log configuration info
    Logs,
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// Search memory using full-text search
    Search {
        /// The search query
        query: String,
    },
    /// List all stored facts
    List,
    /// Add a new fact to long-term memory
    Add {
        /// The fact to remember
        fact: String,
    },
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
    #[serde(default)]
    memory: MemoryConfig,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct MemoryConfig {
    #[serde(default = "default_memory_dir")]
    dir: String,
    #[serde(default)]
    diary_enabled: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            dir: default_memory_dir(),
            diary_enabled: true,
        }
    }
}

fn default_memory_dir() -> String {
    "~/.merlionclaw/memory".to_string()
}

#[derive(Debug, Default, serde::Deserialize)]
struct ChannelsConfig {
    #[serde(default)]
    telegram: TelegramChannelConfig,
    #[serde(default)]
    slack: SlackChannelConfig,
    #[serde(default)]
    discord: DiscordChannelConfig,
    #[serde(default)]
    whatsapp: WhatsAppChannelConfig,
    #[serde(default)]
    teams: TeamsChannelConfig,
}

#[derive(Debug, Default, serde::Deserialize)]
struct TeamsChannelConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_teams_app_id_env")]
    app_id_env: String,
    #[serde(default = "default_teams_app_password_env")]
    app_password_env: String,
    #[serde(default)]
    allow_from: Vec<String>,
}

fn default_teams_app_id_env() -> String {
    "TEAMS_APP_ID".to_string()
}

fn default_teams_app_password_env() -> String {
    "TEAMS_APP_PASSWORD".to_string()
}

#[derive(Debug, Default, serde::Deserialize)]
struct WhatsAppChannelConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_whatsapp_token_env")]
    access_token_env: String,
    #[serde(default)]
    phone_number_id: String,
    #[serde(default = "default_whatsapp_verify_token")]
    verify_token: String,
    #[serde(default)]
    allow_from: Vec<String>,
}

fn default_whatsapp_token_env() -> String {
    "WHATSAPP_ACCESS_TOKEN".to_string()
}

fn default_whatsapp_verify_token() -> String {
    "merlionclaw_verify".to_string()
}

#[derive(Debug, Default, serde::Deserialize)]
struct DiscordChannelConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_discord_token_env")]
    bot_token_env: String,
    #[serde(default)]
    allow_from: Vec<String>,
    #[serde(default = "default_true")]
    require_mention: bool,
}

fn default_discord_token_env() -> String {
    "DISCORD_BOT_TOKEN".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, serde::Deserialize)]
struct SlackChannelConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_slack_app_token_env")]
    app_token_env: String,
    #[serde(default = "default_slack_bot_token_env")]
    bot_token_env: String,
    #[serde(default)]
    allow_from: Vec<String>,
    #[serde(default)]
    require_mention: bool,
}

fn default_slack_app_token_env() -> String {
    "SLACK_APP_TOKEN".to_string()
}

fn default_slack_bot_token_env() -> String {
    "SLACK_BOT_TOKEN".to_string()
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

            // Create LLM provider (optional — gateway runs without it)
            let provider = match mclaw_agent::llm::anthropic::AnthropicProvider::from_env(
                &config.agent.providers.anthropic.api_key_env,
            ) {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!(error = %e, "LLM provider not available — chat will be disabled");
                    None
                }
            };

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

            // Register Helm skill handler if available
            match mclaw_skills::helm::HelmSkill::new().await {
                Ok(helm) => {
                    registry.register_handler("helm", Box::new(helm));
                    info!("Helm skill handler registered");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Helm skill not available");
                }
            }

            // Register Istio skill handler if K8s is available
            match mclaw_skills::istio::IstioSkill::new().await {
                Ok(istio) => {
                    registry.register_handler("istio", Box::new(istio));
                    info!("Istio skill handler registered");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Istio skill not available");
                }
            }

            // Note: Loki skill requires config (loki_url) — registered when config is present

            // Register incident skill
            let incident_skill = mclaw_skills::incident::IncidentSkill::new();
            registry.register_handler("incident", Box::new(incident_skill));
            info!("Incident skill handler registered");

            // Initialize memory store
            let memory_dir = shellexpand::tilde(&config.memory.dir).to_string();
            let memory_store = std::sync::Arc::new(
                mclaw_memory::store::MemoryStore::new(std::path::PathBuf::from(&memory_dir)).await?,
            );
            let memory_skill = mclaw_skills::memory::MemorySkill::new(memory_store);
            registry.register_handler("memory", Box::new(memory_skill));
            info!(dir = %memory_dir, "memory system initialized");

            // Create agent (only if LLM provider is available)
            let handler: Option<Arc<dyn mclaw_gateway::server::MessageHandler>> =
                if let Some(provider) = provider {
                    let agent = mclaw_agent::agent::Agent::new(
                        Box::new(provider),
                        config.agent.default_model.clone(),
                    )
                    .with_dispatcher(Box::new(registry));
                    Some(Arc::new(AgentHandler { agent }))
                } else {
                    info!("agent disabled (no LLM provider)");
                    None
                };

            let shutdown = tokio_util::sync::CancellationToken::new();

            // Start gateway with agent
            let gateway_config = config.gateway.clone();
            let handler_clone = handler.clone();
            let gateway_handle = tokio::spawn(async move {
                if let Err(e) =
                    mclaw_gateway::server::start_with_handler(gateway_config, handler_clone)
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

            // Start Slack adapter if enabled
            let slack_handle = if config.channels.slack.enabled {
                let app_token = std::env::var(&config.channels.slack.app_token_env).map_err(
                    |_| anyhow::anyhow!("{} not set", config.channels.slack.app_token_env),
                )?;
                let bot_token = std::env::var(&config.channels.slack.bot_token_env).map_err(
                    |_| anyhow::anyhow!("{} not set", config.channels.slack.bot_token_env),
                )?;

                let slack_config = mclaw_channels::slack::SlackConfig {
                    app_token,
                    bot_token,
                    allow_from: config.channels.slack.allow_from.clone(),
                    require_mention: config.channels.slack.require_mention,
                };

                let adapter = mclaw_channels::slack::SlackAdapter::new(slack_config);
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
                        tracing::error!(error = %e, "Slack adapter error");
                    }
                }))
            } else {
                info!("Slack adapter disabled");
                None
            };

            // Start WhatsApp adapter if enabled
            let whatsapp_handle = if config.channels.whatsapp.enabled {
                let access_token =
                    std::env::var(&config.channels.whatsapp.access_token_env).map_err(|_| {
                        anyhow::anyhow!("{} not set", config.channels.whatsapp.access_token_env)
                    })?;

                let wa_config = mclaw_channels::whatsapp::WhatsAppConfig {
                    access_token,
                    phone_number_id: config.channels.whatsapp.phone_number_id.clone(),
                    verify_token: config.channels.whatsapp.verify_token.clone(),
                    allow_from: config.channels.whatsapp.allow_from.clone(),
                };

                let adapter = mclaw_channels::whatsapp::WhatsAppAdapter::new(wa_config);
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
                        tracing::error!(error = %e, "WhatsApp adapter error");
                    }
                }))
            } else {
                info!("WhatsApp adapter disabled");
                None
            };

            // Start Teams adapter if enabled
            let teams_handle = if config.channels.teams.enabled {
                let app_id = std::env::var(&config.channels.teams.app_id_env).map_err(|_| {
                    anyhow::anyhow!("{} not set", config.channels.teams.app_id_env)
                })?;
                let app_password =
                    std::env::var(&config.channels.teams.app_password_env).map_err(|_| {
                        anyhow::anyhow!("{} not set", config.channels.teams.app_password_env)
                    })?;

                let teams_config = mclaw_channels::teams::TeamsConfig {
                    app_id,
                    app_password,
                    allow_from: config.channels.teams.allow_from.clone(),
                };

                let adapter = mclaw_channels::teams::TeamsAdapter::new(teams_config);
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
                        tracing::error!(error = %e, "Teams adapter error");
                    }
                }))
            } else {
                info!("Teams adapter disabled");
                None
            };

            // Start Discord adapter if enabled
            let discord_handle = if config.channels.discord.enabled {
                let bot_token = std::env::var(&config.channels.discord.bot_token_env).map_err(
                    |_| anyhow::anyhow!("{} not set", config.channels.discord.bot_token_env),
                )?;

                let discord_config = mclaw_channels::discord::DiscordConfig {
                    bot_token,
                    allow_from: config.channels.discord.allow_from.clone(),
                    require_mention: config.channels.discord.require_mention,
                };

                let adapter = mclaw_channels::discord::DiscordAdapter::new(discord_config);
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
                        tracing::error!(error = %e, "Discord adapter error");
                    }
                }))
            } else {
                info!("Discord adapter disabled");
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
            if let Some(handle) = slack_handle {
                handle.abort();
            }
            if let Some(handle) = discord_handle {
                handle.abort();
            }
            if let Some(handle) = whatsapp_handle {
                handle.abort();
            }
            if let Some(handle) = teams_handle {
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
        Commands::Tui => {
            let config = load_config(&cli.config);
            tui::run(&config)?;
        }
        Commands::Completion { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "mclaw",
                &mut std::io::stdout(),
            );
        }
        Commands::Skills => {
            let skills_dir = std::path::Path::new("skills");
            match mclaw_skills::registry::SkillRegistry::discover(skills_dir) {
                Ok(registry) => {
                    println!("{}", registry.skills_summary());
                }
                Err(e) => {
                    println!("Failed to discover skills: {e}");
                }
            }
        }
        Commands::Memory { action } => {
            let config = load_config(&cli.config);
            let memory_dir = shellexpand::tilde(&config.memory.dir).to_string();
            let store = mclaw_memory::store::MemoryStore::new(std::path::PathBuf::from(&memory_dir)).await?;
            match action {
                MemoryCommands::Search { query } => {
                    let hits = store.search(&query, 10).await?;
                    if hits.is_empty() {
                        println!("No results found for: {query}");
                    } else {
                        println!("Search results for \"{query}\":");
                        for hit in &hits {
                            let source = match &hit.source {
                                mclaw_memory::search::MemorySource::Fact => "fact".to_string(),
                                mclaw_memory::search::MemorySource::Diary(d) => format!("diary:{d}"),
                                mclaw_memory::search::MemorySource::Context(s) => format!("ctx:{s}"),
                            };
                            println!("  [{:.2}] [{}] {}", hit.score, source, hit.content);
                        }
                    }
                }
                MemoryCommands::List => {
                    let facts = store.get_facts().await?;
                    if facts.is_empty() {
                        println!("No facts stored.");
                    } else {
                        println!("Stored facts ({}):", facts.len());
                        for fact in &facts {
                            println!("  - {fact}");
                        }
                    }
                }
                MemoryCommands::Add { fact } => {
                    store.add_fact(&fact).await?;
                    println!("Fact added: {fact}");
                }
            }
        }
        Commands::Channels => {
            let config = load_config(&cli.config);
            println!("Configured channels:");
            println!("  Telegram: {}", if config.channels.telegram.enabled { "enabled" } else { "disabled" });
            println!("  Slack: {}", if config.channels.slack.enabled { "enabled" } else { "disabled" });
            println!("  Discord: {}", if config.channels.discord.enabled { "enabled" } else { "disabled" });
            println!("  WhatsApp: {}", if config.channels.whatsapp.enabled { "enabled" } else { "disabled" });
            println!("  Teams: {}", if config.channels.teams.enabled { "enabled" } else { "disabled" });
        }
        Commands::Logs => {
            println!("MerlionClaw logs are emitted to stderr via the tracing framework.");
            println!();
            println!("To control log output, set the MCLAW_LOG environment variable:");
            println!("  MCLAW_LOG=debug mclaw run    # verbose logging");
            println!("  MCLAW_LOG=warn mclaw run     # warnings and errors only");
            println!("  MCLAW_LOG=mclaw_agent=debug  # debug a specific crate");
            println!();
            println!("You can also use the --log-level flag:");
            println!("  mclaw --log-level debug run");
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
