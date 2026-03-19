//! Policy configuration and evaluation.

use std::collections::HashMap;

use serde::Deserialize;
use tracing::debug;

use crate::capability::Capability;

/// Permission configuration loaded from TOML.
#[derive(Debug, Deserialize)]
pub struct PermissionConfig {
    /// Default policy when no specific grant is found.
    #[serde(default)]
    pub default: DefaultPolicy,
    /// Per-skill permission grants.
    #[serde(default)]
    pub skills: HashMap<String, SkillPolicy>,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default: DefaultPolicy::Deny,
            skills: HashMap::new(),
        }
    }
}

/// The default policy for unlisted capabilities.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultPolicy {
    /// Deny unless explicitly allowed (recommended).
    #[default]
    Deny,
    /// Allow unless explicitly denied.
    Allow,
}

/// Per-skill permission policy.
#[derive(Debug, Deserialize)]
pub struct SkillPolicy {
    /// Capabilities granted to this skill.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Capabilities that require user approval before each use.
    #[serde(default)]
    pub require_approval: Vec<String>,
    /// Capabilities explicitly denied (overrides allow).
    #[serde(default)]
    pub deny: Vec<String>,
}

/// The result of a permission check.
#[derive(Debug)]
pub enum PermissionDecision {
    /// Allowed — proceed with execution.
    Allowed,
    /// Requires user approval before proceeding.
    RequiresApproval {
        capability: String,
        reason: String,
    },
    /// Denied — do not execute.
    Denied {
        capability: String,
        reason: String,
    },
}

/// The permission engine evaluates policies against requested capabilities.
pub struct PermissionEngine {
    config: PermissionConfig,
}

impl PermissionEngine {
    /// Create a new permission engine with the given config.
    pub fn new(config: PermissionConfig) -> Self {
        Self { config }
    }

    /// Create a permissive engine that allows everything (for testing/development).
    pub fn allow_all() -> Self {
        Self {
            config: PermissionConfig {
                default: DefaultPolicy::Allow,
                skills: HashMap::new(),
            },
        }
    }

    /// Check if a skill is permitted to use a set of capabilities.
    /// Returns the first non-Allowed decision encountered.
    pub fn check(
        &self,
        skill_name: &str,
        required_capabilities: &[Capability],
    ) -> PermissionDecision {
        for cap in required_capabilities {
            let decision = self.check_one(skill_name, cap);
            match decision {
                PermissionDecision::Allowed => continue,
                _ => return decision,
            }
        }
        PermissionDecision::Allowed
    }

    /// Check a single capability for a skill.
    pub fn check_one(
        &self,
        skill_name: &str,
        capability: &Capability,
    ) -> PermissionDecision {
        let skill_policy = self.config.skills.get(skill_name);

        // 1. Check deny list first (deny always wins)
        if let Some(policy) = skill_policy {
            for denied in &policy.deny {
                if let Ok(denied_cap) = Capability::new(denied) {
                    if denied_cap.matches(capability) {
                        let decision = PermissionDecision::Denied {
                            capability: capability.to_string(),
                            reason: format!("explicitly denied for skill '{skill_name}'"),
                        };
                        debug!(
                            skill = skill_name,
                            capability = %capability,
                            "permission denied (deny list)"
                        );
                        return decision;
                    }
                }
            }
        }

        // 2. Check require_approval list
        if let Some(policy) = skill_policy {
            for approval in &policy.require_approval {
                if let Ok(approval_cap) = Capability::new(approval) {
                    if approval_cap.matches(capability) {
                        let decision = PermissionDecision::RequiresApproval {
                            capability: capability.to_string(),
                            reason: format!(
                                "requires approval for skill '{skill_name}'"
                            ),
                        };
                        debug!(
                            skill = skill_name,
                            capability = %capability,
                            "permission requires approval"
                        );
                        return decision;
                    }
                }
            }
        }

        // 3. Check allow list
        if let Some(policy) = skill_policy {
            for allowed in &policy.allow {
                if let Ok(allowed_cap) = Capability::new(allowed) {
                    if allowed_cap.matches(capability) {
                        debug!(
                            skill = skill_name,
                            capability = %capability,
                            "permission allowed"
                        );
                        return PermissionDecision::Allowed;
                    }
                }
            }
        }

        // 4. Fall back to default policy
        match self.config.default {
            DefaultPolicy::Allow => {
                debug!(
                    skill = skill_name,
                    capability = %capability,
                    "permission allowed (default policy)"
                );
                PermissionDecision::Allowed
            }
            DefaultPolicy::Deny => {
                debug!(
                    skill = skill_name,
                    capability = %capability,
                    "permission denied (default policy)"
                );
                PermissionDecision::Denied {
                    capability: capability.to_string(),
                    reason: format!(
                        "not in allow list for skill '{skill_name}' and default policy is deny"
                    ),
                }
            }
        }
    }

    /// Get a human-readable summary of permissions for a skill.
    pub fn skill_summary(&self, skill_name: &str) -> String {
        match self.config.skills.get(skill_name) {
            Some(policy) => {
                let mut lines = vec![format!("Permissions for '{skill_name}':")];
                if !policy.allow.is_empty() {
                    lines.push(format!("  Allow: {}", policy.allow.join(", ")));
                }
                if !policy.require_approval.is_empty() {
                    lines.push(format!(
                        "  Require approval: {}",
                        policy.require_approval.join(", ")
                    ));
                }
                if !policy.deny.is_empty() {
                    lines.push(format!("  Deny: {}", policy.deny.join(", ")));
                }
                lines.join("\n")
            }
            None => {
                let default = match self.config.default {
                    DefaultPolicy::Allow => "allow",
                    DefaultPolicy::Deny => "deny",
                };
                format!("No specific policy for '{skill_name}' (default: {default})")
            }
        }
    }

