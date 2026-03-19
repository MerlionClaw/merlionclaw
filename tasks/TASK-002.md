# TASK-002: Gateway Core (axum WebSocket Server + Typed Protocol)

## Objective
Implement the WebSocket gateway that acts as the control plane for all channels and the agent.

## Dependencies
- TASK-001 must be complete

## Steps

### 1. Define the message protocol (mclaw-gateway/src/protocol.rs)

All WebSocket frames are JSON. Define serde types:

```rust
/// Messages FROM channels/clients TO gateway
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InboundMessage {
    /// User sent a chat message
    #[serde(rename = "chat")]
    Chat {
        session_id: String,
        channel: ChannelKind,
        sender: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
    },
    /// Channel adapter registering itself
    #[serde(rename = "register_channel")]
    RegisterChannel {
        channel: ChannelKind,
    },
    /// Heartbeat
    #[serde(rename = "ping")]
    Ping,
}

/// Messages FROM gateway TO channels/clients
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutboundMessage {
    /// Agent response to send to user
    #[serde(rename = "reply")]
    Reply {
        session_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use: Option<serde_json::Value>,
    },
    /// Streaming chunk
    #[serde(rename = "stream_chunk")]
    StreamChunk {
        session_id: String,
        delta: String,
    },
    /// Stream complete
    #[serde(rename = "stream_end")]
    StreamEnd {
        session_id: String,
    },
    /// Heartbeat response
    #[serde(rename = "pong")]
    Pong,
    /// Error
    #[serde(rename = "error")]
    Error {
        session_id: Option<String>,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Telegram,
    Slack,
    Cli,
    Webchat,
}
```

### 2. Implement session manager (mclaw-gateway/src/session.rs)

- `SessionManager` holds a `DashMap<String, Session>` (or `HashMap` behind `Arc<RwLock<>>`)
- Session keyed by `"{channel}:{sender}"` (e.g., `"telegram:+6512345678"`)
- Each session tracks: conversation history, last active timestamp, channel kind
- Sessions expire after configurable timeout (default 24h)

### 3. Implement WS server (mclaw-gateway/src/server.rs)

Using axum:

```rust
pub async fn start(config: GatewayConfig) -> anyhow::Result<()> {
    let state = AppState::new(config);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health))
        .with_state(state);

    let addr = SocketAddr::from((config.host, config.port));
    tracing::info!("Gateway listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

The WS handler should:
1. Accept WS upgrade
2. Spawn a read task and write task
3. Read task: parse JSON → InboundMessage → route to agent
4. Write task: receive OutboundMessage from agent → serialize → send

### 4. Gateway config (mclaw-gateway/src/config.rs)

```rust
#[derive(Debug, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_host")]
    pub host: IpAddr,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_session_timeout")]
    pub session_timeout_secs: u64,
}
```

### 5. Wire it up in mclaw CLI

The `run` subcommand should:
1. Load config from TOML
2. Initialize tracing
3. Start the gateway via `mclaw_gateway::server::start(config).await`

## Validation

```bash
cargo run -- run &
# In another terminal:
websocat ws://127.0.0.1:18789/ws
# Type: {"type":"ping"}
# Should receive: {"type":"pong"}

# Type: {"type":"chat","session_id":"test","channel":"cli","sender":"larry","content":"hello"}
# Should receive: {"type":"error","session_id":"test","message":"agent not configured"}
# (expected - agent comes in TASK-003)

# Health check:
curl http://127.0.0.1:18789/health
# Should return: {"status":"ok","sessions":0}
```

## Output

A running WebSocket gateway that accepts connections, parses typed messages, manages sessions, and responds with pong/error. Ready for agent integration.
