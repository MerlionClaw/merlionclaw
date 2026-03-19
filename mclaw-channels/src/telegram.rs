//! Telegram bot adapter using teloxide.

#[cfg(feature = "telegram")]
mod inner {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use mclaw_gateway::protocol::{ChannelKind, InboundMessage, OutboundMessage};
    use teloxide::prelude::*;
    use teloxide::types::ParseMode;
    use tokio::sync::{mpsc, RwLock};
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    use tokio_util::sync::CancellationToken;
    use tracing::{debug, error, info, warn};

    use crate::traits::ChannelAdapter;

    /// Telegram channel configuration.
    #[derive(Debug, Clone)]
    pub struct TelegramConfig {
        /// Bot token from @BotFather.
        pub bot_token: String,
        /// Allowed user IDs or usernames.
        pub allow_from: Vec<String>,
    }

    /// Telegram bot adapter.
    pub struct TelegramAdapter {
        config: TelegramConfig,
    }

    impl TelegramAdapter {
        /// Create a new Telegram adapter.
        pub fn new(config: TelegramConfig) -> Self {
            Self { config }
        }

        /// Check if a user is allowed to interact with the bot.
        pub fn is_allowed(&self, user: &teloxide::types::User) -> bool {
            if self.config.allow_from.is_empty() {
                return true; // No allowlist = allow all
            }
            let user_id = user.id.to_string();
            let username = user.username.as_deref().unwrap_or("");
            self.config.allow_from.iter().any(|a| {
                a == &user_id || a == username || a == &format!("@{username}")
            })
        }
    }

    /// Escape special characters for Telegram MarkdownV2.
    fn escape_markdown_v2(text: &str) -> String {
        let special_chars = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
        let mut result = String::with_capacity(text.len());
        for ch in text.chars() {
            if special_chars.contains(&ch) {
                result.push('\\');
            }
            result.push(ch);
        }
        result
    }

    /// Split a message into chunks that fit Telegram's 4096 char limit.
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

            // Try to split at a newline
            let split_at = remaining[..max_len]
                .rfind('\n')
                .unwrap_or(max_len);

