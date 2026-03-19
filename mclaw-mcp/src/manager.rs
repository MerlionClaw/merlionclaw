//! MCP server lifecycle manager.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use tracing::{info, warn};

use crate::bridge::McpSkillBridge;
use crate::client::McpClient;
use crate::transport::{SseTransport, StdioTransport};

/// Configuration for an MCP server connection.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    /// Server name (used as tool prefix).
    pub name: String,
    /// Transport type: "stdio" or "sse".
    pub transport: String,
    /// Command to run (stdio transport).
    #[serde(default)]
    pub command: String,
    /// Command arguments (stdio transport).
    #[serde(default)]
    pub args: Vec<String>,
    /// SSE URL (sse transport).
    #[serde(default)]
    pub url: String,
    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Required permissions.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Manages multiple MCP server connections.
pub struct McpManager {
    clients: HashMap<String, Arc<McpClient>>,
}

impl McpManager {
    /// Connect to all configured MCP servers.
    pub async fn start_all(configs: &[McpServerConfig]) -> anyhow::Result<Self> {
        let mut clients = HashMap::new();

        for config in configs {
            match Self::connect_server(config).await {
                Ok(client) => {
                    info!(
                        server = %config.name,
                        tools = client.tools().len(),
                        "MCP server connected"
                    );
                    clients.insert(config.name.clone(), Arc::new(client));
                }
                Err(e) => {
                    warn!(
                        server = %config.name,
                        error = %e,
                        "failed to connect MCP server"
                    );
                }
            }
        }

        Ok(Self { clients })
    }

    async fn connect_server(config: &McpServerConfig) -> anyhow::Result<McpClient> {
        // Resolve environment variable references in env values
        let env: HashMap<String, String> = config
            .env
            .iter()
            .map(|(k, v)| {
                let resolved = if let Some(var_name) = v.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
                    std::env::var(var_name).unwrap_or_default()
                } else {
                    v.clone()
                };
                (k.clone(), resolved)
            })
            .collect();

        match config.transport.as_str() {
            "stdio" => {
                let transport =
                    StdioTransport::spawn(&config.command, &config.args, &env).await?;
                McpClient::connect(config.name.clone(), Box::new(transport)).await
            }
            "sse" => {
                let transport = SseTransport::connect(&config.url).await?;
                McpClient::connect(config.name.clone(), Box::new(transport)).await
            }
            other => anyhow::bail!("unknown MCP transport: {other}"),
        }
    }

    /// Get all tool definitions from all connected servers.
    pub fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.clients
            .values()
            .flat_map(|c| c.tool_definitions())
            .collect()
    }

    /// Create skill bridges for all connected servers.
    pub fn bridges(&self) -> Vec<McpSkillBridge> {
        self.clients
            .values()
            .map(|c| McpSkillBridge::new(c.clone()))
            .collect()
    }

    /// Close all connections.
    pub async fn stop_all(&self) {
        for (name, client) in &self.clients {
            if let Err(e) = client.close().await {
                warn!(server = %name, error = %e, "error closing MCP server");
            }
        }
    }

    /// Get a summary of all connected servers.
    pub fn summary(&self) -> String {
        if self.clients.is_empty() {
            return "No MCP servers connected.".to_string();
        }

        let mut lines = vec!["MCP servers:".to_string()];
        for (name, client) in &self.clients {
            lines.push(format!(
                "  {} ({} v{}) - {} tools",
                name,
                client.server_info().name,
                client.server_info().version,
                client.tools().len(),
            ));
        }
        lines.join("\n")
    }
}
