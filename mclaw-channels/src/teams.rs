//! Microsoft Teams adapter using Bot Framework REST API.
//!
//! Incoming messages arrive via webhook (POST /webhook/teams on the gateway).
//! Outgoing messages are sent via the Bot Framework v3 REST API.
//!
//! Setup:
//! 1. Register a bot at https://dev.botframework.com or Azure Portal
//! 2. Get App ID and App Password (client secret)
//! 3. Configure messaging endpoint: https://your-domain/webhook/teams
//! 4. Install the bot in your Teams tenant

#[cfg(feature = "teams")]
mod inner {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use mclaw_gateway::protocol::{ChannelKind, InboundMessage, OutboundMessage};
    use tokio::sync::RwLock;
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    use tokio_util::sync::CancellationToken;
    use tracing::{info, warn};

    use crate::traits::ChannelAdapter;

    /// Microsoft Teams configuration.
    #[derive(Debug, Clone)]
    pub struct TeamsConfig {
        /// Bot Framework App ID.
        pub app_id: String,
        /// Bot Framework App Password (client secret).
        pub app_password: String,
        /// Allowed user AAD object IDs (empty = allow all).
        pub allow_from: Vec<String>,
    }

    /// Microsoft Teams adapter.
    pub struct TeamsAdapter {
        config: TeamsConfig,
    }

    impl TeamsAdapter {
        /// Create a new Teams adapter.
        pub fn new(config: TeamsConfig) -> Self {
            Self { config }
        }

        pub fn is_allowed(&self, user_id: &str) -> bool {
            self.config.allow_from.is_empty()
                || self.config.allow_from.iter().any(|a| a == user_id)
        }
    }

