//! WhatsApp adapter using Meta's WhatsApp Cloud API.
//!
//! Incoming messages arrive via webhook (POST /webhook/whatsapp on the gateway).
//! Outgoing messages are sent via the WhatsApp Cloud API REST endpoint.
//!
//! Setup:
//! 1. Create a Meta Business app at https://developers.facebook.com
//! 2. Add WhatsApp product, get phone number ID and access token
//! 3. Configure webhook URL: https://your-domain/webhook/whatsapp
//! 4. Set verify token to match config

#[cfg(feature = "whatsapp")]
mod inner {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use mclaw_gateway::protocol::{ChannelKind, InboundMessage, OutboundMessage};
    use tokio::sync::RwLock;
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    use tokio_util::sync::CancellationToken;
    use tracing::{error, info, warn};

    use crate::traits::ChannelAdapter;

    /// WhatsApp Cloud API configuration.
    #[derive(Debug, Clone)]
    pub struct WhatsAppConfig {
        /// WhatsApp Cloud API access token.
        pub access_token: String,
        /// Phone number ID from Meta Business.
        pub phone_number_id: String,
        /// Webhook verify token (for webhook registration handshake).
        pub verify_token: String,
        /// Allowed phone numbers (empty = allow all).
        pub allow_from: Vec<String>,
    }

    /// WhatsApp adapter — connects to the gateway and sends replies via Cloud API.
    pub struct WhatsAppAdapter {
        config: WhatsAppConfig,
    }

    impl WhatsAppAdapter {
        /// Create a new WhatsApp adapter.
        pub fn new(config: WhatsAppConfig) -> Self {
            Self { config }
        }

        pub fn is_allowed(&self, phone: &str) -> bool {
            self.config.allow_from.is_empty()
                || self.config.allow_from.iter().any(|a| a == phone)
        }
    }

    /// Send a text message via WhatsApp Cloud API.
    async fn send_whatsapp_message(
        client: &reqwest::Client,
        access_token: &str,
        phone_number_id: &str,
        to: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        let url = format!(
            "https://graph.facebook.com/v21.0/{phone_number_id}/messages"
        );

        // WhatsApp max message length is 4096
        let chunks = split_message(text, 4096);
        for chunk in chunks {
            let body = serde_json::json!({
                "messaging_product": "whatsapp",
                "to": to,
                "type": "text",
                "text": { "body": chunk }
            });

            let resp = client
                .post(&url)
                .bearer_auth(access_token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "WhatsApp API error");
            }
        }

