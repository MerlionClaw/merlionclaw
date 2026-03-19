# TASK-005: Telegram Channel Adapter

## Objective
Implement the Telegram bot adapter that connects Telegram messages to the MerlionClaw gateway via WebSocket.

## Dependencies
- TASK-002 must be complete (gateway WS server)

## Steps

### 1. Define channel trait (mclaw-channels/src/traits.rs)

```rust
#[async_trait]
pub trait ChannelAdapter: Send + Sync + 'static {
    /// Channel identifier
    fn kind(&self) -> ChannelKind;

    /// Start the adapter — connect to gateway WS and begin listening
    /// This runs forever (or until shutdown signal)
    async fn start(&self, gateway_url: String, shutdown: CancellationToken) -> Result<()>;
}
```

### 2. Implement Telegram adapter (mclaw-channels/src/telegram.rs)

Use `teloxide` crate with long polling (simpler than webhook for personal use):

```rust
pub struct TelegramAdapter {
    config: TelegramConfig,
}

pub struct TelegramConfig {
    pub bot_token: String,
    pub allow_from: Vec<String>,  // allowed user IDs or usernames
}
```

The adapter should:

1. Create teloxide Bot instance
2. Connect to gateway WS as a client (`tokio-tungstenite`)
3. Send `RegisterChannel { channel: Telegram }` on connect
4. Set up teloxide message handler:
   - Receive message from user
   - Check if sender is in `allow_from` (security!)
   - Build `InboundMessage::Chat` with:
     - `session_id`: `"telegram:{user_id}"`
     - `sender`: user's display name or username
     - `content`: message text
   - Send to gateway via WS
5. Listen for `OutboundMessage` from gateway WS:
   - `Reply` → send text to Telegram chat via `bot.send_message(chat_id, content)`
   - `StreamChunk` → collect chunks, send as a single message on `StreamEnd`
     (Telegram doesn't support real streaming, so buffer and send)
   - `Error` → send error message to user

### 3. Handle Telegram-specific features

- **Markdown formatting**: Convert agent responses to Telegram MarkdownV2 (escape special chars)
- **Long messages**: Split messages >4096 chars into multiple sends
- **Reply context**: If user replies to a bot message, include `reply_to` in InboundMessage
- **File/image handling** (stretch): If agent response contains image URLs, send as photo

### 4. Allowlist security

Critical: The bot must only respond to allowed users.

```rust
fn is_allowed(&self, user: &teloxide::types::User) -> bool {
    let user_id = user.id.to_string();
    let username = user.username.as_deref().unwrap_or("");
    self.config.allow_from.iter().any(|a| {
        a == &user_id || a == username || a == &format!("@{}", username)
    })
}
```

If not allowed → respond with "Unauthorized. Contact the admin to get access." and log a warning.

### 5. Wire into CLI

Update `mclaw/src/main.rs` `run` command to:
1. Start gateway
2. Start telegram adapter (if enabled in config)
3. Both run concurrently via `tokio::select!` or `JoinSet`

### 6. Graceful shutdown

Handle SIGINT/SIGTERM:
- Cancel the CancellationToken
- Telegram adapter stops polling
- Gateway closes WS connections
- Clean exit

## Validation

```bash
# Set up a Telegram bot via @BotFather, get token
export TELEGRAM_BOT_TOKEN=xxx
export ANTHROPIC_API_KEY=xxx

# Update config.toml:
# [channels.telegram]
# enabled = true
# allow_from = ["your_telegram_user_id"]

cargo run -- run

# Send a message to your bot in Telegram
# Bot should respond with LLM-generated text
# Try: "list pods in kube-system" (if K8s skill is available)
```

## Output

A working Telegram bot that relays messages between Telegram and the MerlionClaw agent. Supports allowlist security, markdown formatting, and graceful shutdown.
