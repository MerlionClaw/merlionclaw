//! Discord bot adapter using serenity.

#[cfg(feature = "discord")]
mod inner {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use mclaw_gateway::protocol::{ChannelKind, InboundMessage, OutboundMessage};
    use serenity::all::*;
    use tokio::sync::{mpsc, RwLock};
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    use tokio_util::sync::CancellationToken;
    use tracing::{debug, error, info, warn};

    use crate::traits::ChannelAdapter;

    /// Discord channel configuration.
    #[derive(Debug, Clone)]
    pub struct DiscordConfig {
        /// Bot token from Discord Developer Portal.
        pub bot_token: String,
        /// Allowed user IDs (empty = allow all).
        pub allow_from: Vec<String>,
        /// Whether to require @mention in guild channels.
        pub require_mention: bool,
    }

    /// Discord bot adapter.
    pub struct DiscordAdapter {
        config: DiscordConfig,
    }

    impl DiscordAdapter {
        /// Create a new Discord adapter.
        pub fn new(config: DiscordConfig) -> Self {
            Self { config }
        }
    }

    /// Split a message into chunks that fit Discord's 2000 char limit.
    fn split_message(text: &str, max_len: usize) -> Vec<String> {
        if text.len() <= max_len {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut remaining = text;

        while !remaining.is_empty() {
            if remaining.len() <= max_len {
                chunks.push(remaining.to_string());
                break;
            }

            let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);
            chunks.push(remaining[..split_at].to_string());
            remaining = remaining[split_at..].trim_start_matches('\n');
        }

        chunks
    }

    /// Shared state passed to the serenity event handler.
    struct Handler {
        gateway_tx: mpsc::Sender<String>,
        reply_map: Arc<RwLock<HashMap<String, ChannelId>>>,
        allow_from: Vec<String>,
        require_mention: bool,
        bot_user_id: RwLock<Option<UserId>>,
    }

    #[async_trait]
    impl EventHandler for Handler {
        async fn ready(&self, _ctx: Context, ready: Ready) {
            info!(user = %ready.user.name, "Discord bot connected");
            *self.bot_user_id.write().await = Some(ready.user.id);
        }

        async fn message(&self, _ctx: Context, msg: Message) {
            // Skip bot messages
            if msg.author.bot {
                return;
            }

            let user_id = msg.author.id.to_string();

            // Check allowlist
            if !self.allow_from.is_empty()
                && !self.allow_from.iter().any(|a| a == &user_id)
            {
                debug!(user = %user_id, "Discord user not in allowlist");
                return;
            }

            let mut content = msg.content.clone();

            // In guild channels, check for @mention if required
            if msg.guild_id.is_some() && self.require_mention {
                let bot_id = self.bot_user_id.read().await;
                if let Some(bot_id) = *bot_id {
                    let mention = format!("<@{}>", bot_id);
                    let mention_nick = format!("<@!{}>", bot_id);
                    if content.contains(&mention) || content.contains(&mention_nick) {
                        content = content
                            .replace(&mention, "")
                            .replace(&mention_nick, "")
                            .trim()
                            .to_string();
                    } else {
                        return; // Not mentioned, ignore
                    }
                }
            }

            if content.is_empty() {
                return;
            }

            let session_id = format!("discord:{}", msg.author.id);
            let sender = msg.author.name.clone();

            // Map session to channel for replies
            self.reply_map
                .write()
                .await
                .insert(session_id.clone(), msg.channel_id);

            let inbound = InboundMessage::Chat {
                session_id,
                channel: ChannelKind::Discord,
                sender,
                content,
                reply_to: msg
                    .referenced_message
                    .as_ref()
                    .map(|r| r.content.clone()),
            };

            let json = serde_json::to_string(&inbound).unwrap();
            if let Err(e) = self.gateway_tx.send(json).await {
                error!(error = %e, "failed to queue Discord message");
            }
        }
    }

    #[async_trait]
    impl ChannelAdapter for DiscordAdapter {
        fn kind(&self) -> ChannelKind {
            ChannelKind::Discord
        }

