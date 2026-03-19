# TASK-009: Helm + Istio + Loki Skills

## Objective
Implement three additional DevOps skills: Helm release management, Istio traffic management, and Loki/Grafana log querying. These are the vertical differentiation skills that make MerlionClaw uniquely valuable for infrastructure operators.

## Dependencies
- TASK-004 must be complete (skill engine + SkillHandler trait)
- TASK-006 must be complete (working MVP)
- TASK-008 recommended (permissions for destructive operations)

## Steps

---

### Part A: Helm Skill (mclaw-skills/src/helm.rs)

Helm operations via CLI wrapper (helm binary must be installed on the host).

#### Tools

| Tool | Description | Permission |
|------|-------------|------------|
| `helm_list` | List releases in a namespace or all namespaces | k8s:read |
| `helm_status` | Get status of a specific release | k8s:read |
| `helm_history` | Show release revision history | k8s:read |
| `helm_values` | Get current values for a release | k8s:read |
| `helm_upgrade` | Upgrade a release (with values) | exec:helm |
| `helm_install` | Install a new chart | exec:helm |
| `helm_rollback` | Rollback to a previous revision | exec:helm |
| `helm_uninstall` | Uninstall a release | exec:helm |
| `helm_diff` | Show diff between current and proposed values (requires helm-diff plugin) | k8s:read |

#### Implementation

```rust
pub struct HelmSkill {
    helm_binary: PathBuf,  // default: "helm" (from PATH)
    kubeconfig: Option<PathBuf>,
}

impl HelmSkill {
    pub async fn new() -> Result<Self> {
        // Verify helm binary exists: `helm version --short`
        // Log helm version
    }

    async fn run_helm(&self, args: &[&str]) -> Result<String> {
        // tokio::process::Command to run helm with args
        // Capture stdout + stderr
        // Return stdout on success, error with stderr on failure
        // Add --kubeconfig if configured
        // Add --output json where supported for structured parsing
    }
}
```

For `helm_upgrade` and `helm_install`:
- Accept `values` as a JSON object → write to temp YAML file → pass as `--values`
- Accept `set` as key=value pairs → pass as `--set`
- Always add `--wait --timeout 5m` unless overridden

#### SKILL.md (skills/helm/SKILL.md)

```yaml
---
name: helm
description: Manage Helm releases - list, install, upgrade, rollback, check status and history
version: 0.1.0
permissions:
  - k8s:read
  - exec:helm
tools:
  - name: helm_list
    description: List Helm releases
    parameters:
      namespace:
        type: string
        description: Namespace (omit for all namespaces)
        required: false
      filter:
        type: string
        description: Filter by release name substring
        required: false
  - name: helm_status
    description: Get detailed status of a Helm release
    parameters:
      release:
        type: string
        required: true
      namespace:
        type: string
        default: default
  - name: helm_history
    description: Show revision history of a Helm release
    parameters:
      release:
        type: string
        required: true
      namespace:
        type: string
        default: default
  - name: helm_values
    description: Get current values for a Helm release
    parameters:
      release:
        type: string
        required: true
      namespace:
        type: string
        default: default
  - name: helm_upgrade
    description: Upgrade a Helm release with new values
    parameters:
      release:
        type: string
        required: true
      chart:
        type: string
        description: Chart reference (e.g., bitnami/nginx, ./my-chart)
        required: true
      namespace:
        type: string
        default: default
      values:
        type: object
        description: Values to set (will be converted to YAML)
        required: false
      set:
        type: array
        description: Individual key=value overrides
        required: false
  - name: helm_rollback
    description: Rollback a Helm release to a previous revision
    parameters:
      release:
        type: string
        required: true
      revision:
        type: integer
        description: Target revision number (omit for previous)
        required: false
      namespace:
        type: string
        default: default
  - name: helm_uninstall
    description: Uninstall a Helm release
    parameters:
      release:
        type: string
        required: true
      namespace:
        type: string
        default: default
---

# Helm Skill

You help manage Helm releases on Kubernetes clusters. Guidelines:
- Always show the current revision and app version when reporting status
- Before upgrade/install, confirm the chart version and key value changes
- For rollbacks, show the history first so the user can pick the right revision
- Warn if a release is in a failed state before attempting operations
- Use `helm_diff` when available to preview changes before upgrade
```

