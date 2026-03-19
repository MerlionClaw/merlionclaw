//! WASM skill runtime for MerlionClaw.
//!
//! Enables third-party skills to run in a secure wasmtime sandbox
//! with WASI capabilities gated by the permission engine.

pub mod runtime;
