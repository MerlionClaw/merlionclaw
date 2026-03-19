//! Agent loop and LLM abstraction layer for MerlionClaw.
//!
//! Manages the core agent loop: receive message, build context,
//! call LLM, execute tool calls, and return responses.

/// Placeholder for the LLM provider trait.
pub trait LlmProvider: Send + Sync {
    /// Provider name (e.g., "anthropic", "openai").
    fn name(&self) -> &str;
}
