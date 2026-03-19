//! Persistent memory system for MerlionClaw.
//!
//! Provides markdown file-based storage with tantivy full-text search
//! for agent memory, daily diaries, and long-term facts.

pub mod search;
pub mod store;
