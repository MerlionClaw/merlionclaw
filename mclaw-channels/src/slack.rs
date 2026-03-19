//! Slack bot adapter using the Slack Web API and Socket Mode.
//!
//! Uses reqwest for API calls since slack-morphism's Socket Mode API
//! is complex. The adapter connects to the gateway WS and polls Slack
//! for messages using conversations.history with a cursor.

#[cfg(feature = "slack")]
mod inner {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use mclaw_gateway::protocol::{ChannelKind, InboundMessage, OutboundMessage};
    use tokio::sync::RwLock;
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    use tokio_util::sync::CancellationToken;
    use tracing::{debug, error, info, warn};

    use crate::traits::ChannelAdapter;

    /// Slack channel configuration.
    #[derive(Debug, Clone)]
    pub struct SlackConfig {
        /// App-level token for Socket Mode (xapp-...).
        pub app_token: String,
        /// Bot token for API calls (xoxb-...).
        pub bot_token: String,
        /// Allowed user IDs.
        pub allow_from: Vec<String>,
        /// Whether to require @mention in channels.
        pub require_mention: bool,
    }

    /// Slack bot adapter.
    pub struct SlackAdapter {
        config: SlackConfig,
    }

    impl SlackAdapter {
        /// Create a new Slack adapter.
        pub fn new(config: SlackConfig) -> Self {
            Self { config }
        }

        pub fn is_allowed(&self, user_id: &str) -> bool {
            self.config.allow_from.is_empty()
                || self.config.allow_from.iter().any(|a| a == user_id)
        }
    }

