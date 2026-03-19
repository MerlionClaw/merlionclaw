# MerlionClaw 🦁

## What is this?

MerlionClaw is an **Infrastructure Agent Runtime** written in Rust. It is a personal AI agent (similar to OpenClaw) that connects to chat platforms (Telegram, Slack) and uses LLMs to execute tasks — but specifically optimized for DevOps/SRE workflows with first-class Kubernetes, Helm, Istio, and observability support.

**Key differentiators from OpenClaw:**
- Single static binary, <15MB, <10MB idle memory (vs OpenClaw's Node.js ~300MB)
- Rust + memory safety, no GC pauses
- Capability-based permission engine (skills declare required permissions)
- WASM skill sandbox via wasmtime (Phase 3)
- K8s-native: can run as sidecar or CRD operator
- MCP client compatible (reuse OpenClaw MCP server ecosystem)

## Architecture

```
User (Telegram/Slack/CLI/WebChat)
       │
       ▼
┌─────────────────────────┐
│   Gateway (axum + WS)   │  ← mclaw-gateway crate
│   WebSocket control      │
│   plane on :18789        │
└───────────┬─────────────┘
            │
     ┌──────┴──────┐
     ▼              ▼
┌──────────┐  ┌───────────────┐
│ Agent    │  │ Permission    │  ← mclaw-agent, mclaw-permissions
│ Loop     │  │ Engine        │
└────┬─────┘  └───────────────┘
     │
     ├──► LLM Provider (Anthropic/OpenAI)  ← mclaw-agent
     │
     ├──► SKILL.md Skills (parsed)         ← mclaw-skills
     ├──► WASM Skills (wasmtime sandbox)   ← mclaw-wasm (Phase 3)
     └──► MCP Bridge (SSE client)          ← mclaw-mcp (Phase 3)
              │
              ▼
     Infrastructure APIs
     ├── K8s API (kube-rs)
     ├── Helm CLI
     ├── Istio CRDs
     ├── Loki/Grafana HTTP API
     ├── PagerDuty/OpsGenie
     └── Terraform CLI
```

## Workspace Structure

```
merlionclaw/
├── Cargo.toml                 # workspace manifest
├── CLAUDE.md                  # this file
├── README.md
├── .cargo/config.toml         # build profile, target config
│
├── mclaw/                     # CLI binary crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs            # clap CLI, subcommands: run, onboard, status, doctor
│
├── mclaw-gateway/             # WebSocket gateway + HTTP server
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── server.rs          # axum router + WS upgrade
│       ├── session.rs         # per-sender session management
│       ├── protocol.rs        # typed JSON message protocol (serde)
│       └── config.rs          # gateway config (TOML)
│
├── mclaw-agent/               # Agent loop + LLM abstraction
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── loop.rs            # main agent loop: receive msg → think → act → respond
│       ├── llm/
│       │   ├── mod.rs         # LlmProvider trait
│       │   ├── anthropic.rs   # Anthropic Messages API client
│       │   └── openai.rs      # OpenAI Chat Completions client
│       ├── tool.rs            # tool/function calling protocol
│       └── context.rs         # conversation context management
│
├── mclaw-skills/              # Built-in skill implementations
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── registry.rs        # skill discovery + registration
│       ├── parser.rs          # SKILL.md parser (yaml frontmatter + markdown body)
│       ├── k8s.rs             # Kubernetes skill (kube-rs)
│       ├── helm.rs            # Helm skill (CLI wrapper)
│       ├── istio.rs           # Istio skill (CRD via kube-rs)
│       └── loki.rs            # Loki/Grafana skill (HTTP API)
│
├── mclaw-channels/            # Chat platform adapters
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── traits.rs          # Channel trait: send/receive/format
│       ├── telegram.rs        # teloxide-based Telegram bot
│       └── slack.rs           # Slack Socket Mode / Events API
│
├── mclaw-memory/              # Persistent memory system
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── store.rs           # Markdown file-based storage
│       ├── search.rs          # tantivy full-text search
│       └── diary.rs           # daily diary + long-term facts
│
├── mclaw-permissions/         # Capability-based permission engine
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── capability.rs      # Permission enum: Fs, Net, K8s, Exec, Helm, etc.
│       ├── policy.rs          # Policy evaluation: skill declares needs, engine grants/denies
│       └── config.rs          # per-skill permission config
│
├── skills/                    # SKILL.md files (user-facing skill definitions)
│   ├── k8s/SKILL.md
│   ├── helm/SKILL.md
│   ├── istio/SKILL.md
│   └── loki/SKILL.md
│
├── config/
│   └── default.toml           # default config template
│
└── tests/
    ├── integration/
    └── e2e/
```

## Tech Stack & Crate Choices

| Purpose | Crate | Why |
|---------|-------|-----|
| Async runtime | `tokio` | De facto standard, required by kube-rs and axum |
| HTTP/WS server | `axum` | Tower middleware ecosystem, shares tokio with kube-rs |
| WebSocket | `tokio-tungstenite` | Async WS, used by axum WS upgrade |
| CLI | `clap` (derive) | Standard Rust CLI framework |
| Serialization | `serde` + `serde_json` + `toml` | Config (TOML), protocol (JSON) |
| Logging | `tracing` + `tracing-subscriber` | Structured logging, spans for request tracing |
| HTTP client | `reqwest` | LLM API calls, Grafana/Loki API, webhook sends |
| K8s client | `kube` + `kube-runtime` + `k8s-openapi` | Native async K8s API, CRD support for Istio |
| Telegram | `teloxide` | Most mature Rust Telegram bot framework |
| Slack | `slack-morphism` | Typed Slack API client |
| Full-text search | `tantivy` | Rust-native Lucene-like, for memory search |
| Markdown parsing | `pulldown-cmark` | SKILL.md body parsing |
| YAML parsing | `serde_yaml` | SKILL.md frontmatter parsing |
| UUID | `uuid` | Session IDs, message IDs |
| Time | `chrono` | Timestamps, log time ranges |
| Streaming | `async-stream` + `futures-core` | LLM streaming responses |
| WASM runtime | `wasmtime` + `wasmtime-wasi` | Phase 3: WASM skill sandbox |

## Code Style & Conventions

- **Error handling:** Use `thiserror` for library errors, `anyhow` in the binary crate. All public APIs return `Result<T, Error>`.
- **Async:** Everything is async. Use `tokio::spawn` for concurrent tasks, not `std::thread`.
- **Naming:** snake_case for files/functions, CamelCase for types. Crate names: `mclaw-*`.
- **Module structure:** One file per logical concern. Split when a file exceeds ~400 LOC.
- **Tests:** Unit tests in the same file (`#[cfg(test)] mod tests`). Integration tests in `tests/`.
- **Documentation:** `///` doc comments on all public items. Include usage examples in doc comments.
- **No unwrap():** Use `?` operator or `.expect("descriptive message")` only in main/tests.
- **Feature flags:** Use Cargo features for optional skills (e.g., `k8s`, `helm`, `istio`, `loki`).
- **Config:** All config via TOML. Environment variable overrides with `MCLAW_` prefix.
- **Logging:** Use `tracing::info!`, `debug!`, `warn!`, `error!` — never `println!`.

## How the Agent Loop Works

```
1. Channel adapter receives message from user
2. Channel sends normalized Message to Gateway via WS
3. Gateway routes to Agent based on session
4. Agent builds context:
   - System prompt (identity + available tools)
   - Memory (relevant facts from tantivy search)
   - Conversation history
   - Available skills → converted to LLM tool definitions
5. Agent calls LLM with context
6. LLM responds with either:
   a. Text → send back to user via channel
   b. Tool call → Permission Engine checks capability → execute skill → feed result back to LLM → goto 5
7. Agent updates memory with conversation summary
```

## LLM Tool Calling Protocol

Skills are exposed to the LLM as tools. Example for the K8s skill:

```json
{
  "name": "k8s_list_pods",
  "description": "List pods in a Kubernetes namespace",
  "input_schema": {
    "type": "object",
    "properties": {
      "namespace": { "type": "string", "description": "K8s namespace (default: current context)" },
      "label_selector": { "type": "string", "description": "Label selector (e.g., app=nginx)" },
      "field_selector": { "type": "string", "description": "Field selector (e.g., status.phase=Running)" }
    }
  }
}
```

The agent loop converts LLM tool_use responses into skill invocations, checks permissions, executes, and feeds tool_result back.

## Permission Model

Each skill declares required capabilities in its SKILL.md frontmatter:

```yaml
permissions:
  - k8s:read           # can list/get K8s resources
  - k8s:write          # can create/update/delete
  - exec:helm          # can invoke helm CLI
  - net:grafana        # can call Grafana API
```

The permission engine checks these against the user's policy config before allowing execution. Default policy: deny all, require explicit grant.

## Config Format (TOML)

```toml
[gateway]
host = "127.0.0.1"
port = 18789

[agent]
default_model = "claude-sonnet-4-20250514"

[agent.providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"

[agent.providers.openai]
api_key_env = "OPENAI_API_KEY"

[channels.telegram]
enabled = true
bot_token_env = "TELEGRAM_BOT_TOKEN"
allow_from = ["+6512345678"]

[channels.slack]
enabled = false
app_token_env = "SLACK_APP_TOKEN"
bot_token_env = "SLACK_BOT_TOKEN"

[permissions.default]
policy = "deny"

[permissions.skills.k8s]
allow = ["k8s:read", "k8s:write"]

[permissions.skills.helm]
allow = ["k8s:read", "exec:helm"]
require_approval = ["exec:helm"]  # ask user before running helm install/upgrade

[memory]
dir = "~/.merlionclaw/memory"
diary_enabled = true
```

## Build & Run

```bash
# Development
cargo build
cargo run -- run                    # start gateway + agent
cargo run -- onboard                # guided setup wizard
cargo run -- status                 # show running status
cargo run -- doctor                 # check config + connectivity

# Release (single static binary)
cargo build --release
# Binary at: target/release/mclaw (~15MB)

# Docker
docker build -t merlionclaw .
docker run -v ~/.merlionclaw:/data merlionclaw run

# K8s sidecar (copy binary into distroless image)
FROM gcr.io/distroless/static-debian12
COPY target/release/mclaw /mclaw
ENTRYPOINT ["/mclaw", "run", "--sidecar"]
```

## Implementation Priority

Work on tasks in this order. Each task has its own file in `tasks/`.

1. **TASK-001**: Workspace scaffold + CLI skeleton
2. **TASK-002**: Gateway core (axum WS server + typed protocol)
3. **TASK-003**: LLM abstraction layer (Anthropic client + streaming)
4. **TASK-004**: Skill engine v1 (SKILL.md parser + tool dispatch)
5. **TASK-005**: Telegram channel adapter
6. **TASK-006**: End-to-end MVP (Telegram → Gateway → Agent → K8s skill → response)
7. **TASK-007**: Memory system
8. **TASK-008**: Permission engine
9. **TASK-009**: Helm + Istio + Loki skills
10. **TASK-010**: Slack adapter
11. **TASK-011**: Incident response skill
12. **TASK-012**: WASM skill runtime
13. **TASK-013**: MCP bridge

Always read the relevant task file before starting work. Run `cargo check` and `cargo clippy` after each change. Run `cargo test` before committing.
