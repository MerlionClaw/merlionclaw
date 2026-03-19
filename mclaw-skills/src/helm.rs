//! Helm skill handler — manages Helm releases via CLI wrapper.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use crate::registry::SkillHandler;

/// Helm skill — executes Helm CLI operations.
pub struct HelmSkill {
    helm_binary: PathBuf,
}

impl HelmSkill {
    /// Create a new Helm skill, verifying the helm binary exists.
    pub async fn new() -> anyhow::Result<Self> {
        let helm_binary = PathBuf::from("helm");

        // Verify helm is available
        let output = Command::new(&helm_binary)
            .args(["version", "--short"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("helm binary not found: {e}"))?;

        if !output.status.success() {
            anyhow::bail!("helm version check failed");
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!(version, "helm detected");
        Ok(Self { helm_binary })
    }

    async fn run_helm(&self, args: &[&str]) -> anyhow::Result<String> {
        debug!(args = ?args, "running helm");
        let output = Command::new(&self.helm_binary)
            .args(args)
            .output()
            .await?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("helm error: {stderr}"))
        }
    }

    async fn helm_list(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let mut args = vec!["list"];

        let namespace = input["namespace"].as_str();
        let ns_string;
        if let Some(ns) = namespace {
            args.push("-n");
            ns_string = ns.to_string();
            args.push(&ns_string);
        } else {
            args.push("-A");
        }

        args.push("-o");
        args.push("table");

        let mut output = self.run_helm(&args).await?;

        // Apply filter if provided
        if let Some(filter) = input["filter"].as_str() {
            let lines: Vec<&str> = output.lines().collect();
            let filtered: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(i, line)| *i == 0 || line.to_lowercase().contains(&filter.to_lowercase()))
                .map(|(_, line)| *line)
                .collect();
            output = filtered.join("\n");
        }

        if output.trim().is_empty() {
            Ok("No Helm releases found.".to_string())
        } else {
            Ok(output)
        }
    }

    async fn helm_status(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let release = input["release"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'release' parameter is required"))?;
        let namespace = input["namespace"].as_str().unwrap_or("default");

        self.run_helm(&["status", release, "-n", namespace]).await
    }

    async fn helm_history(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let release = input["release"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'release' parameter is required"))?;
        let namespace = input["namespace"].as_str().unwrap_or("default");

        self.run_helm(&["history", release, "-n", namespace]).await
    }

    async fn helm_values(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let release = input["release"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'release' parameter is required"))?;
        let namespace = input["namespace"].as_str().unwrap_or("default");

        self.run_helm(&["get", "values", release, "-n", namespace])
            .await
    }

    async fn helm_upgrade(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let release = input["release"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'release' parameter is required"))?;
        let chart = input["chart"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'chart' parameter is required"))?;
        let namespace = input["namespace"].as_str().unwrap_or("default");

        let mut args = vec!["upgrade", release, chart, "-n", namespace, "--wait", "--timeout", "5m"];

        // Collect --set args
        let set_args: Vec<String> = if let Some(sets) = input["set"].as_array() {
            sets.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else {
            vec![]
        };

        let mut extra: Vec<String> = Vec::new();
        for s in &set_args {
            extra.push("--set".to_string());
            extra.push(s.clone());
        }

        let extra_refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&extra_refs);

        self.run_helm(&args).await
    }

    async fn helm_rollback(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let release = input["release"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'release' parameter is required"))?;
        let namespace = input["namespace"].as_str().unwrap_or("default");

        let revision_str;
        let mut args = vec!["rollback", release];

        if let Some(rev) = input["revision"].as_u64() {
            revision_str = rev.to_string();
            args.push(&revision_str);
        }

        args.extend_from_slice(&["-n", namespace, "--wait"]);

        self.run_helm(&args).await
    }

    async fn helm_uninstall(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let release = input["release"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'release' parameter is required"))?;
        let namespace = input["namespace"].as_str().unwrap_or("default");

        self.run_helm(&["uninstall", release, "-n", namespace])
            .await
    }
}

#[async_trait]
impl SkillHandler for HelmSkill {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
        match tool_name {
            "helm_list" => self.helm_list(input).await,
            "helm_status" => self.helm_status(input).await,
            "helm_history" => self.helm_history(input).await,
            "helm_values" => self.helm_values(input).await,
            "helm_upgrade" => self.helm_upgrade(input).await,
            "helm_rollback" => self.helm_rollback(input).await,
            "helm_uninstall" => self.helm_uninstall(input).await,
            _ => Err(anyhow::anyhow!("unknown helm tool: {tool_name}")),
        }
    }
}
