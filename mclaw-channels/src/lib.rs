//! Chat platform adapters for MerlionClaw.
//!
//! Provides the `ChannelAdapter` trait and implementations for
//! Telegram, Slack, and other messaging platforms.

pub mod traits;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "slack")]
pub mod slack;
