//! MCP (Model Context Protocol) client for MerlionClaw.
//!
//! Connects to external MCP servers (stdio or SSE transport),
//! discovers tools, and bridges them into the skill registry.

pub mod bridge;
pub mod client;
pub mod manager;
pub mod protocol;
pub mod transport;