    /// Get a summary of all configured permissions.
    pub fn summary(&self) -> String {
        let default = match self.config.default {
            DefaultPolicy::Allow => "allow",
            DefaultPolicy::Deny => "deny",
        };
        let mut lines = vec![format!("Default policy: {default}")];

        for (skill_name, policy) in &self.config.skills {
            let allow = if policy.allow.is_empty() {
                "none".to_string()
            } else {
                policy.allow.join(", ")
            };
            lines.push(format!("  {skill_name}: allow=[{allow}]"));
        }

        if self.config.skills.is_empty() {
            lines.push("  No per-skill policies configured.".to_string());
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> PermissionConfig {
        let mut skills = HashMap::new();
        skills.insert(
            "k8s".to_string(),
            SkillPolicy {
                allow: vec!["k8s:read".to_string(), "k8s:write".to_string()],
                require_approval: vec!["k8s:write".to_string()],
                deny: vec![],
            },
        );
        skills.insert(
            "helm".to_string(),
            SkillPolicy {
                allow: vec!["k8s:read".to_string(), "exec:helm".to_string()],
                require_approval: vec!["exec:helm".to_string()],
                deny: vec![],
            },
        );
        skills.insert(
            "memory".to_string(),
            SkillPolicy {
                allow: vec!["memory:*".to_string()],
                require_approval: vec![],
                deny: vec![],
            },
        );
        PermissionConfig {
            default: DefaultPolicy::Deny,
            skills,
        }
    }

    #[test]
    fn test_allowed() {
        let engine = PermissionEngine::new(test_config());
        let cap = Capability::new("k8s:read").unwrap();
        assert!(matches!(
            engine.check_one("k8s", &cap),
            PermissionDecision::Allowed
        ));
    }

    #[test]
    fn test_requires_approval() {
        let engine = PermissionEngine::new(test_config());
        let cap = Capability::new("k8s:write").unwrap();
        assert!(matches!(
            engine.check_one("k8s", &cap),
            PermissionDecision::RequiresApproval { .. }
        ));
    }

    #[test]
    fn test_denied_default() {
        let engine = PermissionEngine::new(test_config());
        let cap = Capability::new("exec:terraform").unwrap();
        assert!(matches!(
            engine.check_one("k8s", &cap),
            PermissionDecision::Denied { .. }
        ));
    }

    #[test]
    fn test_denied_explicit() {
        let mut config = test_config();
        config
            .skills
            .get_mut("k8s")
            .unwrap()
            .deny
            .push("k8s:admin".to_string());
        let engine = PermissionEngine::new(config);
        let cap = Capability::new("k8s:admin").unwrap();
        assert!(matches!(
            engine.check_one("k8s", &cap),
            PermissionDecision::Denied { .. }
        ));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let mut config = test_config();
        let policy = config.skills.get_mut("k8s").unwrap();
        policy.allow.push("k8s:admin".to_string());
        policy.deny.push("k8s:admin".to_string());
        let engine = PermissionEngine::new(config);
        let cap = Capability::new("k8s:admin").unwrap();
        // Deny wins over allow
        assert!(matches!(
            engine.check_one("k8s", &cap),
            PermissionDecision::Denied { .. }
        ));
    }

    #[test]
    fn test_wildcard_allow() {
        let engine = PermissionEngine::new(test_config());
        let cap = Capability::new("memory:read").unwrap();
        assert!(matches!(
            engine.check_one("memory", &cap),
            PermissionDecision::Allowed
        ));
        let cap2 = Capability::new("memory:write").unwrap();
        assert!(matches!(
            engine.check_one("memory", &cap2),
            PermissionDecision::Allowed
        ));
    }

    #[test]
    fn test_default_allow_policy() {
        let config = PermissionConfig {
            default: DefaultPolicy::Allow,
            skills: HashMap::new(),
        };
        let engine = PermissionEngine::new(config);
        let cap = Capability::new("anything:read").unwrap();
        assert!(matches!(
            engine.check_one("unknown_skill", &cap),
            PermissionDecision::Allowed
        ));
    }

    #[test]
    fn test_check_multiple() {
        let engine = PermissionEngine::new(test_config());
        let caps = vec![
            Capability::new("k8s:read").unwrap(),
            Capability::new("k8s:write").unwrap(),
        ];
        // Should stop at require_approval for k8s:write
        assert!(matches!(
            engine.check("k8s", &caps),
            PermissionDecision::RequiresApproval { .. }
        ));
    }

    #[test]
    fn test_unknown_skill_denied() {
        let engine = PermissionEngine::new(test_config());
        let cap = Capability::new("k8s:read").unwrap();
        assert!(matches!(
            engine.check_one("nonexistent", &cap),
            PermissionDecision::Denied { .. }
        ));
    }

    #[test]
    fn test_allow_all() {
        let engine = PermissionEngine::allow_all();
        let cap = Capability::new("anything:admin").unwrap();
        assert!(matches!(
            engine.check_one("any_skill", &cap),
            PermissionDecision::Allowed
        ));
    }
}
