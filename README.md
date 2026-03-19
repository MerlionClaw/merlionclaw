# MerlionClaw 🦁

**Infrastructure Agent Runtime** — a personal AI agent written in Rust, optimized for DevOps/SRE workflows with first-class Kubernetes, Helm, Istio, and observability support.

Connect it to **Telegram, Slack, Discord, WhatsApp, or Microsoft Teams**, and it uses LLMs (Anthropic/OpenAI) to execute infrastructure tasks on your behalf — with a capability-based permission engine so you stay in control.

## Why MerlionClaw?

- **Single static binary** — ~19MB stripped, <10MB idle memory
- **Rust** — memory safe, no GC pauses
- **5 chat platforms** — Telegram, Slack, Discord, WhatsApp, Microsoft Teams
- **28 built-in tools** — K8s, Helm, Istio, Loki/Grafana, incident response, memory
- **Capability-based permissions** — skills declare what they need, the engine grants or denies
- **WASM skill sandbox** — run third-party skills safely (via wasmtime)
- **MCP compatible** — connect to any MCP server (GitHub, Sentry, Jira, etc.)
- **Persistent memory** — tantivy full-text search, facts, daily diary
- **Incident response** — webhook intake from Alertmanager/PagerDuty, auto-triage
- **K8s-native** — run as a sidecar or standalone

## Architecture

```
User (Telegram / Slack / Discord / WhatsApp / Teams)
       │
       ▼
┌──────────────────────────────┐
│   Gateway (axum + WS)        │
│   :18789                     │
│   /ws          WebSocket     │
│   /health      Health check  │
│   /webhook/*   Alert intake  │
└───────────┬──────────────────┘
            │
     ┌──────┴──────┐
     ▼              ▼
┌──────────┐  ┌───────────────┐
│  Agent   │  │  Permission   │
│  Loop    │  │  Engine       │
└────┬─────┘  └───────────────┘
     │
     ├──► LLM Provider (Anthropic / OpenAI)
     ├──► SKILL.md Skills (6 built-in)
     ├──► WASM Skills (wasmtime sandbox)
     ├──► MCP Bridge (stdio / SSE)
     └──► Memory (tantivy search)
              │
              ▼
     Infrastructure APIs
     ├── K8s API (kube-rs)
     ├── Helm CLI
     ├── Istio CRDs
     ├── Loki / Grafana HTTP API
     ├── PagerDuty / Alertmanager webhooks
     └── Any MCP server
```

## Quick Start

```bash
# Build
cargo build --release

# Check setup
./target/release/mclaw doctor

# Run (gateway + agent)
ANTHROPIC_API_KEY=sk-xxx ./target/release/mclaw run
```

## Chat Platforms

| Platform | Transport | Env Vars |
|----------|-----------|----------|
| Telegram | Long polling | `TELEGRAM_BOT_TOKEN` |
| Slack | Socket Mode WS | `SLACK_APP_TOKEN` + `SLACK_BOT_TOKEN` |
| Discord | Gateway WS | `DISCORD_BOT_TOKEN` |
| WhatsApp | Webhook + Cloud API | `WHATSAPP_ACCESS_TOKEN` |
| Microsoft Teams | Webhook + Bot Framework | `TEAMS_APP_ID` + `TEAMS_APP_PASSWORD` |

See [Chat Platforms Roadmap](docs/CHAT-PLATFORMS-ROADMAP.md) for the full list of planned platforms.

## Skills & Tools

| Skill | Tools | Description |
|-------|-------|-------------|
| **k8s** | 4 | List pods/deployments, get logs, describe pods |
| **helm** | 7 | List/status/history/values/upgrade/rollback/uninstall releases |
| **istio** | 4 | VirtualServices, DestinationRules, Gateways |
| **loki** | 5 | LogQL queries, label listing, log tailing |
| **incident** | 4 | Alert triage, acknowledge, resolve, auto-diagnose |
| **memory** | 4 | Store/search/list/remove long-term facts |

**Total: 28 tools** exposed to the LLM.

### Special Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/status` | Gateway status + tool count |
| `/skills` | List registered skills and tools |
| `/reset` | Clear conversation context |

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
allow_from = ["your_user_id"]

[channels.slack]
enabled = false
app_token_env = "SLACK_APP_TOKEN"
bot_token_env = "SLACK_BOT_TOKEN"

[channels.discord]
enabled = false
bot_token_env = "DISCORD_BOT_TOKEN"
require_mention = true

[channels.whatsapp]
enabled = false
access_token_env = "WHATSAPP_ACCESS_TOKEN"
phone_number_id = "your_phone_number_id"

[channels.teams]
enabled = false
app_id_env = "TEAMS_APP_ID"
app_password_env = "TEAMS_APP_PASSWORD"

[permissions.default]
policy = "deny"

[permissions.skills.k8s]
allow = ["k8s:read", "k8s:write"]
require_approval = ["k8s:write"]

[permissions.skills.helm]
allow = ["k8s:read", "exec:helm"]
require_approval = ["exec:helm"]

[memory]
dir = "~/.merlionclaw/memory"
diary_enabled = true
```

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `mclaw` | CLI binary (clap) — run, doctor, status, onboard |
| `mclaw-gateway` | WebSocket gateway + HTTP server + webhook endpoints (axum) |
| `mclaw-agent` | Agent loop + LLM abstraction (Anthropic client + streaming) |
| `mclaw-skills` | SKILL.md parser, skill registry, K8s/Helm/Istio/Loki/incident/memory handlers |
| `mclaw-channels` | Chat adapters — Telegram, Slack, Discord, WhatsApp, Teams |
| `mclaw-memory` | Persistent memory with tantivy full-text search |
| `mclaw-permissions` | Capability-based permission engine (deny/allow/require_approval) |
| `mclaw-wasm` | WASM skill runtime (wasmtime + WASI sandbox) |
| `mclaw-mcp` | MCP client bridge (stdio + SSE transport) |

## Webhook Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Health check + session count |
| `GET /ws` | WebSocket for channel adapters |
| `POST /webhook/alertmanager` | Alertmanager alert intake |
| `POST /webhook/pagerduty` | PagerDuty incident intake |
| `POST /webhook/whatsapp` | WhatsApp Cloud API messages |
| `GET /webhook/whatsapp` | WhatsApp webhook verification |
| `POST /webhook/teams` | Microsoft Teams Bot Framework activities |

## Docker

```bash
docker build -t merlionclaw .
docker run -e ANTHROPIC_API_KEY=sk-xxx -p 18789:18789 merlionclaw
```

## K8s Sidecar

```dockerfile
FROM gcr.io/distroless/cc-debian12
COPY target/release/mclaw /mclaw
ENTRYPOINT ["/mclaw", "run", "--sidecar"]
```

## License

MIT
