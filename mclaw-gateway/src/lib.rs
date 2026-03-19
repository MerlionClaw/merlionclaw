//! WebSocket gateway and HTTP server for MerlionClaw.
//!
//! Provides the axum-based server that handles WebSocket connections
//! from channel adapters and routes messages to the agent loop.

pub mod alert;
pub mod config;
pub mod protocol;
pub mod server;
pub mod session;
