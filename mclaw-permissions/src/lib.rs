//! Capability-based permission engine for MerlionClaw.
//!
//! Skills declare required capabilities and the engine evaluates
//! policies to grant or deny execution. Default policy: deny all.

/// A capability that a skill may require.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// Kubernetes read access.
    K8sRead,
    /// Kubernetes write access.
    K8sWrite,
    /// Execute a specific CLI tool.
    Exec(String),
    /// Network access to a specific service.
    Net(String),
}