---

### Part B: Istio Skill (mclaw-skills/src/istio.rs)

Istio resource management via kube-rs with custom CRD types.

#### Tools

| Tool | Description | Permission |
|------|-------------|------------|
| `istio_list_virtualservices` | List VirtualServices | k8s:read |
| `istio_get_virtualservice` | Get VirtualService details with routing rules | k8s:read |
| `istio_list_destinationrules` | List DestinationRules | k8s:read |
| `istio_list_gateways` | List Istio Gateways | k8s:read |
| `istio_traffic_shift` | Update VirtualService weights for canary/blue-green | k8s:write, istio:write |
| `istio_fault_inject` | Add fault injection to a VirtualService | k8s:write, istio:write |
| `istio_mtls_status` | Check mTLS status across namespaces | k8s:read |
| `istio_proxy_status` | Check Envoy sidecar sync status (istioctl equivalent) | k8s:read |

#### Implementation

Define Istio CRD types for kube-rs:

```rust
use kube::CustomResource;

#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "networking.istio.io",
    version = "v1",
    kind = "VirtualService",
    namespaced
)]
pub struct VirtualServiceSpec {
    pub hosts: Vec<String>,
    pub gateways: Option<Vec<String>>,
    pub http: Option<Vec<HttpRoute>>,
    // ... define nested types as needed
}

// Similar for DestinationRule, Gateway, PeerAuthentication
```

For `istio_traffic_shift`:
- Read current VirtualService
- Update route weights (e.g., 90/10 canary split)
- Apply via kube-rs `Api::replace`
- Return before/after weight summary

For `istio_mtls_status`:
- List PeerAuthentication resources
- Check if `STRICT` mTLS is enabled per-namespace
- Report status table

#### SKILL.md (skills/istio/SKILL.md)

Write full SKILL.md with all tool definitions and Istio-specific system prompt guidance.

---

### Part C: Loki/Grafana Skill (mclaw-skills/src/loki.rs)

Log querying via Loki HTTP API and Grafana dashboard integration.

#### Tools

| Tool | Description | Permission |
|------|-------------|------------|
| `loki_query` | Execute a LogQL query and return results | net:grafana |
| `loki_query_range` | Query logs within a time range | net:grafana |
| `loki_labels` | List available label names | net:grafana |
| `loki_label_values` | List values for a specific label | net:grafana |
| `loki_tail` | Get the most recent N log lines for a stream | net:grafana |
| `grafana_dashboard_link` | Generate a link to a Grafana dashboard with time range | net:grafana |
| `grafana_list_alerts` | List firing alerts from Grafana | net:grafana |

#### Implementation

```rust
pub struct LokiSkill {
    loki_url: String,       // e.g., http://loki.monitoring:3100
    grafana_url: String,    // e.g., https://grafana.example.com
    grafana_token: Option<String>,  // Grafana API token for authenticated access
    client: reqwest::Client,
}
```

For `loki_query`:
- POST to `/loki/api/v1/query` with `query` parameter
- Parse response, format log lines as readable text
- Limit output to last N lines (default 100) to avoid overwhelming LLM context

For `loki_query_range`:
- POST to `/loki/api/v1/query_range` with `query`, `start`, `end`
- Accept human-readable time inputs: "last 1h", "last 30m", "2026-03-19 10:00 to 11:00"
- Parse these into Unix timestamps

For `grafana_dashboard_link`:
- Construct URL: `{grafana_url}/d/{dashboard_uid}?orgId=1&from={start}&to={end}&var-namespace={ns}`
- Return clickable link

For `grafana_list_alerts`:
- GET `/api/v1/provisioning/alert-rules` or `/api/alertmanager/grafana/api/v2/alerts`
- Filter by state: firing, pending
- Format as table: alert name, state, labels, since