        Ok(())
    }

    /// Split message into chunks.
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

    /// Parse an incoming WhatsApp webhook payload into chat messages.
    /// Returns Vec of (sender_phone, message_text).
    pub fn parse_webhook_payload(payload: &serde_json::Value) -> Vec<(String, String)> {
        let mut messages = Vec::new();

        let entries = match payload["entry"].as_array() {
            Some(e) => e,
            None => return messages,
        };

        for entry in entries {
            let changes = match entry["changes"].as_array() {
                Some(c) => c,
                None => continue,
            };

            for change in changes {
                let value = &change["value"];

                // Only process message notifications
                if value.get("messages").is_none() {
                    continue;
                }

                let msgs = match value["messages"].as_array() {
                    Some(m) => m,
                    None => continue,
                };

                for msg in msgs {
                    // Only handle text messages for now
                    if msg["type"].as_str() != Some("text") {
                        continue;
                    }

                    let from = msg["from"].as_str().unwrap_or("").to_string();
                    let text = msg["text"]["body"].as_str().unwrap_or("").to_string();

                    if !from.is_empty() && !text.is_empty() {
                        messages.push((from, text));
                    }
                }
            }
        }

        messages
    }

    #[async_trait]
    impl ChannelAdapter for WhatsAppAdapter {
        fn kind(&self) -> ChannelKind {
            ChannelKind::Whatsapp
        }

        async fn start(
            &self,
            gateway_url: String,
            shutdown: CancellationToken,
        ) -> anyhow::Result<()> {
            info!("starting WhatsApp adapter");

            // Connect to gateway WS
            let ws_url = format!("{gateway_url}/ws");
            let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
                .await
                .map_err(|e| anyhow::anyhow!("failed to connect to gateway: {e}"))?;

            let (mut ws_write, mut ws_read) = ws_stream.split();

            // Register with gateway
            let register = serde_json::to_string(&InboundMessage::RegisterChannel {
                channel: ChannelKind::Whatsapp,
            })?;
            ws_write.send(WsMessage::Text(register.into())).await?;

            // Phone map: session_id → phone number for replies
            let phone_map: Arc<RwLock<HashMap<String, String>>> =
                Arc::new(RwLock::new(HashMap::new()));

            let _ws_write = Arc::new(tokio::sync::Mutex::new(ws_write));

            // Task: receive replies from gateway and send via WhatsApp API
            let config = self.config.clone();
            let phone_map_out = phone_map.clone();
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
                                    let map = phone_map_out.read().await;
                                    if let Some(phone) = map.get(&session_id) {
                                        if let Err(e) = send_whatsapp_message(
                                            &client,
                                            &config.access_token,
                                            &config.phone_number_id,
                                            phone,
                                            &content,
                                        ).await {
                                            error!(error = %e, "failed to send WhatsApp message");
                                        }
                                    }
                                }
                                OutboundMessage::Error { session_id: Some(sid), message, .. } => {
                                    let map = phone_map_out.read().await;
                                    if let Some(phone) = map.get(&sid) {
                                        let _ = send_whatsapp_message(
                                            &client,
                                            &config.access_token,
                                            &config.phone_number_id,
                                            phone,
                                            &format!("Error: {message}"),
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

            // WhatsApp uses webhook-based incoming messages.
            // The gateway's /webhook/whatsapp endpoint handles incoming messages
            // and forwards them to this adapter via the WS connection.
            // We just need to keep running until shutdown.
            info!("WhatsApp adapter ready (webhook mode)");
            info!("Configure webhook URL: https://your-domain/webhook/whatsapp");

            shutdown.cancelled().await;
            info!("WhatsApp adapter shutting down");

            reply_task.abort();
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_parse_webhook_payload() {
            let payload = serde_json::json!({
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "6512345678",
                                "type": "text",
                                "text": { "body": "list pods" }
                            }]
                        }
                    }]
                }]
            });

            let msgs = parse_webhook_payload(&payload);
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].0, "6512345678");
            assert_eq!(msgs[0].1, "list pods");
        }

        #[test]
        fn test_parse_webhook_no_messages() {
            let payload = serde_json::json!({"entry": []});
            let msgs = parse_webhook_payload(&payload);
            assert!(msgs.is_empty());
        }

        #[test]
        fn test_parse_webhook_non_text() {
            let payload = serde_json::json!({
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "123",
                                "type": "image"
                            }]
                        }
                    }]
                }]
            });

            let msgs = parse_webhook_payload(&payload);
            assert!(msgs.is_empty());
        }

        #[test]
        fn test_split_message_short() {
            let chunks = split_message("hello", 4096);
            assert_eq!(chunks.len(), 1);
        }

        #[test]
        fn test_split_message_long() {
            let long = "a".repeat(5000);
            let chunks = split_message(&long, 4096);
            assert_eq!(chunks.len(), 2);
        }

        #[test]
        fn test_is_allowed_empty() {
            let adapter = WhatsAppAdapter::new(WhatsAppConfig {
                access_token: String::new(),
                phone_number_id: String::new(),
                verify_token: String::new(),
                allow_from: vec![],
            });
            assert!(adapter.is_allowed("anyone"));
        }

        #[test]
        fn test_is_allowed_listed() {
            let adapter = WhatsAppAdapter::new(WhatsAppConfig {
                access_token: String::new(),
                phone_number_id: String::new(),
                verify_token: String::new(),
                allow_from: vec!["6512345678".to_string()],
            });
            assert!(adapter.is_allowed("6512345678"));
            assert!(!adapter.is_allowed("9999999999"));
        }
    }
}

#[cfg(feature = "whatsapp")]
pub use inner::{parse_webhook_payload, WhatsAppAdapter, WhatsAppConfig};
