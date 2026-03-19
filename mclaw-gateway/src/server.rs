//! axum WebSocket server and HTTP routes.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

use crate::config::GatewayConfig;
use crate::protocol::{InboundMessage, OutboundMessage};
use crate::session::SessionManager;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Session manager.
    pub sessions: SessionManager,
}

impl AppState {
    /// Create new app state from config.
    pub fn new(config: &GatewayConfig) -> Self {
        Self {
            sessions: SessionManager::new(config.session_timeout_secs),
        }
    }
}

/// Start the gateway server.
pub async fn start(config: GatewayConfig) -> anyhow::Result<()> {
    let state = AppState::new(&config);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
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
            state.sessions.record_message(&session.key, content).await;

            // Agent not yet implemented — return placeholder error
            OutboundMessage::Error {
                session_id: Some(session_id),
                message: "agent not configured".to_string(),
            }
        }
    }
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
    async fn test_handle_chat_returns_agent_not_configured() {
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
