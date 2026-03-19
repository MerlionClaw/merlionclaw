//! axum WebSocket server and HTTP routes.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

use crate::config::GatewayConfig;
use crate::protocol::{InboundMessage, OutboundMessage};
use crate::session::SessionManager;

/// Trait for processing chat messages (implemented by the Agent).
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Process a chat message and return a response.
    async fn handle(
        &self,
        session_id: String,
        sender: String,
        content: String,
        history: Vec<String>,
    ) -> HandlerResponse;
}

/// Response from a message handler.
pub enum HandlerResponse {
    /// A text reply.
    Reply { session_id: String, content: String },
    /// An error.
    Error { session_id: String, message: String },
    /// Context was cleared.
    ContextCleared { session_id: String },
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Session manager.
    pub sessions: SessionManager,
    /// Optional message handler (agent).
    handler: Option<Arc<dyn MessageHandler>>,
}

impl AppState {
    /// Create new app state from config.
    pub fn new(config: &GatewayConfig) -> Self {
        Self {
            sessions: SessionManager::new(config.session_timeout_secs),
            handler: None,
        }
    }

    /// Set the message handler.
    pub fn with_handler(mut self, handler: Arc<dyn MessageHandler>) -> Self {
        self.handler = Some(handler);
        self
    }
}

/// Start the gateway server.
pub async fn start(config: GatewayConfig) -> anyhow::Result<()> {
    start_with_handler(config, None).await
}

/// Start the gateway server with an optional message handler.
pub async fn start_with_handler(
    config: GatewayConfig,
    handler: Option<Arc<dyn MessageHandler>>,
) -> anyhow::Result<()> {
    let mut state = AppState::new(&config);
    if let Some(h) = handler {
        state = state.with_handler(h);
    }

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/webhook/alertmanager", post(alertmanager_webhook))
        .route("/webhook/pagerduty", post(pagerduty_webhook))
        .route("/webhook/whatsapp", get(whatsapp_verify))
        .route("/webhook/whatsapp", post(whatsapp_webhook))
        .with_state(state);

    let addr = SocketAddr::from((config.host, config.port));
    info!(%addr, "gateway listening");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Handle WebSocket upgrade requests.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Process a single WebSocket connection.
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!("new WebSocket connection");

    while let Some(msg) = socket.recv().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                warn!(error = %e, "WebSocket receive error");
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                let response = handle_text_message(&text, &state).await;
                let json = match serde_json::to_string(&response) {
                    Ok(j) => j,
                    Err(e) => {
                        error!(error = %e, "failed to serialize response");
                        continue;
                    }
                };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => {
                info!("WebSocket connection closed");
                break;
            }
            _ => {}
        }
    }
}

/// Route a text message to the appropriate handler.
async fn handle_text_message(text: &str, state: &AppState) -> OutboundMessage {
    let inbound: InboundMessage = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            warn!(error = %e, "invalid message format");
            return OutboundMessage::Error {
                session_id: None,
                message: format!("invalid message format: {e}"),
            };
        }
    };

    match inbound {
        InboundMessage::Ping => OutboundMessage::Pong,

        InboundMessage::RegisterChannel { channel } => {
            info!(%channel, "channel registered");
            OutboundMessage::Pong
        }

        InboundMessage::Chat {
            session_id,
            channel,
            sender,
            content,
            ..
        } => {
            let session = state.sessions.get_or_create(channel, &sender).await;
            state
                .sessions
                .record_message(&session.key, content.clone())
                .await;

            match &state.handler {
                Some(handler) => {
                    let resp = handler
                        .handle(session_id, sender, content, session.history)
                        .await;
                    match resp {
                        HandlerResponse::Reply {
                            session_id,
                            content,
                        } => OutboundMessage::Reply {
                            session_id,
                            content,
                            tool_use: None,
                        },
                        HandlerResponse::Error {
                            session_id,
                            message,
                        } => OutboundMessage::Error {
                            session_id: Some(session_id),
                            message,
                        },
                        HandlerResponse::ContextCleared { session_id } => {
                            state.sessions.clear_history(&session.key).await;
                            OutboundMessage::Reply {
                                session_id,
                                content: "Context cleared.".to_string(),
                                tool_use: None,
                            }
                        }
                    }
                }
                None => OutboundMessage::Error {
                    session_id: Some(session_id),
                    message: "agent not configured".to_string(),
                },
            }
        }
    }
}