        async fn start(
            &self,
            gateway_url: String,
            shutdown: CancellationToken,
        ) -> anyhow::Result<()> {
            info!("starting Discord adapter");

            // Connect to gateway WS
            let ws_url = format!("{gateway_url}/ws");
            let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
                .await
                .map_err(|e| anyhow::anyhow!("failed to connect to gateway: {e}"))?;

            let (mut ws_write, mut ws_read) = ws_stream.split();
            info!(url = %ws_url, "connected to gateway");

            // Register with gateway
            let register = serde_json::to_string(&InboundMessage::RegisterChannel {
                channel: ChannelKind::Discord,
            })?;
            ws_write.send(WsMessage::Text(register.into())).await?;

            let reply_map: Arc<RwLock<HashMap<String, ChannelId>>> =
                Arc::new(RwLock::new(HashMap::new()));

            let (gateway_tx, mut gateway_rx) = mpsc::channel::<String>(100);

            let ws_write = Arc::new(tokio::sync::Mutex::new(ws_write));

            // Task: forward messages to gateway WS
            let ws_write_fwd = ws_write.clone();
            let fwd_task = tokio::spawn(async move {
                while let Some(json) = gateway_rx.recv().await {
                    let mut writer = ws_write_fwd.lock().await;
                    if let Err(e) = writer.send(WsMessage::Text(json.into())).await {
                        error!(error = %e, "failed to send to gateway");
                    }
                }
            });

            // Task: receive replies from gateway and send to Discord
            let bot_token = self.config.bot_token.clone();
            let reply_map_out = reply_map.clone();
            let shutdown_out = shutdown.clone();
            let reply_task = tokio::spawn(async move {
                let http = serenity::http::Http::new(&bot_token);

                loop {
                    tokio::select! {
                        msg = ws_read.next() => {
                            let text = match msg {
                                Some(Ok(WsMessage::Text(t))) => t,
                                Some(Err(e)) => { warn!(error = %e, "gateway error"); break; }
                                None => break,
                                _ => continue,
                            };
                            let outbound: OutboundMessage = match serde_json::from_str(&text) {
                                Ok(m) => m,
                                Err(_) => continue,
                            };
                            match outbound {
                                OutboundMessage::Reply { session_id, content, .. } => {
                                    let map = reply_map_out.read().await;
                                    if let Some(&channel_id) = map.get(&session_id) {
                                        for chunk in split_message(&content, 2000) {
                                            if let Err(e) = channel_id
                                                .say(&http, &chunk)
                                                .await
                                            {
                                                error!(error = %e, "failed to send Discord message");
                                            }
                                        }
                                    }
                                }
                                OutboundMessage::Error { session_id: Some(sid), message, .. } => {
                                    let map = reply_map_out.read().await;
                                    if let Some(&channel_id) = map.get(&sid) {
                                        let _ = channel_id
                                            .say(&http, format!("⚠️ Error: {message}"))
                                            .await;
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ = shutdown_out.cancelled() => break,
                    }
                }
            });

            // Build serenity client
            let intents = GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::DIRECT_MESSAGES
                | GatewayIntents::MESSAGE_CONTENT;

            let handler = Handler {
                gateway_tx,
                reply_map,
                allow_from: self.config.allow_from.clone(),
                require_mention: self.config.require_mention,
                bot_user_id: RwLock::new(None),
            };

            let mut client = Client::builder(&self.config.bot_token, intents)
                .event_handler(handler)
                .await
                .map_err(|e| anyhow::anyhow!("failed to create Discord client: {e}"))?;

            // Run Discord client until shutdown
            tokio::select! {
                result = client.start() => {
                    if let Err(e) = result {
                        error!(error = %e, "Discord client error");
                    }
                }
                _ = shutdown.cancelled() => {
                    info!("Discord adapter shutting down");
                    client.shard_manager.shutdown_all().await;
                }
            }

            fwd_task.abort();
            reply_task.abort();

            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_split_message_short() {
            let chunks = split_message("short", 2000);
            assert_eq!(chunks.len(), 1);
        }

        #[test]
        fn test_split_message_long() {
            let long = "a".repeat(3000);
            let chunks = split_message(&long, 2000);
            assert_eq!(chunks.len(), 2);
        }

        #[test]
        fn test_split_at_newline() {
            let msg = format!("{}\n{}", "a".repeat(1000), "b".repeat(1500));
            let chunks = split_message(&msg, 2000);
            assert_eq!(chunks.len(), 2);
            assert_eq!(chunks[0], "a".repeat(1000));
        }
    }
}

#[cfg(feature = "discord")]
pub use inner::{DiscordAdapter, DiscordConfig};
