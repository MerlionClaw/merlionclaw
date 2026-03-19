//! Alert types and webhook payload parsing.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A normalized alert from any source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert identifier.
    pub id: String,
    /// Alert source platform.
    pub source: AlertSource,
    /// Alert severity.
    pub severity: Severity,
    /// Alert title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Labels (namespace, pod, service, etc.).
    pub labels: HashMap<String, String>,
    /// When the alert started firing.
    pub started_at: DateTime<Utc>,
    /// Current status.
    pub status: AlertStatus,
}

/// Where the alert came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSource {
    Alertmanager,
    PagerDuty,
    OpsGenie,
    Custom,
}

/// Alert severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

/// Alert status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertStatus {
    Firing,
    Resolved,
}

/// Parse an Alertmanager webhook payload into normalized alerts.
pub fn parse_alertmanager(payload: &serde_json::Value) -> Vec<Alert> {
    let mut alerts = Vec::new();

    let items = match payload["alerts"].as_array() {
        Some(a) => a,
        None => return alerts,
    };

    for item in items {
        let labels: HashMap<String, String> = item["labels"]
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let severity = match labels.get("severity").map(|s| s.as_str()) {
            Some("critical") => Severity::Critical,
            Some("high") | Some("warning") => Severity::High,
            Some("medium") => Severity::Medium,
            Some("low") | Some("info") => Severity::Low,
            _ => Severity::Medium,
        };

        let title = labels
            .get("alertname")
            .cloned()
            .unwrap_or_else(|| "Unknown Alert".to_string());

        let description = item["annotations"]["summary"]
            .as_str()
            .or(item["annotations"]["description"].as_str())
            .unwrap_or("")
            .to_string();

        let started_at = item["startsAt"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let status = match item["status"].as_str() {
            Some("resolved") => AlertStatus::Resolved,
            _ => AlertStatus::Firing,
        };

        let id = item["fingerprint"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        alerts.push(Alert {
            id,
            source: AlertSource::Alertmanager,
            severity,
            title,
            description,
            labels,
            started_at,
            status,
        });
    }

    alerts
}

/// Parse a PagerDuty webhook payload into normalized alerts.
pub fn parse_pagerduty(payload: &serde_json::Value) -> Vec<Alert> {
    let mut alerts = Vec::new();

    // PagerDuty v3 Events API
    let event = payload;
    let title = event["event"]["data"]["title"]
        .as_str()
        .or(event["messages"][0]["incident"]["title"].as_str())
        .unwrap_or("PagerDuty Incident")
        .to_string();

    let severity_str = event["event"]["data"]["priority"]["name"]
        .as_str()
        .unwrap_or("medium");
    let severity = match severity_str.to_lowercase().as_str() {
        "p1" | "critical" => Severity::Critical,
        "p2" | "high" => Severity::High,
        "p3" | "medium" => Severity::Medium,
        _ => Severity::Low,
    };

    let id = event["event"]["data"]["id"]
        .as_str()
        .or(event["messages"][0]["incident"]["id"].as_str())
        .unwrap_or("")
        .to_string();

    let description = event["event"]["data"]["description"]
        .as_str()
        .or(event["messages"][0]["incident"]["description"].as_str())
        .unwrap_or("")
        .to_string();

    alerts.push(Alert {
        id,
        source: AlertSource::PagerDuty,
        severity,
        title,
        description,
        labels: HashMap::new(),
        started_at: Utc::now(),
        status: AlertStatus::Firing,
    });

    alerts
}

/// Format an alert as a human-readable string for the agent.
pub fn format_alert(alert: &Alert) -> String {
    let mut lines = vec![
        format!("{} {}: {}", alert_emoji(&alert.severity), alert.severity, alert.title),
    ];

    if !alert.description.is_empty() {
        lines.push(format!("Description: {}", alert.description));
    }

    if !alert.labels.is_empty() {
        let label_str: Vec<String> = alert
            .labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        lines.push(format!("Labels: {}", label_str.join(", ")));
    }

    lines.push(format!("Since: {}", alert.started_at.format("%H:%M:%S UTC")));
    lines.push(format!("Source: {:?}", alert.source));

    lines.join("\n")
}

fn alert_emoji(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "🚨",
        Severity::High => "🔴",
        Severity::Medium => "🟡",
        Severity::Low => "🟢",
        Severity::Info => "ℹ️",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_alertmanager() {
        let payload = serde_json::json!({
            "alerts": [{
                "status": "firing",
                "labels": {
                    "alertname": "PodMemoryHigh",
                    "namespace": "production",
                    "pod": "api-server-abc123",
                    "severity": "critical"
                },
                "annotations": {
                    "summary": "Pod memory usage above 90%"
                },
                "startsAt": "2026-03-19T10:15:00Z",
                "fingerprint": "abc123"
            }]
        });

        let alerts = parse_alertmanager(&payload);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "PodMemoryHigh");
        assert!(matches!(alerts[0].severity, Severity::Critical));
        assert_eq!(alerts[0].labels["namespace"], "production");
        assert_eq!(alerts[0].description, "Pod memory usage above 90%");
    }

    #[test]
    fn test_parse_alertmanager_resolved() {
        let payload = serde_json::json!({
            "alerts": [{
                "status": "resolved",
                "labels": { "alertname": "TestAlert" },
                "annotations": {},
                "startsAt": "2026-03-19T10:00:00Z"
            }]
        });

        let alerts = parse_alertmanager(&payload);
        assert!(matches!(alerts[0].status, AlertStatus::Resolved));
    }

    #[test]
    fn test_format_alert() {
        let alert = Alert {
            id: "test".to_string(),
            source: AlertSource::Alertmanager,
            severity: Severity::Critical,
            title: "PodMemoryHigh".to_string(),
            description: "Memory above 90%".to_string(),
            labels: HashMap::from([
                ("namespace".to_string(), "production".to_string()),
            ]),
            started_at: Utc::now(),
            status: AlertStatus::Firing,
        };

        let formatted = format_alert(&alert);
        assert!(formatted.contains("CRITICAL"));
        assert!(formatted.contains("PodMemoryHigh"));
        assert!(formatted.contains("namespace=production"));
    }
}
