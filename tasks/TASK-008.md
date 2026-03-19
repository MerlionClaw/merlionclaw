# TASK-008: Permission Engine

## Objective
Implement a capability-based permission system that gates every skill tool invocation. Skills declare required permissions, the engine checks against user-configured policies, and destructive operations can require explicit user approval.

## Dependencies
- TASK-004 must be complete (skill engine with SkillHandler trait)
- TASK-006 must be complete (working MVP to integrate into)

## Steps

### 1. Define capabilities (mclaw-permissions/src/capability.rs)

```rust
/// A capability represents a specific permission a skill may need.
/// Format: "{domain}:{action}" — e.g., "k8s:read", "exec:helm", "net:grafana"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capability(String);

impl Capability {
    pub fn new(s: &str) -> Result<Self> {
        // Validate format: must be "{domain}:{action}"
        // domain: alphanumeric + underscore
        // action: read | write | exec | admin | * (wildcard)
    }

    pub fn domain(&self) -> &str;
    pub fn action(&self) -> &str;

    /// Check if this capability matches another (supports wildcard)
    /// "k8s:*" matches "k8s:read", "k8s:write", etc.
    pub fn matches(&self, other: &Capability) -> bool;
}

/// Standard capability domains
pub mod domains {
    pub const K8S: &str = "k8s";
    pub const HELM: &str = "helm";
    pub const ISTIO: &str = "istio";
    pub const TERRAFORM: &str = "terraform";
    pub const NET: &str = "net";       // outbound HTTP calls
    pub const FS: &str = "fs";         // filesystem access
    pub const EXEC: &str = "exec";     // shell command execution
    pub const MEMORY: &str = "memory"; // memory read/write
}

/// Standard actions
pub mod actions {
    pub const READ: &str = "read";
    pub const WRITE: &str = "write";
    pub const EXEC: &str = "exec";
    pub const ADMIN: &str = "admin";
    pub const WILDCARD: &str = "*";
}
```

### 2. Policy configuration (mclaw-permissions/src/policy.rs)

```rust
#[derive(Debug, Deserialize)]
pub struct PermissionConfig {
    /// Default policy: "deny" (recommended) or "allow"
    pub default: DefaultPolicy,
    /// Per-skill permission grants
    pub skills: HashMap<String, SkillPolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultPolicy {
    Deny,
    Allow,
}

#[derive(Debug, Deserialize)]
pub struct SkillPolicy {
    /// Capabilities granted to this skill
    pub allow: Vec<String>,
    /// Capabilities that require user approval before each use
    #[serde(default)]
    pub require_approval: Vec<String>,
    /// Capabilities explicitly denied (overrides allow)
    #[serde(default)]
    pub deny: Vec<String>,
}
```

Example config:
```toml
[permissions]
default = "deny"

[permissions.skills.k8s]
allow = ["k8s:read", "k8s:write"]
require_approval = ["k8s:write"]  # ask before creating/deleting resources

[permissions.skills.helm]
allow = ["k8s:read", "exec:helm"]
require_approval = ["exec:helm"]

[permissions.skills.loki]
allow = ["net:grafana"]

[permissions.skills.memory]
allow = ["memory:*", "fs:read"]
```

### 3. Permission engine (mclaw-permissions/src/lib.rs)

```rust
pub struct PermissionEngine {
    config: PermissionConfig,
}

#[derive(Debug)]
pub enum PermissionDecision {
    /// Allowed — proceed with execution
    Allowed,
    /// Requires user approval — ask before proceeding
    RequiresApproval { capability: Capability, reason: String },
    /// Denied — do not execute
    Denied { capability: Capability, reason: String },
}

impl PermissionEngine {
    pub fn new(config: PermissionConfig) -> Self;

    /// Check if a skill's tool invocation is permitted
    pub fn check(
        &self,
        skill_name: &str,
        required_capabilities: &[Capability],
    ) -> PermissionDecision {
        // 1. Check deny list first (deny always wins)
        // 2. Check require_approval list
        // 3. Check allow list
        // 4. Fall back to default policy
    }

    /// Check a single capability for a skill
    pub fn check_one(
        &self,
        skill_name: &str,
        capability: &Capability,
    ) -> PermissionDecision;
}
```

### 4. Integrate into skill dispatch

Update `mclaw-skills/src/registry.rs` dispatch flow:

```rust
pub async fn dispatch(
    &self,
    tool_name: &str,
    input: serde_json::Value,
    permissions: &PermissionEngine,
    approval_fn: impl AsyncFn() -> bool,  // ask user for approval
) -> Result<ToolResult> {
    let skill = self.find_skill_for_tool(tool_name)?;
    let required_caps = skill.parsed.manifest.permissions
        .iter()
        .map(|s| Capability::new(s))
        .collect::<Result<Vec<_>>>()?;

    match permissions.check(&skill.parsed.manifest.name, &required_caps) {
        PermissionDecision::Allowed => {
            skill.handler.execute(tool_name, input).await
        }
        PermissionDecision::RequiresApproval { capability, reason } => {
            // Ask user: "Helm upgrade requires exec:helm permission. Approve? [y/n]"
            if approval_fn().await {
                skill.handler.execute(tool_name, input).await
            } else {
                Ok(format!("Operation cancelled by user."))
            }
        }
        PermissionDecision::Denied { capability, reason } => {
            Err(anyhow!("Permission denied: {} requires '{}' capability, which is not granted. Update permissions.skills.{} in config.", tool_name, capability, skill.parsed.manifest.name))
        }
    }
}
```

### 5. User approval flow

When `require_approval` is triggered:

1. Agent sends approval request to user via channel:
   ```
   ⚠️ Approval required: `helm upgrade nginx` needs `exec:helm` permission.
   Reply "yes" to proceed or "no" to cancel.
   ```
2. Wait for user response (with timeout, default 60s)
3. If "yes" / "y" → execute
4. If "no" / "n" / timeout → cancel and inform user

### 6. Permission audit logging

Log every permission check at DEBUG level:
```
DEBUG mclaw_permissions: check skill=k8s capability=k8s:read decision=allowed
WARN mclaw_permissions: check skill=terraform capability=exec:terraform decision=denied reason="not in allow list"
INFO mclaw_permissions: approval_requested skill=helm capability=exec:helm tool=helm_upgrade
INFO mclaw_permissions: approval_granted skill=helm capability=exec:helm user=larry
```

### 7. Special command

- `/permissions` → show current permission config for all skills
- `/permissions <skill>` → show permissions for a specific skill

## Validation

```bash
cargo test -p mclaw-permissions

# Unit tests:
# - DefaultPolicy::Deny blocks unlisted capabilities
# - DefaultPolicy::Allow permits unlisted capabilities
# - Deny list overrides allow list
# - Wildcard matching works (k8s:* matches k8s:read)
# - RequiresApproval is returned for approval-listed capabilities

# Integration test via Telegram:
You: "delete the nginx deployment in staging"
Bot: "⚠️ Approval required: deleting deployment requires `k8s:write` permission. Reply 'yes' to proceed."
You: "yes"
Bot: "Deployment nginx deleted in staging namespace."

# Denied test (remove helm from allow list):
You: "upgrade the nginx helm release"
Bot: "Permission denied: helm_upgrade requires 'exec:helm' capability. Update permissions.skills.helm in config."
```

## Output

A working permission engine that enforces capability-based access control on all skill invocations, with support for user approval flows and audit logging.
