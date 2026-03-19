//! Incident response skill handler.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use tracing::info;

use crate::registry::SkillHandler;

/// An active incident tracked by the skill.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Incident {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub description: String,
    pub labels: HashMap<String, String>,
    pub started_at: String,
    pub status: String,
    pub acknowledged: bool,
}

/// Incident response skill — tracks and manages alerts.
pub struct IncidentSkill {
    incidents: Arc<RwLock<HashMap<String, Incident>>>,
}

impl IncidentSkill {
    /// Create a new incident skill.
    pub fn new() -> Self {
        Self {
            incidents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record an alert from a webhook.
    pub async fn record_alert(
        &self,
        id: String,
        title: String,
        severity: String,
        description: String,
        labels: HashMap<String, String>,
    ) {
        let incident = Incident {
            id: id.clone(),
            title,
            severity,
            description,
            labels,
            started_at: Utc::now().to_rfc3339(),
            status: "firing".to_string(),
            acknowledged: false,
        };
        self.incidents.write().await.insert(id, incident);
    }
}

impl Default for IncidentSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillHandler for IncidentSkill {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
        match tool_name {
            "incident_list" => {
                let incidents = self.incidents.read().await;
                if incidents.is_empty() {
                    return Ok("No active incidents.".to_string());
                }

                let mut lines = vec![format!(
                    "{:<12} {:<10} {:<30} {:<12} {}",
                    "ID", "SEVERITY", "TITLE", "STATUS", "SINCE"
                )];

                for inc in incidents.values() {
                    lines.push(format!(
                        "{:<12} {:<10} {:<30} {:<12} {}",
                        &inc.id[..inc.id.len().min(12)],
                        inc.severity,
                        &inc.title[..inc.title.len().min(30)],
                        if inc.acknowledged {
                            "acked"
                        } else {
                            &inc.status
                        },
                        inc.started_at,
                    ));
                }

                Ok(lines.join("\n"))
            }

            "incident_acknowledge" => {
                let alert_id = input["alert_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'alert_id' is required"))?;

                let mut incidents = self.incidents.write().await;
                if let Some(inc) = incidents.get_mut(alert_id) {
                    inc.acknowledged = true;
                    info!(alert_id, "incident acknowledged");
                    Ok(format!("Acknowledged incident: {}", inc.title))
                } else {
                    // Try partial match
                    let key = incidents
                        .keys()
                        .find(|k| k.contains(alert_id))
                        .cloned();
                    if let Some(key) = key {
                        let inc = incidents.get_mut(&key).unwrap();
                        inc.acknowledged = true;
                        info!(alert_id = %key, "incident acknowledged");
                        Ok(format!("Acknowledged incident: {}", inc.title))
                    } else {
                        Ok(format!("No incident found with ID: {alert_id}"))
                    }
                }
            }

            "incident_resolve" => {
                let alert_id = input["alert_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'alert_id' is required"))?;

                let mut incidents = self.incidents.write().await;
                if let Some(inc) = incidents.get_mut(alert_id) {
                    inc.status = "resolved".to_string();
                    info!(alert_id, "incident resolved");
                    Ok(format!("Resolved incident: {}", inc.title))
                } else {
                    let key = incidents
                        .keys()
                        .find(|k| k.contains(alert_id))
                        .cloned();
                    if let Some(key) = key {
                        let inc = incidents.get_mut(&key).unwrap();
                        inc.status = "resolved".to_string();
                        info!(alert_id = %key, "incident resolved");
                        Ok(format!("Resolved incident: {}", inc.title))
                    } else {
                        Ok(format!("No incident found with ID: {alert_id}"))
                    }
                }
            }

            "incident_diagnose" => {
                let alert_id = input["alert_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'alert_id' is required"))?;

                let incidents = self.incidents.read().await;
                let inc = incidents
                    .get(alert_id)
                    .or_else(|| incidents.values().find(|i| i.id.contains(alert_id)));

                match inc {
                    Some(inc) => {
                        let mut diagnosis = vec![
                            format!("Diagnosis for: {} ({})", inc.title, inc.severity),
                            String::new(),
                        ];

                        // Suggest diagnostic steps based on labels
                        if let Some(ns) = inc.labels.get("namespace") {
                            if let Some(pod) = inc.labels.get("pod") {
                                diagnosis.push(format!(
                                    "Suggested checks for pod {pod} in {ns}:"
                                ));
                                diagnosis.push(format!(
                                    "1. k8s_describe_pod(pod=\"{pod}\", namespace=\"{ns}\")"
                                ));
                                diagnosis.push(format!(
                                    "2. k8s_get_logs(pod=\"{pod}\", namespace=\"{ns}\", lines=50)"
                                ));
                            }
                            if let Some(deploy) = inc.labels.get("deployment") {
                                diagnosis.push(format!(
                                    "3. k8s_list_pods(namespace=\"{ns}\", label_selector=\"app={deploy}\")"
                                ));
                            }
                        }

                        if let Some(app) = inc.labels.get("app") {
                            diagnosis.push(format!(
                                "4. loki_query(query='{{app=\"{app}\"}} |= \"error\"', limit=20)"
                            ));
                        }

                        diagnosis.push(String::new());
                        diagnosis.push(
                            "Use these tool calls to investigate further.".to_string(),
                        );

                        Ok(diagnosis.join("\n"))
                    }
                    None => Ok(format!("No incident found with ID: {alert_id}")),
                }
            }

            _ => Err(anyhow::anyhow!("unknown incident tool: {tool_name}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_empty() {
        let skill = IncidentSkill::new();
        let result = skill
            .execute("incident_list", serde_json::json!({}))
            .await
            .unwrap();
        assert!(result.contains("No active incidents"));
    }

    #[tokio::test]
    async fn test_record_and_list() {
        let skill = IncidentSkill::new();
        skill
            .record_alert(
                "test-1".to_string(),
                "PodMemoryHigh".to_string(),
                "critical".to_string(),
                "Memory above 90%".to_string(),
                HashMap::from([("namespace".to_string(), "prod".to_string())]),
            )
            .await;

        let result = skill
            .execute("incident_list", serde_json::json!({}))
            .await
            .unwrap();
        assert!(result.contains("PodMemoryHigh"));
    }

    #[tokio::test]
    async fn test_acknowledge() {
        let skill = IncidentSkill::new();
        skill
            .record_alert(
                "test-1".to_string(),
                "TestAlert".to_string(),
                "high".to_string(),
                "test".to_string(),
                HashMap::new(),
            )
            .await;

        let result = skill
            .execute(
                "incident_acknowledge",
                serde_json::json!({"alert_id": "test-1"}),
            )
            .await
            .unwrap();
        assert!(result.contains("Acknowledged"));
    }

    #[tokio::test]
    async fn test_diagnose() {
        let skill = IncidentSkill::new();
        skill
            .record_alert(
                "test-1".to_string(),
                "PodMemoryHigh".to_string(),
                "critical".to_string(),
                "Memory high".to_string(),
                HashMap::from([
                    ("namespace".to_string(), "production".to_string()),
                    ("pod".to_string(), "api-server-abc".to_string()),
                ]),
            )
            .await;

        let result = skill
            .execute(
                "incident_diagnose",
                serde_json::json!({"alert_id": "test-1"}),
            )
            .await
            .unwrap();
        assert!(result.contains("k8s_describe_pod"));
        assert!(result.contains("k8s_get_logs"));
    }
}
