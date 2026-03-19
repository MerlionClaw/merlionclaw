//! Capability-based permission engine for MerlionClaw.
//!
//! Skills declare required capabilities and the engine evaluates
//! policies to grant or deny execution. Default policy: deny all.

pub mod capability;
pub mod policy;

pub use capability::Capability;
pub use policy::{DefaultPolicy, PermissionConfig, PermissionDecision, PermissionEngine, SkillPolicy};
