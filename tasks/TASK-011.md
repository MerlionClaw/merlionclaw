# TASK-011: Incident Response Skill

## Objective
Implement an incident response skill that receives alerts from PagerDuty/OpsGenie/Alertmanager webhooks, auto-triages using LLM analysis, executes diagnostic runbooks, and coordinates response via Slack/Telegram.

## Dependencies
- TASK-006 must be complete (working MVP)
- TASK-009 recommended (Loki skill for log-based diagnosis)
- TASK-008 recommended (permissions for auto-remediation)

## Steps

### 1. Webhook receiver (mclaw-gateway extension)

Add a webhook HTTP endpoint to the gateway:

```rust
// In mclaw-gateway/src/server.rs, add route:
.route("/webhook/alert", post(alert_webhook_handler))
.route("/webhook/pagerduty", post(pagerduty_webhook_handler))
.route("/webhook/alertmanager", post(alertmanager_webhook_handler))
```

Each webhook handler:
1. Parse the incoming alert payload (PagerDuty, OpsGenie, Alertmanager each have different formats)
2. Normalize into a common `Alert` struct
3. Route to the agent as a special `InboundMessage::Alert`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub source: AlertSource,  // PagerDuty, OpsGenie, Alertmanager, Custom
    pub severity: Severity,   // Critical, High, Medium, Low, Info
    pub title: String,
    pub description: String,
    pub labels: HashMap<String, String>,  // e.g., namespace, pod, service
    pub started_at: DateTime<Utc>,
    pub status: AlertStatus,  // Firing, Resolved
    pub raw: serde_json::Value,  // original payload for reference
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}
```

### 2. Incident skill tools

| Tool | Description | Permission |
|------|-------------|------------|
| `incident_list` | List active incidents | net:alerting |
| `incident_get` | Get incident details with timeline | net:alerting |
| `incident_acknowledge` | Acknowledge an incident in PagerDuty/OpsGenie | net:alerting |
| `incident_resolve` | Resolve an incident | net:alerting |
| `incident_diagnose` | Run diagnostic checks for an alert (auto-triage) | k8s:read, net:grafana |
| `incident_runbook` | Execute a predefined runbook for a known alert type | exec:runbook |
| `incident_notify` | Send notification to a Slack channel or Telegram group | net:slack, net:telegram |
| `incident_create_warroom` | Create a Slack channel for incident coordination | net:slack |
| `incident_timeline` | Show incident timeline with all actions taken | net:alerting |

### 3. Auto-triage (mclaw-skills/src/incident.rs)

When an alert comes in, the agent auto-triages:

```rust
pub async fn diagnose(&self, alert: &Alert) -> Result<DiagnosisReport> {
    let mut checks = Vec::new();

    // 1. Extract K8s context from alert labels
    if let Some(namespace) = alert.labels.get("namespace") {
        if let Some(pod) = alert.labels.get("pod") {
            // Check pod status
            checks.push(self.k8s.describe_pod(pod, namespace).await?);
            // Get recent logs
            checks.push(self.k8s.get_logs(pod, namespace, 30).await?);
            // Check events
            checks.push(self.k8s.get_events(namespace).await?);
        }
        if let Some(deployment) = alert.labels.get("deployment") {
            // Check deployment status
            checks.push(self.k8s.describe_deployment(deployment, namespace).await?);
        }
    }

    // 2. Query Loki for related errors
    if let Some(app) = alert.labels.get("app") {
        let query = format!(r#"{{app="{}"}} |= "error" | logfmt"#, app);
        checks.push(self.loki.query(&query, 20).await?);
    }

    // 3. Check Grafana for related firing alerts
    checks.push(self.grafana.list_alerts("firing").await?);

    Ok(DiagnosisReport { checks })
}
```

### 4. Runbook execution

Runbooks are defined as YAML files:

```yaml
# runbooks/high-memory-pod.yaml
name: high-memory-pod
triggers:
  - alert_name: "PodMemoryHigh"
  - alert_name: "ContainerOOMKilled"
steps:
  - name: Check pod memory usage
    tool: k8s_describe_pod
    params:
      pod: "{{labels.pod}}"
      namespace: "{{labels.namespace}}"
  - name: Get recent logs
    tool: k8s_get_logs
    params:
      pod: "{{labels.pod}}"
      namespace: "{{labels.namespace}}"
      lines: 50
  - name: Check if OOMKilled
    tool: k8s_get_events
    params:
      namespace: "{{labels.namespace}}"
      field_selector: "involvedObject.name={{labels.pod}}"
  - name: Auto-remediate (restart pod)
    tool: k8s_delete_pod
    params:
      pod: "{{labels.pod}}"
      namespace: "{{labels.namespace}}"
    requires_approval: true
    condition: "events contain OOMKilled"
```

The runbook executor:
1. Matches incoming alert to runbook by `triggers`
2. Renders params with `{{labels.xxx}}` template substitution
3. Executes steps in order
4. Collects results
5. Presents summary to user / notifies via channel
6. Steps with `requires_approval` pause and ask user

### 5. Proactive alert routing

When an alert webhook fires:
1. Agent receives the alert
2. Agent runs auto-triage diagnosis
3. Agent sends a summary to the configured notification channel:
   ```
   🚨 CRITICAL: PodMemoryHigh
   
   Pod: api-server-abc123 (production)
   Since: 10:15 UTC (5 min ago)
   
   Diagnosis:
   - Pod memory at 95% (limit: 512Mi)
   - 3 OOMKilled events in last hour
   - Recent logs show memory leak in /api/search handler
   
   Suggested actions:
   1. Restart the pod (temporary fix)
   2. Increase memory limit
   3. Investigate memory leak in search handler
   
   Reply with a number to execute, or ask me anything about this incident.
   ```

### 6. Config additions

```toml
[skills.incident]
enabled = true

[skills.incident.pagerduty]
api_key_env = "PAGERDUTY_API_KEY"

[skills.incident.alertmanager]
url = "http://alertmanager.monitoring:9093"

[skills.incident.notification]
channel = "slack:#incidents"  # or "telegram:{chat_id}"
auto_diagnose = true
auto_acknowledge = false

[skills.incident.runbooks]
dir = "~/.merlionclaw/runbooks"
```

### 7. Webhook security

- Support HMAC signature verification for webhook payloads
- PagerDuty: verify `X-PagerDuty-Signature` header
- Alertmanager: support basic auth or bearer token
- Custom webhooks: configurable shared secret

## Validation

```bash
cargo test -p mclaw-skills -- incident

# Simulate an alert:
curl -X POST http://localhost:18789/webhook/alertmanager \
  -H "Content-Type: application/json" \
  -d '{
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
      "startsAt": "2026-03-19T10:15:00Z"
    }]
  }'

# In Slack/Telegram, should see:
# 🚨 CRITICAL alert with diagnosis summary and suggested actions

# Interactive:
You: "tell me more about this incident"
Bot: [detailed diagnosis with logs, events, and resource usage]

You: "restart the pod"
Bot: "⚠️ Approval required... [Approve] [Deny]"
You: "yes"
Bot: "Pod api-server-abc123 deleted. New pod api-server-def456 is starting..."
```

## Output

A complete incident response skill with webhook intake from major alerting platforms, LLM-powered auto-triage, runbook execution, and multi-channel notification. This is a killer feature that positions MerlionClaw as indispensable for on-call SREs.
