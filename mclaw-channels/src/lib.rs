//! Chat platform adapters for MerlionClaw.
//!
//! Provides the `Channel` trait and implementations for
//! Telegram, Slack, and other messaging platforms.

/// A normalized message from any chat platform.
pub struct Message {
    /// Sender identifier.
    pub sender: String,
    /// Message text content.
    pub text: String,
}
