//! WebSocket gateway and HTTP server for MerlionClaw.
//!
//! Provides the axum-based server that handles WebSocket connections
//! from channel adapters and routes messages to the agent loop.

/// Gateway configuration.
pub struct GatewayConfig {
    /// Host to bind to.
    pub host: String,
    /// Port to listen on.
    pub port: u16,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 18789,
        }
    }
}
