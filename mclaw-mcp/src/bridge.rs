//! MCP skill bridge — wraps MCP tools as a SkillHandler.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::debug;

use crate::client::McpClient;
use crate::protocol::McpContent;

/// Bridges MCP tools into the MerlionClaw skill system.
pub struct McpSkillBridge {
    client: Arc<McpClient>,
    server_name: String,
}

impl McpSkillBridge {
    /// Create a new bridge for an MCP client.
    pub fn new(client: Arc<McpClient>) -> Self {
        let server_name = client.server_name().to_string();
        Self {
            client,
            server_name,
        }
    }

    /// Get tool definitions for registration.
    pub fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.client.tool_definitions()
    }

    /// Get the MCP tool name prefix.
    pub fn tool_prefix(&self) -> String {
        format!("mcp_{}_", self.server_name)
    }
}

#[async_trait]
impl mclaw_agent::agent::ToolDispatcher for McpSkillBridge {
    async fn dispatch(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<String> {
        // Strip the mcp_{server}_ prefix to get the original MCP tool name
        let prefix = self.tool_prefix();
        let mcp_tool_name = tool_name
            .strip_prefix(&prefix)
            .ok_or_else(|| anyhow::anyhow!("invalid MCP tool name: {tool_name}"))?;

        debug!(
            server = %self.server_name,
            tool = mcp_tool_name,
            "calling MCP tool"
        );

        let result = self.client.call_tool(mcp_tool_name, input).await?;

        // Convert MCP content to string
        let text: String = result
            .content
            .iter()
            .filter_map(|c| match c {
                McpContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_error.unwrap_or(false) {
            Err(anyhow::anyhow!("MCP tool error: {text}"))
        } else {
            Ok(text)
        }
    }

    fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.client.tool_definitions()
    }

    fn system_prompt(&self) -> String {
        String::new()
    }

    fn skills_summary(&self) -> String {
        format!(
            "MCP server '{}' ({} v{}, {} tools)",
            self.server_name,
            self.client.server_info().name,
            self.client.server_info().version,
            self.client.tools().len(),
        )
    }
}
