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

fn default_telegram_token_env() -> String {
    "TELEGRAM_BOT_TOKEN".to_string()
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
            let shutdown = tokio_util::sync::CancellationToken::new();

            // Start gateway
            let gateway_config = config.gateway.clone();
            let gateway_handle = tokio::spawn(async move {
                if let Err(e) = mclaw_gateway::server::start(gateway_config).await {
                    tracing::error!(error = %e, "gateway error");
                }
            });

            // Give gateway a moment to bind
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Start Telegram adapter if enabled
            let telegram_handle = if config.channels.telegram.enabled {
                let bot_token = std::env::var(&config.channels.telegram.bot_token_env)
                    .map_err(|_| anyhow::anyhow!("{} not set", config.channels.telegram.bot_token_env))?;

                let telegram_config = mclaw_channels::telegram::TelegramConfig {
                    bot_token,
                    allow_from: config.channels.telegram.allow_from.clone(),
                };

                let adapter = mclaw_channels::telegram::TelegramAdapter::new(telegram_config);
                let gateway_url = format!("ws://{}:{}", config.gateway.host, config.gateway.port);
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

            // Clean up telegram handle if it exists
            if let Some(handle) = telegram_handle {
                handle.abort();
            }
        }
        Commands::Onboard => {
            info!("starting onboard wizard");
            // TODO: guided setup
        }
        Commands::Status => {
            info!("checking status");
            // TODO: query running instance
        }
        Commands::Doctor => {
            info!("running diagnostics");
            let config_path = shellexpand::tilde(&cli.config).to_string();
            if std::path::Path::new(&config_path).exists() {
                info!(path = %config_path, "config file found");
            } else {
                tracing::warn!(path = %config_path, "config file not found");
            }
        }
    }

    Ok(())
}