#### SKILL.md (skills/loki/SKILL.md)

```yaml
---
name: loki
description: Query logs from Loki and interact with Grafana dashboards and alerts
version: 0.1.0
permissions:
  - net:grafana
tools:
  - name: loki_query
    description: Execute a LogQL query against Loki
    parameters:
      query:
        type: string
        description: "LogQL query, e.g., {namespace=\"production\",app=\"nginx\"} |= \"error\""
        required: true
      limit:
        type: integer
        description: Max number of log lines to return
        default: 50
  - name: loki_query_range
    description: Query logs within a specific time range
    parameters:
      query:
        type: string
        required: true
      start:
        type: string
        description: "Start time (e.g., '1h ago', '2026-03-19T10:00:00Z')"
        required: true
      end:
        type: string
        description: "End time (default: now)"
        required: false
      limit:
        type: integer
        default: 100
  - name: loki_labels
    description: List available log stream label names
  - name: loki_label_values
    description: List values for a specific label
    parameters:
      label:
        type: string
        required: true
  - name: loki_tail
    description: Get the most recent log lines for a stream selector
    parameters:
      query:
        type: string
        description: "Stream selector, e.g., {app=\"nginx\"}"
        required: true
      lines:
        type: integer
        default: 20
  - name: grafana_dashboard_link
    description: Generate a Grafana dashboard URL with time range
    parameters:
      dashboard_uid:
        type: string
        required: true
      time_range:
        type: string
        description: "Time range, e.g., 'last 1h', 'last 24h'"
        default: last 1h
      variables:
        type: object
        description: "Dashboard variables, e.g., {\"namespace\": \"production\"}"
        required: false
  - name: grafana_list_alerts
    description: List currently firing or pending alerts from Grafana
    parameters:
      state:
        type: string
        description: Filter by state (firing, pending, all)
        default: firing
---

# Loki / Grafana Skill

You help query logs and monitor alerts. Guidelines:
- When showing logs, format them with timestamps aligned
- Highlight error-level lines or patterns like "panic", "fatal", "OOMKilled"
- For broad queries, suggest narrowing with label selectors or pattern filters
- When errors are found, proactively suggest related K8s commands to investigate (e.g., pod describe, events)
- Always mention the time range of returned logs
- For Grafana links, format as clickable URLs
```

---

### 3. Config additions

```toml
[skills.helm]
enabled = true
helm_binary = "helm"  # default: helm from PATH

[skills.istio]
enabled = true

[skills.loki]
enabled = true
loki_url = "http://loki.monitoring.svc.cluster.local:3100"
grafana_url = "https://grafana.example.com"
grafana_token_env = "GRAFANA_API_TOKEN"
```

### 4. Register all skills

Update the startup flow in `mclaw` to:
1. Read which skills are enabled in config
2. Initialize each skill handler
3. Register with SkillRegistry

## Validation

```bash
cargo test -p mclaw-skills

# Helm test (requires helm binary + K8s cluster with releases):
You: "list all helm releases"
Bot: [table of releases with name, namespace, revision, status, chart, app version]

You: "show the history of the nginx release"
Bot: [revision history with dates]

You: "rollback nginx to revision 3"
Bot: "⚠️ Approval required... Reply yes to proceed."
You: "yes"
Bot: "Rolled back nginx to revision 3. Current status: deployed."

# Istio test (requires Istio installed):
You: "list all virtualservices in production"
Bot: [table of virtualservices with hosts and gateways]

You: "shift 10% traffic to canary for the reviews service"
Bot: "Updated VirtualService reviews: v1=90%, canary=10%"

# Loki test (requires Loki accessible):
You: "show me error logs from nginx in the last hour"
Bot: [formatted log lines with timestamps]

You: "are there any firing alerts?"
Bot: [list of firing alerts with details]
```

## Output

Three production-ready DevOps skills that cover Helm release management, Istio traffic management, and Loki/Grafana observability. These skills establish MerlionClaw's unique value as an infrastructure agent.
