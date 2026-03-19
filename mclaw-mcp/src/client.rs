//! MCP client — manages sessions with MCP servers.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::protocol::*;
use crate::transport::McpTransport;

/// MCP client for communicating with an MCP server.
pub struct McpClient {
    transport: Arc<Mutex<Box<dyn McpTransport>>>,
    server_info: ServerInfo,
    tools: Vec<McpToolDef>,
    next_id: AtomicU64,
    server_name: String,
}

impl McpClient {
    /// Connect to an MCP server and initialize the session.
    pub async fn connect(
        server_name: String,
        mut transport: Box<dyn McpTransport>,
    ) -> anyhow::Result<Self> {
        // Send initialize
        let init_req = JsonRpcRequest::new(
            1,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "merlionclaw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        transport.send(&init_req).await?;
        let init_resp = transport.receive().await?;

        let init_result: InitializeResult = match init_resp.result {
            Some(r) => serde_json::from_value(r)?,
            None => {
                let err = init_resp
                    .error
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                anyhow::bail!("MCP initialize failed: {err}");
            }
        };

        info!(
            server = %init_result.server_info.name,
            version = %init_result.server_info.version,
            protocol = %init_result.protocol_version,
            "MCP server initialized"
        );

        // Send initialized notification
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        transport.send(&notif).await?;

        // List tools if supported
        let mut tools = Vec::new();
        if init_result.capabilities.tools.is_some() {
            let list_req = JsonRpcRequest::new(2, "tools/list", None);
            transport.send(&list_req).await?;
            let list_resp = transport.receive().await?;

            if let Some(result) = list_resp.result {
                if let Some(tool_array) = result.get("tools") {
                    tools = serde_json::from_value(tool_array.clone()).unwrap_or_default();
                }
            }

            info!(count = tools.len(), "discovered MCP tools");
        }

        Ok(Self {
            transport: Arc::new(Mutex::new(transport)),
            server_info: init_result.server_info,
            tools,
            next_id: AtomicU64::new(10),
            server_name,
        })
    }

    /// Call an MCP tool by name.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<McpToolResult> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(
            id,
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments,
            })),
        );

        let mut transport = self.transport.lock().await;
        transport.send(&req).await?;
        let resp = transport.receive().await?;

        match resp.result {
            Some(r) => {
                let result: McpToolResult = serde_json::from_value(r)?;
                debug!(tool = name, "MCP tool call completed");
                Ok(result)
            }
            None => {
                let err = resp
                    .error
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                anyhow::bail!("MCP tool call failed: {err}")
            }
        }
    }

    /// Get available tools as LLM ToolDefinitions (prefixed with server name).
    pub fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.tools
            .iter()
            .map(|t| mclaw_agent::llm::ToolDefinition {
                name: format!("mcp_{}_{}", self.server_name, t.name),
                description: t.description.clone().unwrap_or_default(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }

    /// Get the server name.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Get the server info.
    pub fn server_info(&self) -> &ServerInfo {
        &self.server_info
    }

    /// Get the raw tool definitions.
    pub fn tools(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// Close the connection.
    pub async fn close(&self) -> anyhow::Result<()> {
        let mut transport = self.transport.lock().await;
        transport.close().await
    }
}