/// Handle Alertmanager webhook.
async fn alertmanager_webhook(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let alerts = crate::alert::parse_alertmanager(&payload);
    info!(count = alerts.len(), "received Alertmanager webhook");

    if let Some(handler) = &state.handler {
        for alert in &alerts {
            let content = crate::alert::format_alert(alert);
            let session_id = format!("alert:{}", alert.id);
            handler
                .handle(session_id, "alertmanager".to_string(), content, vec![])
                .await;
        }
    }

    axum::Json(serde_json::json!({"status": "ok", "alerts_received": alerts.len()}))
}

/// Handle PagerDuty webhook.
async fn pagerduty_webhook(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let alerts = crate::alert::parse_pagerduty(&payload);
    info!(count = alerts.len(), "received PagerDuty webhook");

    if let Some(handler) = &state.handler {
        for alert in &alerts {
            let content = crate::alert::format_alert(alert);
            let session_id = format!("alert:{}", alert.id);
            handler
                .handle(session_id, "pagerduty".to_string(), content, vec![])
                .await;
        }
    }

    axum::Json(serde_json::json!({"status": "ok", "alerts_received": alerts.len()}))
}

/// WhatsApp webhook verification (GET).
/// Meta sends a GET request with hub.mode, hub.verify_token, hub.challenge.
async fn whatsapp_verify(
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mode = params.get("hub.mode").map(|s| s.as_str());
    let challenge = params.get("hub.challenge").cloned().unwrap_or_default();

    if mode == Some("subscribe") {
        // Return the challenge to verify the webhook
        debug!("WhatsApp webhook verified");
        (axum::http::StatusCode::OK, challenge)
    } else {
        (axum::http::StatusCode::FORBIDDEN, "invalid".to_string())
    }
}

/// WhatsApp incoming message webhook (POST).
async fn whatsapp_webhook(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    // Parse incoming WhatsApp messages
    let messages = parse_whatsapp_messages(&payload);
    debug!(count = messages.len(), "received WhatsApp webhook");

    if let Some(handler) = &state.handler {
        for (phone, text) in &messages {
            let session_id = format!("whatsapp:{phone}");
            handler
                .handle(session_id, phone.clone(), text.clone(), vec![])
                .await;
        }
    }

    axum::Json(serde_json::json!({"status": "ok"}))
}

/// Parse WhatsApp Cloud API webhook payload.
fn parse_whatsapp_messages(payload: &serde_json::Value) -> Vec<(String, String)> {
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
            let msgs = match change["value"]["messages"].as_array() {
                Some(m) => m,
                None => continue,
            };
            for msg in msgs {
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

/// Health check endpoint.
async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let sessions = state.sessions.active_count().await;
    axum::Json(serde_json::json!({
        "status": "ok",
        "sessions": sessions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_ping() {
        let state = AppState::new(&GatewayConfig::default());
        let response = handle_text_message(r#"{"type":"ping"}"#, &state).await;
        assert!(matches!(response, OutboundMessage::Pong));
    }

    #[tokio::test]
    async fn test_handle_chat_no_handler() {
        let state = AppState::new(&GatewayConfig::default());
        let msg = r#"{"type":"chat","session_id":"test","channel":"cli","sender":"larry","content":"hello"}"#;
        let response = handle_text_message(msg, &state).await;
        match response {
            OutboundMessage::Error {
                session_id,
                message,
            } => {
                assert_eq!(session_id.unwrap(), "test");
                assert_eq!(message, "agent not configured");
            }
            _ => panic!("expected Error"),
        }
    }

    #[tokio::test]
    async fn test_handle_invalid_json() {
        let state = AppState::new(&GatewayConfig::default());
        let response = handle_text_message("not json", &state).await;
        assert!(matches!(response, OutboundMessage::Error { .. }));
    }
}
