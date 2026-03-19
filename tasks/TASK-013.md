# TASK-013: MCP Bridge (Model Context Protocol Client)

## Objective
Implement an MCP client that allows MerlionClaw to connect to external MCP servers, reusing the existing OpenClaw/Claude ecosystem of MCP-compatible tools (GitHub, Jira, Confluence, Google Drive, Asana, Linear, Sentry, etc.).

## Dependencies
- TASK-004 must be complete (skill engine)
- TASK-006 must be complete (working MVP)

## Background

MCP (Model Context Protocol) is an open standard for connecting LLMs to external tools and data sources. By implementing an MCP client, MerlionClaw gains instant access to hundreds of existing MCP servers without building custom integrations.

MCP uses JSON-RPC 2.0 over:
- **stdio**: for local MCP servers (spawn process, communicate via stdin/stdout)
- **SSE (Server-Sent Events)**: for remote MCP servers (HTTP POST for requests, SSE for responses)
- **Streamable HTTP**: newer transport, POST with streaming response

## Steps

### 1. MCP protocol types (mclaw-mcp/src/protocol.rs)

```rust
/// JSON-RPC 2.0 request
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,  // "2.0"
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// MCP initialize response
#[derive(Debug, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Deserialize)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
    pub prompts: Option<PromptsCapability>,
}

/// MCP tool definition (from tools/list)
#[derive(Debug, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// MCP tool call result (from tools/call)
#[derive(Debug, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: McpResource },
}
```

### 2. MCP transport layer (mclaw-mcp/src/transport.rs)

#### Stdio transport

```rust
pub struct StdioTransport {
    child: tokio::process::Child,
    stdin: tokio::io::BufWriter<ChildStdin>,
    stdout: tokio::io::BufReader<ChildStdout>,
}

impl StdioTransport {
    pub async fn spawn(command: &str, args: &[&str], env: &HashMap<String, String>) -> Result<Self> {
        let child = tokio::process::Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // ...
    }

    pub async fn send(&mut self, request: &JsonRpcRequest) -> Result<()> {
        // Write JSON + newline to stdin
    }

    pub async fn receive(&mut self) -> Result<JsonRpcResponse> {
        // Read JSON line from stdout
    }
}
```

#### SSE transport

```rust
pub struct SseTransport {
    base_url: String,
    client: reqwest::Client,
    session_id: Option<String>,
}

impl SseTransport {
    pub async fn connect(url: &str) -> Result<Self> {
        // GET {url}/sse to establish SSE connection
        // Parse endpoint message to get POST URL
    }

    pub async fn send(&self, request: &JsonRpcRequest) -> Result<()> {
        // POST to the endpoint URL with JSON-RPC body
    }

    pub async fn receive(&mut self) -> Result<JsonRpcResponse> {
        // Read from SSE stream, parse JSON-RPC response from "message" events
    }
}
```

### 3. MCP client (mclaw-mcp/src/client.rs)

```rust
pub struct McpClient {
    transport: Box<dyn McpTransport>,
    server_info: Option<ServerInfo>,
    tools: Vec<McpToolDef>,
    next_id: AtomicU64,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<()>;
    async fn receive(&mut self) -> Result<JsonRpcResponse>;
    async fn close(&mut self) -> Result<()>;
}

impl McpClient {
    /// Connect and initialize the MCP session
    pub async fn connect(transport: Box<dyn McpTransport>) -> Result<Self> {
        let mut client = Self { transport, server_info: None, tools: vec![], next_id: AtomicU64::new(1) };

        // Send initialize request
        let init_result: InitializeResult = client.call("initialize", json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "merlionclaw",
                "version": env!("CARGO_PKG_VERSION")
            }
        })).await?;

        client.server_info = Some(init_result.server_info);

        // Send initialized notification
        client.notify("notifications/initialized", None).await?;

        // List available tools
        if init_result.capabilities.tools.is_some() {
            let tools_result = client.call("tools/list", None).await?;
            client.tools = serde_json::from_value(tools_result["tools"].clone())?;
        }

        Ok(client)
    }

    /// Call an MCP tool
    pub async fn call_tool(&mut self, name: &str, arguments: serde_json::Value) -> Result<McpToolResult> {
        let result = self.call("tools/call", json!({
            "name": name,
            "arguments": arguments,
        })).await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Get available tools as LLM ToolDefinitions
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| ToolDefinition {
            name: format!("mcp_{}", t.name),  // prefix to avoid collision
            description: t.description.clone().unwrap_or_default(),
            input_schema: t.input_schema.clone(),
        }).collect()
    }
}
```

