# TASK-001: Workspace Scaffold + CLI Skeleton

## Objective
Set up the Cargo workspace with all crate stubs and a working CLI binary.

## Steps

### 1. Create workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "mclaw",
    "mclaw-gateway",
    "mclaw-agent",
    "mclaw-skills",
    "mclaw-channels",
    "mclaw-memory",
    "mclaw-permissions",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/merlionclaw/merlionclaw"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
anyhow = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
axum = { version = "0.8", features = ["ws"] }
tokio-tungstenite = "0.26"
clap = { version = "4", features = ["derive"] }
```

### 2. Create each crate with `cargo init`

For each member, create `Cargo.toml` with `workspace.package` inheritance and a minimal `src/lib.rs` (or `src/main.rs` for `mclaw`).

### 3. Implement mclaw CLI (mclaw/src/main.rs)

Use clap derive to create subcommands:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mclaw")]
#[command(about = "MerlionClaw - Infrastructure Agent Runtime 🦁")]
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
```

### 4. Set up tracing

Initialize tracing-subscriber in main with env-filter support:
- `MCLAW_LOG` env var or `--log-level` flag
- Default format: `2026-03-19T10:00:00Z INFO mclaw_gateway::server: listening on 127.0.0.1:18789`

### 5. Create default config template

Create `config/default.toml` with the config structure from CLAUDE.md.

### 6. Create stub lib.rs for all library crates

Each lib.rs should have a doc comment explaining the crate's purpose and a placeholder public type or function.

## Validation

```bash
cargo check                  # all crates compile
cargo clippy                 # no warnings
cargo run -- --help          # shows CLI help
cargo run -- run --help      # shows run subcommand help
cargo run -- doctor          # prints "config not found" (expected)
```

## Output

A compilable workspace where `cargo run -- --help` works and all crates have their skeleton ready for implementation.
