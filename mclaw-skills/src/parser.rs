//! SKILL.md parser — extracts YAML frontmatter and markdown body.

use indexmap::IndexMap;
use serde::Deserialize;

/// The parsed YAML frontmatter from a SKILL.md file.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillManifest {
    /// Unique skill name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Skill version.
    pub version: String,
    /// Required permissions (e.g., "k8s:read").
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Tool definitions.
    #[serde(default)]
    pub tools: Vec<SkillToolDef>,
}

/// A tool defined in a SKILL.md frontmatter.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillToolDef {
    /// Tool name (e.g., "k8s_list_pods").
    pub name: String,
    /// Tool description for the LLM.
    pub description: String,
    /// Tool parameters.
    #[serde(default)]
    pub parameters: IndexMap<String, ParameterDef>,
}

/// A parameter definition for a tool.
#[derive(Debug, Clone, Deserialize)]
pub struct ParameterDef {
    /// Parameter type: string, integer, boolean, array.
    #[serde(rename = "type")]
    pub param_type: String,
    /// Parameter description.
    pub description: String,
    /// Whether this parameter is required (default: true).
    pub required: Option<bool>,
    /// Default value.
    pub default: Option<serde_json::Value>,
}

impl ParameterDef {
    /// Whether this parameter is required.
    pub fn is_required(&self) -> bool {
        self.required.unwrap_or(self.default.is_none())
    }
}

/// A fully parsed SKILL.md file.
#[derive(Debug, Clone)]
pub struct ParsedSkill {
    /// The parsed frontmatter.
    pub manifest: SkillManifest,
    /// The markdown body (used as system prompt fragment).
    pub system_prompt_fragment: String,
}

/// Parse a SKILL.md file content into a `ParsedSkill`.
pub fn parse_skill_md(content: &str) -> anyhow::Result<ParsedSkill> {
    let content = content.trim();

    // Must start with ---
    if !content.starts_with("---") {
        anyhow::bail!("SKILL.md must start with YAML frontmatter (---)");
    }

    // Find the closing ---
    let rest = &content[3..];
    let end = rest
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("SKILL.md missing closing --- for frontmatter"))?;

    let yaml_str = &rest[..end];
    let body = rest[end + 4..].trim().to_string();

    let manifest: SkillManifest = serde_yaml::from_str(yaml_str)?;

    Ok(ParsedSkill {
        manifest,
        system_prompt_fragment: body,
    })
}

impl SkillToolDef {
    /// Convert to an LLM `ToolDefinition`.
    pub fn to_tool_definition(&self) -> mclaw_agent::llm::ToolDefinition {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (name, param) in &self.parameters {
            let mut prop = serde_json::Map::new();
            prop.insert(
                "type".to_string(),
                serde_json::Value::String(param.param_type.clone()),
            );
            prop.insert(
                "description".to_string(),
                serde_json::Value::String(param.description.clone()),
            );
            properties.insert(name.clone(), serde_json::Value::Object(prop));

            if param.is_required() {
                required.push(serde_json::Value::String(name.clone()));
            }
        }

        mclaw_agent::llm::ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL: &str = r#"---
name: test_skill
description: A test skill
version: 0.1.0
permissions:
  - test:read
tools:
  - name: test_echo
    description: Echo input back
    parameters:
      message:
        type: string
        description: Message to echo
        required: true
  - name: test_greet
    description: Greet someone
    parameters:
      name:
        type: string
        description: Name to greet
      greeting:
        type: string
        description: Greeting prefix
        required: false
        default: "Hello"
---

# Test Skill

This is a test skill for unit testing.
"#;

    #[test]
    fn test_parse_skill_md() {
        let parsed = parse_skill_md(SAMPLE_SKILL).unwrap();
        assert_eq!(parsed.manifest.name, "test_skill");
        assert_eq!(parsed.manifest.description, "A test skill");
        assert_eq!(parsed.manifest.version, "0.1.0");
        assert_eq!(parsed.manifest.permissions, vec!["test:read"]);
        assert_eq!(parsed.manifest.tools.len(), 2);
        assert!(parsed.system_prompt_fragment.contains("# Test Skill"));
    }

    #[test]
    fn test_tool_definitions() {
        let parsed = parse_skill_md(SAMPLE_SKILL).unwrap();
        let tool = &parsed.manifest.tools[0];
        let def = tool.to_tool_definition();
        assert_eq!(def.name, "test_echo");
        assert_eq!(def.input_schema["properties"]["message"]["type"], "string");
        assert_eq!(def.input_schema["required"][0], "message");
    }

    #[test]
    fn test_parameter_required_default() {
        let parsed = parse_skill_md(SAMPLE_SKILL).unwrap();
        // test_echo.message: required=true explicitly
        assert!(parsed.manifest.tools[0].parameters["message"].is_required());
        // test_greet.greeting: required=false explicitly
        assert!(!parsed.manifest.tools[1].parameters["greeting"].is_required());
        // test_greet.name: no required field, no default → required
        assert!(parsed.manifest.tools[1].parameters["name"].is_required());
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let result = parse_skill_md("just some text");
        assert!(result.is_err());
    }
}