### 4. MCP Skill Handler bridge

Wrap MCP tools as a SkillHandler so they integrate with the existing skill registry:

```rust
pub struct McpSkillBridge {
    client: Arc<Mutex<McpClient>>,
    server_name: String,
}

#[async_trait]
impl SkillHandler for McpSkillBridge {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> Result<String> {
        // Strip "mcp_" prefix to get original MCP tool name
        let mcp_tool_name = tool_name.strip_prefix("mcp_")
            .ok_or_else(|| anyhow!("invalid MCP tool name"))?;

        let mut client = self.client.lock().await;
        let result = client.call_tool(mcp_tool_name, input).await?;

        // Convert MCP result to string
        let text = result.content.iter()
            .filter_map(|c| match c {
                McpContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_error.unwrap_or(false) {
            Err(anyhow!("MCP tool error: {}", text))
        } else {
            Ok(text)
        }
    }
}
```

### 5. Config for MCP servers

```toml
[[mcp.servers]]
name = "github"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}" }
permissions = ["net:github"]

[[mcp.servers]]
name = "sentry"
transport = "sse"
url = "https://mcp.sentry.dev/sse"
env = { SENTRY_AUTH_TOKEN = "${SENTRY_TOKEN}" }
permissions = ["net:sentry"]

[[mcp.servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/larry/projects"]
permissions = ["fs:read", "fs:write"]
```

### 6. Auto-discovery and connection

On startup:
1. Read `mcp.servers` from config
2. For each server, spawn/connect transport
3. Initialize MCP session
4. Discover tools
5. Register as skills in the SkillRegistry with `mcp_{server_name}_` prefix

Handle reconnection:
- If stdio process crashes, restart it
- If SSE connection drops, reconnect with backoff
- Log connection status

### 7. MCP server lifecycle management

```rust
pub struct McpManager {
    servers: HashMap<String, McpClient>,
}

impl McpManager {
    pub async fn start_all(configs: &[McpServerConfig]) -> Result<Self>;
    pub async fn stop_all(&mut self) -> Result<()>;
    pub fn list_tools(&self) -> Vec<ToolDefinition>;
    pub async fn call_tool(&self, tool_name: &str, input: serde_json::Value) -> Result<String>;
}
```

## Validation

```bash
cargo test -p mclaw-mcp

# Test with a real MCP server (GitHub):
export GITHUB_TOKEN=ghp_xxx

# Config:
# [[mcp.servers]]
# name = "github"
# transport = "stdio"
# command = "npx"
# args = ["-y", "@modelcontextprotocol/server-github"]

cargo run -- run

You: "list my open pull requests on GitHub"
Bot: [list of PRs from GitHub MCP server]

You: "show me recent Sentry errors for the api project"
Bot: [Sentry error list from Sentry MCP server]

# Check tool discovery:
You: /skills
Bot: "Skills: k8s (4 tools), helm (7 tools), mcp_github (12 tools), mcp_sentry (5 tools)..."
```

## Output

A working MCP client that connects to any MCP-compatible server (stdio or SSE transport), discovers tools, and exposes them through the standard skill registry. This gives MerlionClaw instant access to the entire MCP ecosystem without building custom integrations.
