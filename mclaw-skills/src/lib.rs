//! Skill engine for MerlionClaw.
//!
//! Handles SKILL.md parsing, skill registration, and tool dispatch.
//! Skills are exposed to the LLM as callable tools.

/// A registered skill definition parsed from a SKILL.md file.
pub struct SkillDefinition {
    /// Unique skill name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
}
