//! Skill discovery and registration.

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::parser::{ParsedSkill, parse_skill_md};

/// Trait for skill execution handlers.
#[async_trait]
pub trait SkillHandler: Send + Sync {
    /// Execute a tool call and return the result as a string.
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String>;
}

/// A skill registered in the registry with its handler.
pub struct RegisteredSkill {
    /// The parsed SKILL.md definition.
    pub parsed: ParsedSkill,
    /// The handler that executes tool calls.
    pub handler: Box<dyn SkillHandler>,
}

/// Registry of all available skills and their handlers.
pub struct SkillRegistry {
    skills: HashMap<String, RegisteredSkill>,
    /// Maps tool_name → skill_name for dispatch.
    tool_index: HashMap<String, String>,
}

impl SkillRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            tool_index: HashMap::new(),
        }
    }

    /// Discover and parse SKILL.md files from a directory.
    /// Each subdirectory should contain a SKILL.md file.
    /// Note: handlers must be registered separately via `register_handler`.
    pub fn discover(skills_dir: &Path) -> anyhow::Result<Self> {
        let mut registry = Self::new();

        if !skills_dir.exists() {
            warn!(path = %skills_dir.display(), "skills directory not found");
            return Ok(registry);
        }

        let entries = std::fs::read_dir(skills_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            let content = std::fs::read_to_string(&skill_file)?;
            match parse_skill_md(&content) {
                Ok(parsed) => {
                    info!(
                        skill = %parsed.manifest.name,
                        tools = parsed.manifest.tools.len(),
                        "discovered skill"
                    );
                    // Index tool names
                    for tool in &parsed.manifest.tools {
                        registry
                            .tool_index
                            .insert(tool.name.clone(), parsed.manifest.name.clone());
                    }
                    // Register without handler — handler must be added later
                    registry.skills.insert(
                        parsed.manifest.name.clone(),
                        RegisteredSkill {
                            parsed,
                            handler: Box::new(NoOpHandler),
                        },
                    );
                }
                Err(e) => {
                    warn!(
                        path = %skill_file.display(),
                        error = %e,
                        "failed to parse SKILL.md"
                    );
                }
            }
        }

        Ok(registry)
    }

    /// Register a handler for a named skill (replaces the no-op default).
    pub fn register_handler(&mut self, skill_name: &str, handler: Box<dyn SkillHandler>) {
        if let Some(skill) = self.skills.get_mut(skill_name) {
            skill.handler = handler;
            info!(skill = skill_name, "handler registered");
        } else {
            warn!(skill = skill_name, "no skill found for handler");
        }
    }

    /// Register a skill with its handler directly (without discovery).
    pub fn register(&mut self, parsed: ParsedSkill, handler: Box<dyn SkillHandler>) {
        for tool in &parsed.manifest.tools {
            self.tool_index
                .insert(tool.name.clone(), parsed.manifest.name.clone());
        }
        self.skills.insert(
            parsed.manifest.name.clone(),
            RegisteredSkill { parsed, handler },
        );
    }

    /// Convert all registered tools to LLM `ToolDefinition`s.
    pub fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.skills
            .values()
            .flat_map(|s| s.parsed.manifest.tools.iter())
            .map(|t| t.to_tool_definition())
            .collect()
    }

    /// Dispatch a tool call to the appropriate skill handler.
    pub async fn dispatch(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<String> {
        let skill_name = self
            .tool_index
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("unknown tool: {tool_name}"))?;

        let skill = self
            .skills
            .get(skill_name)
            .ok_or_else(|| anyhow::anyhow!("skill not found: {skill_name}"))?;

        skill.handler.execute(tool_name, input).await
    }

    /// Build a combined system prompt fragment from all active skills.
    pub fn system_prompt(&self) -> String {
        self.skills
            .values()
            .filter(|s| !s.parsed.system_prompt_fragment.is_empty())
            .map(|s| s.parsed.system_prompt_fragment.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Get a human-readable summary of all registered skills.
    pub fn skills_summary(&self) -> String {
        if self.skills.is_empty() {
            return "No skills registered.".to_string();
        }

        let mut lines = vec!["Registered skills:".to_string()];
        for skill in self.skills.values() {
            let m = &skill.parsed.manifest;
            let tool_names: Vec<&str> = m.tools.iter().map(|t| t.name.as_str()).collect();
            lines.push(format!(
                "  {} v{} - {} ({} tools: {})",
                m.name,
                m.version,
                m.description,
                m.tools.len(),
                tool_names.join(", ")
            ));
        }
        lines.join("\n")
    }
}

#[async_trait]
impl mclaw_agent::agent::ToolDispatcher for SkillRegistry {
    async fn dispatch(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
        self.dispatch(tool_name, input).await
    }

    fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.tool_definitions()
    }

    fn system_prompt(&self) -> String {
        self.system_prompt()
    }

    fn skills_summary(&self) -> String {
        self.skills_summary()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A no-op handler for skills that have been discovered but not yet wired up.
struct NoOpHandler;

#[async_trait]
impl SkillHandler for NoOpHandler {
    async fn execute(&self, tool_name: &str, _input: serde_json::Value) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("no handler registered for tool: {tool_name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_skill_md;

    struct EchoHandler;

    #[async_trait]
    impl SkillHandler for EchoHandler {
        async fn execute(&self, _tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
            Ok(format!("echo: {input}"))
        }
    }

    #[tokio::test]
    async fn test_register_and_dispatch() {
        let skill_md = r#"---
name: echo
description: Echo skill
version: 0.1.0
tools:
  - name: echo_message
    description: Echo a message
    parameters:
      message:
        type: string
        description: The message
---
Echo things back.
"#;
        let parsed = parse_skill_md(skill_md).unwrap();
        let mut registry = SkillRegistry::new();
        registry.register(parsed, Box::new(EchoHandler));

        let result = registry
            .dispatch("echo_message", serde_json::json!({"message": "hello"}))
            .await
            .unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_unknown_tool_dispatch() {
        let registry = SkillRegistry::new();
        let result = registry
            .dispatch("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_definitions() {
        let skill_md = r#"---
name: test
description: Test
version: 0.1.0
tools:
  - name: test_fn
    description: A test function
    parameters:
      arg:
        type: string
        description: An argument
---
"#;
        let parsed = parse_skill_md(skill_md).unwrap();
        let mut registry = SkillRegistry::new();
        registry.register(parsed, Box::new(NoOpHandler));

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "test_fn");
    }

    #[test]
    fn test_system_prompt() {
        let skill_md = r#"---
name: test
description: Test
version: 0.1.0
tools: []
---
Be helpful.
"#;
        let parsed = parse_skill_md(skill_md).unwrap();
        let mut registry = SkillRegistry::new();
        registry.register(parsed, Box::new(NoOpHandler));

        let prompt = registry.system_prompt();
        assert!(prompt.contains("Be helpful."));
    }
}
