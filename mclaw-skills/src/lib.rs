//! Skill engine for MerlionClaw.
//!
//! Handles SKILL.md parsing, skill registration, and tool dispatch.
//! Skills are exposed to the LLM as callable tools.

pub mod parser;
pub mod registry;

#[cfg(feature = "k8s")]
pub mod k8s;
#[cfg(feature = "k8s")]
pub mod istio;

pub mod helm;
pub mod loki;
pub mod memory;
