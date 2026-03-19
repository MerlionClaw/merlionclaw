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
            info!(config = %cli.config, "loading config");

            let gateway_config = mclaw_gateway::config::GatewayConfig::default();
            mclaw_gateway::server::start(gateway_config).await?;
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
