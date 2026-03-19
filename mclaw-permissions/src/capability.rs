//! Capability definitions — `"{domain}:{action}"` format.

use serde::{Deserialize, Serialize};

/// A capability represents a specific permission a skill may need.
/// Format: `"{domain}:{action}"` — e.g., `"k8s:read"`, `"exec:helm"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capability(String);

impl Capability {
    /// Parse and validate a capability string.
    pub fn new(s: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("invalid capability format: '{s}' (expected 'domain:action')");
        }
        let domain = parts[0];
        let action = parts[1];

        if domain.is_empty()
            || !domain
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '*')
        {
            anyhow::bail!("invalid capability domain: '{domain}'");
        }
        if action.is_empty() {
            anyhow::bail!("invalid capability action: empty");
        }

        Ok(Self(s.to_string()))
    }

    /// Get the domain part (e.g., "k8s" from "k8s:read").
    pub fn domain(&self) -> &str {
        self.0.split(':').next().unwrap_or("")
    }

    /// Get the action part (e.g., "read" from "k8s:read").
    pub fn action(&self) -> &str {
        self.0.split(':').nth(1).unwrap_or("")
    }

    /// Check if this capability matches another (supports wildcards).
    /// `"k8s:*"` matches `"k8s:read"`, `"k8s:write"`, etc.
    /// `"*:*"` matches everything.
    pub fn matches(&self, other: &Capability) -> bool {
        let (self_domain, self_action) = (self.domain(), self.action());
        let (other_domain, other_action) = (other.domain(), other.action());

        let domain_match = self_domain == "*" || other_domain == "*" || self_domain == other_domain;
        let action_match = self_action == "*" || other_action == "*" || self_action == other_action;

        domain_match && action_match
    }

    /// Get the full capability string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Standard capability domains.
pub mod domains {
    pub const K8S: &str = "k8s";
    pub const HELM: &str = "helm";
    pub const ISTIO: &str = "istio";
    pub const TERRAFORM: &str = "terraform";
    pub const NET: &str = "net";
    pub const FS: &str = "fs";
    pub const EXEC: &str = "exec";
    pub const MEMORY: &str = "memory";
}

/// Standard actions.
pub mod actions {
    pub const READ: &str = "read";
    pub const WRITE: &str = "write";
    pub const EXEC: &str = "exec";
    pub const ADMIN: &str = "admin";
    pub const WILDCARD: &str = "*";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let cap = Capability::new("k8s:read").unwrap();
        assert_eq!(cap.domain(), "k8s");
        assert_eq!(cap.action(), "read");
        assert_eq!(cap.as_str(), "k8s:read");
    }

    #[test]
    fn test_parse_invalid() {
        assert!(Capability::new("invalid").is_err());
        assert!(Capability::new(":read").is_err());
        assert!(Capability::new("k8s:").is_err());
    }

    #[test]
    fn test_exact_match() {
        let a = Capability::new("k8s:read").unwrap();
        let b = Capability::new("k8s:read").unwrap();
        assert!(a.matches(&b));
    }

    #[test]
    fn test_no_match() {
        let a = Capability::new("k8s:read").unwrap();
        let b = Capability::new("k8s:write").unwrap();
        assert!(!a.matches(&b));
    }

    #[test]
    fn test_wildcard_action() {
        let wildcard = Capability::new("k8s:*").unwrap();
        let read = Capability::new("k8s:read").unwrap();
        let write = Capability::new("k8s:write").unwrap();
        assert!(wildcard.matches(&read));
        assert!(wildcard.matches(&write));
    }

    #[test]
    fn test_wildcard_domain() {
        let wildcard = Capability::new("*:read").unwrap();
        let k8s = Capability::new("k8s:read").unwrap();
        let helm = Capability::new("helm:read").unwrap();
        assert!(wildcard.matches(&k8s));
        assert!(wildcard.matches(&helm));
    }

    #[test]
    fn test_full_wildcard() {
        let wildcard = Capability::new("*:*").unwrap();
        let any = Capability::new("k8s:admin").unwrap();
        assert!(wildcard.matches(&any));
    }

    #[test]
    fn test_different_domain_no_match() {
        let a = Capability::new("k8s:read").unwrap();
        let b = Capability::new("helm:read").unwrap();
        assert!(!a.matches(&b));
    }
}