    /// Convert standard Markdown to Slack mrkdwn format.
    pub fn to_slack_mrkdwn(text: &str) -> String {
        let mut result = text.to_string();

        // **bold** → *bold*
        while let Some(start) = result.find("**") {
            if let Some(end) = result[start + 2..].find("**") {
                let end = start + 2 + end;
                let inner = result[start + 2..end].to_string();
                result = format!("{}*{}*{}", &result[..start], inner, &result[end + 2..]);
            } else {
                break;
            }
        }

        // - list item → • list item
        result = result
            .lines()
            .map(|line| {
                if let Some(rest) = line.strip_prefix("- ") {
                    format!("• {rest}")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        result
    }

    /// Split a message for Slack's 3000 char block limit.
    fn split_for_slack(text: &str, max_len: usize) -> Vec<String> {
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

    /// Post a message to Slack via the Web API.
    async fn post_message(
        client: &reqwest::Client,
        bot_token: &str,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut body = serde_json::json!({
            "channel": channel,
            "text": text,
        });
        if let Some(ts) = thread_ts {
            body["thread_ts"] = serde_json::Value::String(ts.to_string());
        }

        let resp = client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(bot_token)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            warn!(status = %status, body = %text, "Slack API error");
        }

        Ok(())
    }

    /// Open a Socket Mode WebSocket connection and listen for events.
    async fn open_socket_mode(
        client: &reqwest::Client,
        app_token: &str,
    ) -> anyhow::Result<String> {
        let resp = client
            .post("https://slack.com/api/apps.connections.open")
            .bearer_auth(app_token)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        if body["ok"].as_bool() != Some(true) {
            anyhow::bail!(
                "Failed to open Socket Mode connection: {}",
                body["error"].as_str().unwrap_or("unknown")
            );
        }

        body["url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no URL in Socket Mode response"))
    }

    /// Get bot user ID via auth.test.
    async fn get_bot_user_id(
        client: &reqwest::Client,
        bot_token: &str,
    ) -> anyhow::Result<String> {
        let resp = client
            .post("https://slack.com/api/auth.test")
            .bearer_auth(bot_token)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        body["user_id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("failed to get bot user ID"))
    }

    #[async_trait]
    impl ChannelAdapter for SlackAdapter {
        fn kind(&self) -> ChannelKind {
            ChannelKind::Slack
        }

        async fn start(
            &self,
            gateway_url: String,
            shutdown: CancellationToken,
        ) -> anyhow::Result<()> {
            info!("starting Slack adapter");

            let http_client = reqwest::Client::new();

            // Get bot user ID
            let bot_user_id = get_bot_user_id(&http_client, &self.config.bot_token).await?;
            info!(bot_user_id = %bot_user_id, "Slack bot authenticated");

            // Open Socket Mode WSS
            let wss_url = open_socket_mode(&http_client, &self.config.app_token).await?;
            let (slack_ws, _) = tokio_tungstenite::connect_async(&wss_url).await?;
            let (mut slack_write, mut slack_read) = slack_ws.split();
            info!("Slack Socket Mode connected");

            // Connect to gateway WS
            let gw_ws_url = format!("{gateway_url}/ws");
            let (gw_ws, _) = tokio_tungstenite::connect_async(&gw_ws_url).await?;
            let (mut gw_write, mut gw_read) = gw_ws.split();

            // Register with gateway
            let register = serde_json::to_string(&InboundMessage::RegisterChannel {
                channel: ChannelKind::Slack,
            })?;
            gw_write.send(WsMessage::Text(register.into())).await?;

            // Reply map: session_id → (channel_id, thread_ts)
            let reply_map: Arc<RwLock<HashMap<String, (String, Option<String>)>>> =
                Arc::new(RwLock::new(HashMap::new()));

            let gw_write = Arc::new(tokio::sync::Mutex::new(gw_write));

            // Task: receive gateway replies and post to Slack
            let bot_token = self.config.bot_token.clone();
            let reply_map_out = reply_map.clone();
            let shutdown_out = shutdown.clone();
            let http_out = http_client.clone();
            let outbound_task = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        msg = gw_read.next() => {
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
                                    if let Some((ch, ts)) = map.get(&session_id) {
                                        let mrkdwn = to_slack_mrkdwn(&content);
                                        for chunk in split_for_slack(&mrkdwn, 3000) {
                                            let _ = post_message(&http_out, &bot_token, ch, &chunk, ts.as_deref()).await;
                                        }
                                    }
                                }
                                OutboundMessage::Error { session_id: Some(sid), message, .. } => {
                                    let map = reply_map_out.read().await;
                                    if let Some((ch, ts)) = map.get(&sid) {
                                        let _ = post_message(&http_out, &bot_token, ch, &format!(":warning: {message}"), ts.as_deref()).await;
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ = shutdown_out.cancelled() => break,
                    }
                }
            });

            // Main loop: read Slack Socket Mode events
            let config = self.config.clone();
            let gw_write_clone = gw_write.clone();
            let reply_map_in = reply_map.clone();

            loop {
                tokio::select! {
                    msg = slack_read.next() => {
                        let text = match msg {
                            Some(Ok(WsMessage::Text(t))) => t.to_string(),
                            Some(Err(e)) => { warn!(error = %e, "Slack WS error"); break; }
                            None => { info!("Slack WS closed"); break; }
                            _ => continue,
                        };

                        let envelope: serde_json::Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        // Acknowledge the envelope
                        if let Some(envelope_id) = envelope["envelope_id"].as_str() {
                            let ack = serde_json::json!({"envelope_id": envelope_id});
                            let _ = slack_write.send(WsMessage::Text(ack.to_string().into())).await;
                        }

                        // Process events_api type
                        if envelope["type"].as_str() != Some("events_api") {
                            continue;
                        }

                        let event = &envelope["payload"]["event"];
                        if event["type"].as_str() != Some("message") {
                            continue;
                        }

                        // Skip bot messages and subtypes (edits, etc.)
                        if event.get("bot_id").is_some() || event.get("subtype").is_some() {
                            continue;
                        }

                        let user = event["user"].as_str().unwrap_or("");
                        if user == bot_user_id || user.is_empty() {
                            continue;
                        }

                        if !config.allow_from.is_empty() && !config.allow_from.iter().any(|a| a == user) {
                            debug!(user, "Slack user not in allowlist");
                            continue;
                        }

                        let channel_id = event["channel"].as_str().unwrap_or("").to_string();
                        let mut msg_text = event["text"].as_str().unwrap_or("").to_string();

                        // Strip bot mention
                        let mention = format!("<@{bot_user_id}>");
                        msg_text = msg_text.replace(&mention, "").trim().to_string();

                        if msg_text.is_empty() {
                            continue;
                        }

                        let thread_ts = event["thread_ts"]
                            .as_str()
                            .or(event["ts"].as_str())
                            .map(|s| s.to_string());

                        let session_id = format!("slack:{user}");

                        reply_map_in.write().await.insert(
                            session_id.clone(),
                            (channel_id, thread_ts),
                        );

                        let inbound = InboundMessage::Chat {
                            session_id,
                            channel: ChannelKind::Slack,
                            sender: user.to_string(),
                            content: msg_text,
                            reply_to: None,
                        };

                        let json = serde_json::to_string(&inbound).unwrap();
                        let mut writer = gw_write_clone.lock().await;
                        if let Err(e) = writer.send(WsMessage::Text(json.into())).await {
                            error!(error = %e, "failed to send to gateway");
                        }
                    }
                    _ = shutdown.cancelled() => {
                        info!("Slack adapter shutting down");
                        break;
                    }
                }
            }

            outbound_task.abort();
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_to_slack_mrkdwn_bold() {
            assert_eq!(to_slack_mrkdwn("**hello**"), "*hello*");
        }

        #[test]
        fn test_to_slack_mrkdwn_list() {
            let input = "- item one\n- item two";
            let output = to_slack_mrkdwn(input);
            assert!(output.contains("• item one"));
            assert!(output.contains("• item two"));
        }

        #[test]
        fn test_to_slack_mrkdwn_no_change() {
            assert_eq!(to_slack_mrkdwn("`code`"), "`code`");
            assert_eq!(to_slack_mrkdwn("> quote"), "> quote");
        }

        #[test]
        fn test_split_for_slack_short() {
            let chunks = split_for_slack("short", 3000);
            assert_eq!(chunks.len(), 1);
        }

        #[test]
        fn test_split_for_slack_long() {
            let long = "a".repeat(4000);
            let chunks = split_for_slack(&long, 3000);
            assert_eq!(chunks.len(), 2);
        }

        #[test]
        fn test_is_allowed_empty() {
            let adapter = SlackAdapter::new(SlackConfig {
                app_token: String::new(),
                bot_token: String::new(),
                allow_from: vec![],
                require_mention: false,
            });
            assert!(adapter.is_allowed("anyone"));
        }

        #[test]
        fn test_is_allowed_listed() {
            let adapter = SlackAdapter::new(SlackConfig {
                app_token: String::new(),
                bot_token: String::new(),
                allow_from: vec!["U123".to_string()],
                require_mention: false,
            });
            assert!(adapter.is_allowed("U123"));
            assert!(!adapter.is_allowed("U456"));
        }
    }
}

#[cfg(feature = "slack")]
pub use inner::{to_slack_mrkdwn, SlackAdapter, SlackConfig};