    /// Get an OAuth token from the Bot Framework token endpoint.
    async fn get_bot_token(
        client: &reqwest::Client,
        app_id: &str,
        app_password: &str,
    ) -> anyhow::Result<String> {
        let resp = client
            .post("https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token")
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", app_id),
                ("client_secret", app_password),
                (
                    "scope",
                    "https://api.botframework.com/.default",
                ),
            ])
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        body["access_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "failed to get bot token: {}",
                    body["error_description"]
                        .as_str()
                        .unwrap_or("unknown error")
                )
            })
    }

    /// Send a reply to Teams via the Bot Framework REST API.
    async fn send_teams_reply(
        client: &reqwest::Client,
        token: &str,
        service_url: &str,
        conversation_id: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        let url = format!(
            "{}v3/conversations/{}/activities",
            service_url, conversation_id
        );

        // Split into chunks (Teams limit: ~28KB, but keep it safe at 4000 chars)
        let chunks = split_message(text, 4000);
        for chunk in chunks {
            let body = serde_json::json!({
                "type": "message",
                "text": chunk,
                "textFormat": "markdown",
            });

            let resp = client
                .post(&url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err = resp.text().await.unwrap_or_default();
                warn!(status = %status, body = %err, "Teams API error");
            }
        }

        Ok(())
    }

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

    /// Parse a Bot Framework activity into (user_id, user_name, text, conversation_id, service_url).
    pub fn parse_teams_activity(
        payload: &serde_json::Value,
    ) -> Option<(String, String, String, String, String)> {
        let activity_type = payload["type"].as_str()?;
        if activity_type != "message" {
            return None;
        }

        let text = payload["text"].as_str()?.to_string();
        if text.is_empty() {
            return None;
        }

        let user_id = payload["from"]["id"].as_str()?.to_string();
        let user_name = payload["from"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let conversation_id = payload["conversation"]["id"].as_str()?.to_string();
        let service_url = payload["serviceUrl"].as_str()?.to_string();

        // Strip bot @mention from text (Teams includes it)
        let clean_text = text
            .split("</at>")
            .last()
            .unwrap_or(&text)
            .trim()
            .to_string();

        if clean_text.is_empty() {
            return None;
        }

        Some((user_id, user_name, clean_text, conversation_id, service_url))
    }

    /// Reply context: conversation_id + service_url needed for replies.
    #[derive(Clone)]
    struct TeamsReplyTarget {
        conversation_id: String,
        service_url: String,
    }

    #[async_trait]
    impl ChannelAdapter for TeamsAdapter {
        fn kind(&self) -> ChannelKind {
            ChannelKind::Teams
        }

        async fn start(
            &self,
            gateway_url: String,
            shutdown: CancellationToken,
        ) -> anyhow::Result<()> {
            info!("starting Microsoft Teams adapter");

            // Connect to gateway WS
            let ws_url = format!("{gateway_url}/ws");
            let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
                .await
                .map_err(|e| anyhow::anyhow!("failed to connect to gateway: {e}"))?;

            let (mut ws_write, mut ws_read) = ws_stream.split();

            // Register with gateway
            let register = serde_json::to_string(&InboundMessage::RegisterChannel {
                channel: ChannelKind::Teams,
            })?;
            ws_write.send(WsMessage::Text(register.into())).await?;

            // Reply map: session_id → TeamsReplyTarget
            let reply_map: Arc<RwLock<HashMap<String, TeamsReplyTarget>>> =
                Arc::new(RwLock::new(HashMap::new()));

            let _ws_write = Arc::new(tokio::sync::Mutex::new(ws_write));

            // Get initial bot token
            let http_client = reqwest::Client::new();
            let token = Arc::new(RwLock::new(
                get_bot_token(&http_client, &self.config.app_id, &self.config.app_password)
                    .await
                    .unwrap_or_default(),
            ));

            // Task: receive replies from gateway and send to Teams
            let config = self.config.clone();
            let reply_map_out = reply_map.clone();
            let token_out = token.clone();
            let shutdown_out = shutdown.clone();
            let reply_task = tokio::spawn(async move {
                let client = reqwest::Client::new();

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
                                    if let Some(target) = map.get(&session_id) {
                                        let tok = token_out.read().await.clone();
                                        if let Err(e) = send_teams_reply(
                                            &client,
                                            &tok,
                                            &target.service_url,
                                            &target.conversation_id,
                                            &content,
                                        ).await {
                                            // Token may have expired, try refresh
                                            warn!(error = %e, "Teams reply failed, refreshing token");
                                            if let Ok(new_tok) = get_bot_token(
                                                &client,
                                                &config.app_id,
                                                &config.app_password,
                                            ).await {
                                                *token_out.write().await = new_tok.clone();
                                                let _ = send_teams_reply(
                                                    &client,
                                                    &new_tok,
                                                    &target.service_url,
                                                    &target.conversation_id,
                                                    &content,
                                                ).await;
                                            }
                                        }
                                    }
                                }
                                OutboundMessage::Error { session_id: Some(sid), message, .. } => {
                                    let map = reply_map_out.read().await;
                                    if let Some(target) = map.get(&sid) {
                                        let tok = token_out.read().await.clone();
                                        let _ = send_teams_reply(
                                            &client,
                                            &tok,
                                            &target.service_url,
                                            &target.conversation_id,
                                            &format!("⚠️ Error: {message}"),
                                        ).await;
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ = shutdown_out.cancelled() => break,
                    }
                }
            });

            // Teams uses webhook-based incoming messages.
            // The gateway's /webhook/teams endpoint handles incoming activities.
            info!("Teams adapter ready (webhook mode)");
            info!("Configure messaging endpoint: https://your-domain/webhook/teams");

            shutdown.cancelled().await;
            info!("Teams adapter shutting down");

            reply_task.abort();
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_parse_teams_activity() {
            let payload = serde_json::json!({
                "type": "message",
                "text": "list pods",
                "from": { "id": "user123", "name": "Larry" },
                "conversation": { "id": "conv456" },
                "serviceUrl": "https://smba.trafficmanager.net/teams/"
            });

            let result = parse_teams_activity(&payload);
            assert!(result.is_some());
            let (user_id, user_name, text, conv_id, service_url) = result.unwrap();
            assert_eq!(user_id, "user123");
            assert_eq!(user_name, "Larry");
            assert_eq!(text, "list pods");
            assert_eq!(conv_id, "conv456");
            assert!(service_url.contains("trafficmanager"));
        }

        #[test]
        fn test_parse_teams_activity_with_mention() {
            let payload = serde_json::json!({
                "type": "message",
                "text": "<at>MerlionClaw</at> list pods",
                "from": { "id": "user123", "name": "Larry" },
                "conversation": { "id": "conv456" },
                "serviceUrl": "https://smba.trafficmanager.net/teams/"
            });

            let (_, _, text, _, _) = parse_teams_activity(&payload).unwrap();
            assert_eq!(text, "list pods");
        }

        #[test]
        fn test_parse_non_message() {
            let payload = serde_json::json!({
                "type": "conversationUpdate",
                "from": { "id": "user123" }
            });
            assert!(parse_teams_activity(&payload).is_none());
        }

        #[test]
        fn test_split_message_short() {
            let chunks = split_message("hello", 4000);
            assert_eq!(chunks.len(), 1);
        }

        #[test]
        fn test_split_message_long() {
            let long = "a".repeat(5000);
            let chunks = split_message(&long, 4000);
            assert_eq!(chunks.len(), 2);
        }

        #[test]
        fn test_is_allowed() {
            let adapter = TeamsAdapter::new(TeamsConfig {
                app_id: String::new(),
                app_password: String::new(),
                allow_from: vec!["user123".to_string()],
            });
            assert!(adapter.is_allowed("user123"));
            assert!(!adapter.is_allowed("user456"));

            let open = TeamsAdapter::new(TeamsConfig {
                app_id: String::new(),
                app_password: String::new(),
                allow_from: vec![],
            });
            assert!(open.is_allowed("anyone"));
        }
    }
}

#[cfg(feature = "teams")]
pub use inner::{parse_teams_activity, TeamsAdapter, TeamsConfig};
