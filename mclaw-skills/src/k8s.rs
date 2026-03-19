//! Kubernetes skill handler.

#[cfg(feature = "k8s")]
mod inner {
    use async_trait::async_trait;
    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::core::v1::Pod;
    use kube::api::{Api, ListParams, LogParams};
    use kube::Client;
    use tracing::debug;

    use crate::registry::SkillHandler;

    /// Kubernetes skill — executes K8s API operations.
    pub struct K8sSkill {
        client: Client,
    }

    impl K8sSkill {
        /// Create a new K8s skill, using in-cluster or kubeconfig auth.
        pub async fn new() -> anyhow::Result<Self> {
            let client = Client::try_default().await?;
            Ok(Self { client })
        }

        async fn list_pods(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

            let mut lp = ListParams::default();
            if let Some(selector) = input["label_selector"].as_str() {
                lp = lp.labels(selector);
            }

            let pod_list = pods.list(&lp).await?;
            if pod_list.items.is_empty() {
                return Ok(format!("No pods found in namespace '{namespace}'."));
            }

            let mut lines = vec![format!(
                "{:<40} {:<12} {:<10} {:<8} {}",
                "NAME", "STATUS", "RESTARTS", "AGE", "IP"
            )];

            for pod in &pod_list.items {
                let name = pod.metadata.name.as_deref().unwrap_or("<unknown>");
                let status = pod
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.as_deref())
                    .unwrap_or("Unknown");
                let restarts: i32 = pod
                    .status
                    .as_ref()
                    .and_then(|s| s.container_statuses.as_ref())
                    .map(|cs| cs.iter().map(|c| c.restart_count).sum())
                    .unwrap_or(0);
                let ip = pod
                    .status
                    .as_ref()
                    .and_then(|s| s.pod_ip.as_deref())
                    .unwrap_or("<none>");
                let age = pod
                    .metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| format_age(&t.0))
                    .unwrap_or_else(|| "<unknown>".to_string());

                lines.push(format!(
                    "{:<40} {:<12} {:<10} {:<8} {}",
                    name, status, restarts, age, ip
                ));
            }

            Ok(lines.join("\n"))
        }

        async fn get_logs(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            let pod_name = input["pod"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'pod' parameter is required"))?;
            let lines = input["lines"].as_u64().unwrap_or(50);

            let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

            let mut log_params = LogParams {
                tail_lines: Some(lines as i64),
                ..Default::default()
            };

            if let Some(container) = input["container"].as_str() {
                log_params.container = Some(container.to_string());
            }

            debug!(pod = pod_name, namespace, lines, "fetching pod logs");
            let logs = pods.logs(pod_name, &log_params).await?;

            if logs.is_empty() {
                Ok(format!("No logs found for pod '{pod_name}' in namespace '{namespace}'."))
            } else {
                Ok(logs)
            }
        }

        async fn list_deployments(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);

            let mut lp = ListParams::default();
            if let Some(selector) = input["label_selector"].as_str() {
                lp = lp.labels(selector);
            }

            let deploy_list = deployments.list(&lp).await?;
            if deploy_list.items.is_empty() {
                return Ok(format!("No deployments found in namespace '{namespace}'."));
            }

            let mut lines = vec![format!(
                "{:<40} {:<8} {:<8} {:<10} {}",
                "NAME", "READY", "UP-TO-DATE", "AVAILABLE", "AGE"
            )];

            for deploy in &deploy_list.items {
                let name = deploy.metadata.name.as_deref().unwrap_or("<unknown>");
                let status = deploy.status.as_ref();
                let replicas = status.and_then(|s| s.replicas).unwrap_or(0);
                let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
                let updated = status.and_then(|s| s.updated_replicas).unwrap_or(0);
                let available = status.and_then(|s| s.available_replicas).unwrap_or(0);
                let age = deploy
                    .metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| format_age(&t.0))
                    .unwrap_or_else(|| "<unknown>".to_string());

                lines.push(format!(
                    "{:<40} {}/{:<5} {:<8} {:<10} {}",
                    name, ready, replicas, updated, available, age
                ));
            }

            Ok(lines.join("\n"))
        }

        async fn describe_pod(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            let pod_name = input["pod"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'pod' parameter is required"))?;

            let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
            let pod = pods.get(pod_name).await?;

            let mut output = Vec::new();

            output.push(format!("Name:       {}", pod.metadata.name.as_deref().unwrap_or("")));
            output.push(format!("Namespace:  {namespace}"));

            if let Some(labels) = &pod.metadata.labels {
                let label_str: Vec<String> = labels.iter().map(|(k, v)| format!("{k}={v}")).collect();
                output.push(format!("Labels:     {}", label_str.join(", ")));
            }

            if let Some(status) = &pod.status {
                output.push(format!("Phase:      {}", status.phase.as_deref().unwrap_or("Unknown")));
                output.push(format!("Pod IP:     {}", status.pod_ip.as_deref().unwrap_or("<none>")));
                output.push(format!("Host IP:    {}", status.host_ip.as_deref().unwrap_or("<none>")));

                if let Some(conditions) = &status.conditions {
                    output.push("Conditions:".to_string());
                    for cond in conditions {
                        output.push(format!(
                            "  {}: {} ({})",
                            cond.type_,
                            cond.status,
                            cond.reason.as_deref().unwrap_or("")
                        ));
                    }
                }

                if let Some(containers) = &status.container_statuses {
                    output.push("Containers:".to_string());
                    for cs in containers {
                        output.push(format!("  {}:", cs.name));
                        output.push(format!("    Ready:    {}", cs.ready));
                        output.push(format!("    Restarts: {}", cs.restart_count));
                        if let Some(state) = &cs.state {
                            if let Some(running) = &state.running {
                                output.push(format!(
                                    "    State:    Running (since {})",
                                    running
                                        .started_at
                                        .as_ref()
                                        .map(|t| t.0.to_rfc3339())
                                        .unwrap_or_default()
                                ));
                            } else if let Some(waiting) = &state.waiting {
                                output.push(format!(
                                    "    State:    Waiting ({})",
                                    waiting.reason.as_deref().unwrap_or("unknown")
                                ));
                            } else if let Some(terminated) = &state.terminated {
                                output.push(format!(
                                    "    State:    Terminated (exit {})",
                                    terminated.exit_code
                                ));
                            }
                        }
                    }
                }
            }

            Ok(output.join("\n"))
        }
    }

    #[async_trait]
    impl SkillHandler for K8sSkill {
        async fn execute(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
            match tool_name {
                "k8s_list_pods" => self.list_pods(input).await,
                "k8s_get_logs" => self.get_logs(input).await,
                "k8s_list_deployments" => self.list_deployments(input).await,
                "k8s_describe_pod" => self.describe_pod(input).await,
                _ => Err(anyhow::anyhow!("unknown k8s tool: {tool_name}")),
            }
        }
    }

    fn format_age(time: &chrono::DateTime<chrono::Utc>) -> String {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(time);

        if duration.num_days() > 0 {
            format!("{}d", duration.num_days())
        } else if duration.num_hours() > 0 {
            format!("{}h", duration.num_hours())
        } else if duration.num_minutes() > 0 {
            format!("{}m", duration.num_minutes())
        } else {
            format!("{}s", duration.num_seconds())
        }
    }
}

#[cfg(feature = "k8s")]
pub use inner::K8sSkill;
