//! Agent loop and LLM abstraction layer for MerlionClaw.
//!
//! Manages the core agent loop: receive message, build context,
//! call LLM, execute tool calls, and return responses.

pub mod agent;
pub mod llm;
