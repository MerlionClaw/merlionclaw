//! Persistent memory system for MerlionClaw.
//!
//! Provides markdown file-based storage with tantivy full-text search
//! for agent memory, daily diaries, and long-term facts.

/// A stored memory entry.
pub struct MemoryEntry {
    /// Unique entry identifier.
    pub id: String,
    /// Memory content.
    pub content: String,
}