            chunks.push(remaining[..split_at].to_string());
            remaining = remaining[split_at..].trim_start_matches('\n');
        }

        chunks
    }

    #[async_trait]
    impl ChannelAdapter for TelegramAdapter {
        fn kind(&self) -> ChannelKind {
            ChannelKind::Telegram
        }

        async fn start(&self, gateway_url: String, shutdown: CancellationToken) -> anyhow::Result<()> {
            info!("starting Telegram adapter");

            let bot = Bot::new(&self.config.bot_token);

            // Connect to gateway WS
            let ws_url = format!("{gateway_url}/ws");
            let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await
                .map_err(|e| anyhow::anyhow!("failed to connect to gateway: {e}"))?;

            let (mut ws_write, mut ws_read) = ws_stream.split();
            info!(url = %ws_url, "connected to gateway");

            // Send registration
            let register = serde_json::to_string(&InboundMessage::RegisterChannel {
                channel: ChannelKind::Telegram,
            })?;
            ws_write.send(WsMessage::Text(register.into())).await?;

            // Shared state: map session_id → chat_id for routing replies
            let chat_map: Arc<RwLock<HashMap<String, ChatId>>> = Arc::new(RwLock::new(HashMap::new()));

            // Channel for sending messages from telegram handler → WS writer
            let (tx, mut rx) = mpsc::channel::<(String, ChatId)>(100);

            let ws_write = Arc::new(tokio::sync::Mutex::new(ws_write));

            // Task: forward messages from telegram to gateway
            let ws_write_clone = ws_write.clone();
            let chat_map_clone = chat_map.clone();
            let forward_task = tokio::spawn(async move {
                while let Some((json, _chat_id)) = rx.recv().await {
                    let mut writer = ws_write_clone.lock().await;
                    if let Err(e) = writer.send(WsMessage::Text(json.into())).await {
                        error!(error = %e, "failed to send to gateway");
                    }
                }
                // Store chat_map ref to keep it alive
                drop(chat_map_clone);
            });

            // Task: receive replies from gateway and send to Telegram
            let bot_reply = bot.clone();
            let chat_map_reply = chat_map.clone();
            let shutdown_reply = shutdown.clone();
            let reply_task = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        msg = ws_read.next() => {
                            let msg = match msg {
                                Some(Ok(WsMessage::Text(text))) => text,
                                Some(Err(e)) => {
                                    warn!(error = %e, "gateway WS error");
                                    break;
                                }
                                None => {
                                    info!("gateway WS closed");
                                    break;
                                }
                                _ => continue,
                            };

                            let outbound: OutboundMessage = match serde_json::from_str(&msg) {
                                Ok(m) => m,
                                Err(e) => {
                                    warn!(error = %e, "invalid gateway message");
                                    continue;
                                }
                            };

                            match outbound {
                                OutboundMessage::Reply { session_id, content, .. } => {
                                    let map = chat_map_reply.read().await;
                                    if let Some(&chat_id) = map.get(&session_id) {
                                        for chunk in split_message(&content, 4096) {
                                            if let Err(e) = bot_reply
                                                .send_message(chat_id, &chunk)
                                                .await
                                            {
                                                error!(error = %e, "failed to send Telegram message");
                                            }
                                        }
                                    }
                                }
                                OutboundMessage::Error { session_id: Some(sid), message, .. } => {
                                    let map = chat_map_reply.read().await;
                                    if let Some(&chat_id) = map.get(&sid) {
                                        let escaped = escape_markdown_v2(&message);
                                        let _ = bot_reply
                                            .send_message(chat_id, format!("Error: {escaped}"))
                                            .parse_mode(ParseMode::MarkdownV2)
                                            .await;
                                    }
                                }
                                OutboundMessage::Error { .. } => {}
                                OutboundMessage::Pong => {
                                    debug!("received pong from gateway");
                                }
                                _ => {}
                            }
                        }
                        _ = shutdown_reply.cancelled() => {
                            info!("reply task shutting down");
                            break;
                        }
                    }
                }
            });

            // Set up teloxide dispatcher
            let config = self.config.clone();
            let handler = Update::filter_message().endpoint(
                move |bot: Bot, msg: teloxide::types::Message, tx: Arc<mpsc::Sender<(String, ChatId)>>, chat_map: Arc<RwLock<HashMap<String, ChatId>>>, config: Arc<TelegramConfig>| async move {
                    let text = match msg.text() {
                        Some(t) => t,
                        None => return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(()),
                    };

                    let user = match msg.from {
                        Some(ref u) => u,
                        None => return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(()),
                    };

                    // Check allowlist
                    let user_id = user.id.to_string();
                    let username = user.username.as_deref().unwrap_or("");
                    let allowed = config.allow_from.is_empty()
                        || config.allow_from.iter().any(|a| {
                            a == &user_id || a == username || a == &format!("@{username}")
                        });

                    if !allowed {
                        warn!(user_id = %user.id, username = %username, "unauthorized user");
                        bot.send_message(msg.chat.id, "Unauthorized. Contact the admin to get access.")
                            .await?;
                        return Ok(());
                    }

                    let session_id = format!("telegram:{}", user.id);
                    let sender = user.username.clone().unwrap_or_else(|| user.first_name.clone());

                    // Map session to chat for reply routing
                    chat_map.write().await.insert(session_id.clone(), msg.chat.id);

                    let inbound = InboundMessage::Chat {
                        session_id,
                        channel: ChannelKind::Telegram,
                        sender,
                        content: text.to_string(),
                        reply_to: msg.reply_to_message().and_then(|r| r.text().map(|t| t.to_string())),
                    };

                    let json = serde_json::to_string(&inbound).unwrap();
                    if let Err(e) = tx.send((json, msg.chat.id)).await {
                        error!(error = %e, "failed to queue message");
                    }

                    Ok(())
                },
            );

            let tx = Arc::new(tx);
            let config = Arc::new(config);

            let mut dispatcher = Dispatcher::builder(bot, handler)
                .dependencies(teloxide::dptree::deps![tx, chat_map, config])
                .enable_ctrlc_handler()
                .build();

            // Run dispatcher until shutdown
            tokio::select! {
                _ = dispatcher.dispatch() => {
                    info!("Telegram dispatcher stopped");
                }
                _ = shutdown.cancelled() => {
                    info!("Telegram adapter shutting down");
                    dispatcher.shutdown_token().shutdown().expect("shutdown").await;
                }
            }

            forward_task.abort();
            reply_task.abort();

            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_escape_markdown_v2() {
            assert_eq!(escape_markdown_v2("hello.world"), r"hello\.world");
            assert_eq!(escape_markdown_v2("a_b*c"), r"a\_b\*c");
            assert_eq!(escape_markdown_v2("plain"), "plain");
        }

        #[test]
        fn test_split_message_short() {
            let chunks = split_message("short message", 4096);
            assert_eq!(chunks.len(), 1);
            assert_eq!(chunks[0], "short message");
        }

        #[test]
        fn test_split_message_long() {
            let long = "a".repeat(5000);
            let chunks = split_message(&long, 4096);
            assert_eq!(chunks.len(), 2);
            assert_eq!(chunks[0].len(), 4096);
        }

        #[test]
        fn test_split_message_at_newline() {
            let msg = format!("{}\n{}", "a".repeat(2000), "b".repeat(3000));
            let chunks = split_message(&msg, 4096);
            assert_eq!(chunks.len(), 2);
            assert_eq!(chunks[0], "a".repeat(2000));
        }

        #[test]
        fn test_is_allowed_empty_list() {
            let adapter = TelegramAdapter::new(TelegramConfig {
                bot_token: "test".to_string(),
                allow_from: vec![],
            });
            let user = teloxide::types::User {
                id: UserId(123),
                is_bot: false,
                first_name: "Test".to_string(),
                last_name: None,
                username: None,
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            };
            assert!(adapter.is_allowed(&user));
        }

        #[test]
        fn test_is_allowed_by_id() {
            let adapter = TelegramAdapter::new(TelegramConfig {
                bot_token: "test".to_string(),
                allow_from: vec!["123".to_string()],
            });
            let user = teloxide::types::User {
                id: UserId(123),
                is_bot: false,
                first_name: "Test".to_string(),
                last_name: None,
                username: None,
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            };
            assert!(adapter.is_allowed(&user));
        }

        #[test]
        fn test_is_allowed_denied() {
            let adapter = TelegramAdapter::new(TelegramConfig {
                bot_token: "test".to_string(),
                allow_from: vec!["456".to_string()],
            });
            let user = teloxide::types::User {
                id: UserId(123),
                is_bot: false,
                first_name: "Test".to_string(),
                last_name: None,
                username: Some("testuser".to_string()),
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            };
            assert!(!adapter.is_allowed(&user));
        }
    }
}

#[cfg(feature = "telegram")]
pub use inner::{TelegramAdapter, TelegramConfig};
