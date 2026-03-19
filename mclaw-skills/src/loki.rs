//! Loki/Grafana skill handler — queries logs via Loki HTTP API.

use async_trait::async_trait;
use tracing::debug;

use crate::registry::SkillHandler;

/// Loki/Grafana skill configuration.
#[derive(Debug, Clone)]
pub struct LokiConfig {
    /// Loki API base URL.
    pub loki_url: String,
    /// Optional Grafana base URL (for dashboard links).
    pub grafana_url: Option<String>,
    /// Optional Grafana API token.
    pub grafana_token: Option<String>,
}

/// Loki skill — queries logs and interacts with Grafana.
pub struct LokiSkill {
    config: LokiConfig,
    client: reqwest::Client,
}

impl LokiSkill {
    /// Create a new Loki skill handler.
    pub fn new(config: LokiConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    async fn loki_query(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'query' parameter is required"))?;
        let limit = input["limit"].as_u64().unwrap_or(50);

        debug!(query, limit, "executing LogQL query");

        let url = format!("{}/loki/api/v1/query", self.config.loki_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("query", query), ("limit", &limit.to_string())])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Loki API error {status}: {body}"));
        }

        let body: serde_json::Value = resp.json().await?;
        format_loki_response(&body, limit as usize)
    }

    async fn loki_query_range(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'query' parameter is required"))?;
        let start = input["start"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'start' parameter is required"))?;
        let end = input["end"].as_str().unwrap_or("now");
        let limit = input["limit"].as_u64().unwrap_or(100);

        debug!(query, start, end, limit, "executing LogQL range query");

        let url = format!("{}/loki/api/v1/query_range", self.config.loki_url);
        let mut params = vec![
            ("query", query.to_string()),
            ("limit", limit.to_string()),
            ("start", start.to_string()),
        ];
        if end != "now" {
            params.push(("end", end.to_string()));
        }

        let resp = self.client.get(&url).query(&params).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Loki API error {status}: {body}"));
        }

        let body: serde_json::Value = resp.json().await?;
        format_loki_response(&body, limit as usize)
    }

    async fn loki_labels(&self) -> anyhow::Result<String> {
        let url = format!("{}/loki/api/v1/labels", self.config.loki_url);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Loki API error {status}: {body}"));
        }

        let body: serde_json::Value = resp.json().await?;
        let labels = body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "No labels found.".to_string());

        Ok(format!("Available labels:\n{labels}"))
    }

    async fn loki_label_values(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let label = input["label"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'label' parameter is required"))?;

        let url = format!(
            "{}/loki/api/v1/label/{}/values",
            self.config.loki_url, label
        );
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Loki API error {status}: {body}"));
        }

        let body: serde_json::Value = resp.json().await?;
        let values = body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "No values found.".to_string());

        Ok(format!("Values for label '{label}':\n{values}"))
    }

    async fn loki_tail(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'query' parameter is required"))?;
        let lines = input["lines"].as_u64().unwrap_or(20);

        // Use the instant query endpoint with a limit
        let url = format!("{}/loki/api/v1/query", self.config.loki_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("query", query), ("limit", &lines.to_string())])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Loki API error {status}: {body}"));
        }

        let body: serde_json::Value = resp.json().await?;
        format_loki_response(&body, lines as usize)
    }
}

/// Format a Loki API response into readable log lines.
fn format_loki_response(body: &serde_json::Value, limit: usize) -> anyhow::Result<String> {
    let result = &body["data"]["result"];

    let streams = match result.as_array() {
        Some(s) if !s.is_empty() => s,
        _ => return Ok("No log entries found.".to_string()),
    };

    let mut lines = Vec::new();
    for stream in streams {
        let labels = &stream["stream"];
        let label_str = if let Some(obj) = labels.as_object() {
            obj.iter()
                .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            String::new()
        };

        if let Some(values) = stream["values"].as_array() {
            for entry in values.iter().take(limit) {
                if let Some(arr) = entry.as_array() {
                    let timestamp = arr.first().and_then(|v| v.as_str()).unwrap_or("");
                    let line = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(format!("[{label_str}] {timestamp} {line}"));
                }
            }
        }
    }

    if lines.is_empty() {
        Ok("No log entries found.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

#[async_trait]
impl SkillHandler for LokiSkill {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
        match tool_name {
            "loki_query" => self.loki_query(input).await,
            "loki_query_range" => self.loki_query_range(input).await,
            "loki_labels" => self.loki_labels().await,
            "loki_label_values" => self.loki_label_values(input).await,
            "loki_tail" => self.loki_tail(input).await,
            _ => Err(anyhow::anyhow!("unknown loki tool: {tool_name}")),
        }
    }
}
