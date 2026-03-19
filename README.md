# MerlionClaw 🦁

**Infrastructure Agent Runtime** — a personal AI agent written in Rust, optimized for DevOps/SRE workflows with first-class Kubernetes, Helm, Istio, and observability support.

Connect it to Telegram or Slack, and it uses LLMs (Anthropic, OpenAI) to execute infrastructure tasks on your behalf — with a capability-based permission engine so you stay in control.

## Why MerlionClaw?

- **Single static binary** — <15MB, <10MB idle memory
- **Rust** — memory safe, no GC pauses
- **Capability-based permissions** — skills declare what they need, the engine grants or denies
- **WASM skill sandbox** — run untrusted skills safely (via wasmtime)
- **K8s-native** — run as a sidecar or CRD operator
- **MCP compatible** — reuse the MCP server ecosystem

## Architecture

```
User (Telegram / Slack / CLI)
       │
       ▼
┌─────────────────────────┐
│   Gateway (axum + WS)   │
│   :18789                │
└───────────┬─────────────┘
            │
     ┌──────┴──────┐
     ▼              ▼
┌──────────┐  ┌───────────────┐
│  Agent   │  │  Permission   │
│  Loop    │  │  Engine       │
└────┬─────┘  └───────────────┘
     │
     ├──► LLM Provider (Anthropic / OpenAI)
     ├──► SKILL.md Skills
     ├──► WASM Skills (sandboxed)
     └──► MCP Bridge
              │
              ▼
     Infrastructure APIs
     ├── K8s API (kube-rs)
     ├── Helm CLI
     ├── Istio CRDs
     ├── Loki / Grafana
     ├── PagerDuty / OpsGenie
     └── Terraform CLI
```

## Quick Start

```bash
# Build
cargo build --release

# Guided setup
./target/release/mclaw onboard

# Run
./target/release/mclaw run

# Check config & connectivity
./target/release/mclaw doctor
```

## Configuration

All config lives in `~/.merlionclaw/config.toml`. See [`config/default.toml`](config/default.toml) for the full template.

```toml
[gateway]
host = "127.0.0.1"
port = 18789

[agent]
default_model = "claude-sonnet-4-20250514"

[channels.telegram]
enabled = true
bot_token_env = "TELEGRAM_BOT_TOKEN"

[permissions.default]
policy = "deny"

[permissions.skills.k8s]
allow = ["k8s:read", "k8s:write"]
```

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `mclaw` | CLI binary (clap) |
| `mclaw-gateway` | WebSocket gateway + HTTP server (axum) |
| `mclaw-agent` | Agent loop + LLM abstraction |
| `mclaw-skills` | SKILL.md parser + skill registry |
| `mclaw-channels` | Chat platform adapters (Telegram, Slack) |
| `mclaw-memory` | Persistent memory with full-text search |
| `mclaw-permissions` | Capability-based permission engine |

## Docker

```bash
docker build -t merlionclaw .
docker run -v ~/.merlionclaw:/data merlionclaw run
```

## K8s Sidecar

```dockerfile
FROM gcr.io/distroless/static-debian12
COPY target/release/mclaw /mclaw
ENTRYPOINT ["/mclaw", "run", "--sidecar"]
```

## License

MIT
