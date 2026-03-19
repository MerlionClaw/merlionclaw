//! MCP transport implementations (stdio and SSE).

use std::collections::HashMap;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, info};

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Transport trait for MCP communication.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request.
    async fn send(&mut self, request: &JsonRpcRequest) -> anyhow::Result<()>;

    /// Receive a JSON-RPC response.
    async fn receive(&mut self) -> anyhow::Result<JsonRpcResponse>;

    /// Close the transport.
    async fn close(&mut self) -> anyhow::Result<()>;
}

/// Stdio transport — communicates with a local MCP server process via stdin/stdout.
pub struct StdioTransport {
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
}

impl StdioTransport {
    /// Spawn an MCP server process.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> anyhow::Result<Self> {
        info!(command, ?args, "spawning MCP server");

        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;

        Ok(Self {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&mut self, request: &JsonRpcRequest) -> anyhow::Result<()> {
        let json = serde_json::to_string(request)?;
        debug!(json = %json, "sending to MCP server");
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn receive(&mut self) -> anyhow::Result<JsonRpcResponse> {
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;

        if line.is_empty() {
            anyhow::bail!("MCP server closed connection");
        }

        debug!(line = %line.trim(), "received from MCP server");
        let response: JsonRpcResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    async fn close(&mut self) -> anyhow::Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}

/// SSE transport — communicates with a remote MCP server via HTTP POST + SSE.
pub struct SseTransport {
    base_url: String,
    endpoint_url: Option<String>,
    client: reqwest::Client,
}

impl SseTransport {
    /// Connect to an SSE MCP server.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        info!(url, "connecting to MCP SSE server");

        let client = reqwest::Client::new();

        // The SSE endpoint tells us where to POST requests
        // For now, assume endpoint is at the same base URL
        Ok(Self {
            base_url: url.trim_end_matches('/').to_string(),
            endpoint_url: None,
            client,
        })
    }

    /// Set the endpoint URL for POST requests (discovered from SSE).
    pub fn set_endpoint(&mut self, url: String) {
        self.endpoint_url = Some(url);
    }

    fn post_url(&self) -> String {
        self.endpoint_url
            .clone()
            .unwrap_or_else(|| format!("{}/message", self.base_url))
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn send(&mut self, request: &JsonRpcRequest) -> anyhow::Result<()> {
        let url = self.post_url();
        debug!(url = %url, "sending to MCP SSE server");

        let resp = self.client.post(&url).json(request).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("MCP SSE error {status}: {body}");
        }

        // For SSE transport, the response comes back inline in the POST response
        // Store it for the next receive() call
        Ok(())
    }

    async fn receive(&mut self) -> anyhow::Result<JsonRpcResponse> {
        // For streamable HTTP, the response is in the POST response body
        // For traditional SSE, we'd read from the SSE stream
        // This simplified implementation uses the POST response directly
        let url = self.post_url();
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;

        // Parse SSE events — look for data: lines
        for line in body.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(data) {
                    return Ok(response);
                }
            }
        }

        // Try parsing the whole body as JSON-RPC
        let response: JsonRpcResponse = serde_json::from_str(&body)?;
        Ok(response)
    }

    async fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
