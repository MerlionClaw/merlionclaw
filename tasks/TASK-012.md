# TASK-012: WASM Skill Runtime

## Objective
Implement a WASM-based skill sandbox using wasmtime and WASI, enabling third-party skills to run in a secure, deterministic, language-agnostic sandbox with fine-grained capability control.

## Dependencies
- TASK-004 must be complete (skill engine)
- TASK-008 must be complete (permission engine)

## Why WASM?

OpenClaw skills are SKILL.md files — natural language instructions that the LLM interprets and executes via shell/tools. This is flexible but non-deterministic and hard to audit.

MerlionClaw WASM skills are compiled modules that:
- Run in a memory-safe sandbox (no arbitrary code execution on host)
- Have deterministic inputs/outputs (LLM decides WHAT to call, WASM decides HOW)
- Support any language that compiles to WASM (Rust, Go, Python, JS, C)
- Use WASI capabilities gated by the permission engine
- Are auditable (inspect the .wasm binary, review the source)

## Steps

### 1. WASM skill interface (mclaw-wasm/src/interface.rs)

Define the contract between the host (MerlionClaw) and the WASM guest (skill):

```rust
/// Host-side representation of a WASM skill
pub struct WasmSkill {
    engine: wasmtime::Engine,
    module: wasmtime::Module,
    manifest: WasmSkillManifest,
}

/// Manifest embedded in or alongside the WASM module
#[derive(Debug, Deserialize)]
pub struct WasmSkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub tools: Vec<WasmToolDef>,
}

#[derive(Debug, Deserialize)]
pub struct WasmToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
}
```

The WASM module exports:
```
// Required exports
fn manifest() -> *const u8          // returns JSON manifest
fn execute(tool: *const u8, input: *const u8) -> *const u8  // execute a tool

// Memory management
fn alloc(size: u32) -> *mut u8
fn dealloc(ptr: *mut u8, size: u32)
```

### 2. Host runtime (mclaw-wasm/src/runtime.rs)

```rust
pub struct WasmRuntime {
    engine: wasmtime::Engine,
    linker: wasmtime::Linker<WasmState>,
}

pub struct WasmState {
    /// WASI context with capability-limited filesystem and network
    wasi: wasmtime_wasi::WasiCtx,
    /// Allocated memory regions for data passing
    memory_allocations: Vec<(u32, u32)>,
}

impl WasmRuntime {
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);  // timeout support

        let engine = wasmtime::Engine::new(&config)?;
        let mut linker = wasmtime::Linker::new(&engine);

        // Add WASI with restricted capabilities
        wasmtime_wasi::add_to_linker_sync(&mut linker)?;

        Ok(Self { engine, linker })
    }

    /// Load a WASM skill from file
    pub async fn load_skill(&self, wasm_path: &Path) -> Result<WasmSkill> {
        let bytes = tokio::fs::read(wasm_path).await?;
        let module = wasmtime::Module::new(&self.engine, &bytes)?;

        // Call manifest() to get skill metadata
        let manifest = self.call_manifest(&module)?;

        Ok(WasmSkill { engine: self.engine.clone(), module, manifest })
    }

    /// Execute a tool in the WASM sandbox
    pub async fn execute(
        &self,
        skill: &WasmSkill,
        tool_name: &str,
        input: serde_json::Value,
        capabilities: &AllowedCapabilities,
    ) -> Result<String> {
        // 1. Create WASI context with limited capabilities
        let wasi = self.build_wasi_ctx(capabilities)?;

        // 2. Create store with state
        let mut store = wasmtime::Store::new(&self.engine, WasmState { wasi, .. });

        // 3. Set execution deadline (prevent infinite loops)
        store.set_epoch_deadline(100);  // ~10 seconds

        // 4. Instantiate module
        let instance = self.linker.instantiate(&mut store, &skill.module)?;

        // 5. Call execute(tool_name, input_json) → result_json
        let result = self.call_execute(&mut store, &instance, tool_name, &input)?;

        Ok(result)
    }
}
```

### 3. WASI capability gating

Map MerlionClaw permissions to WASI capabilities:

```rust
fn build_wasi_ctx(&self, capabilities: &AllowedCapabilities) -> Result<WasiCtx> {
    let mut builder = WasiCtxBuilder::new();

    // Filesystem: only if fs:read or fs:write is granted
    if capabilities.has("fs:read") {
        builder.preopened_dir("/data/readonly", ".", DirPerms::READ, FilePerms::READ)?;
    }
    if capabilities.has("fs:write") {
        builder.preopened_dir("/data/workspace", "./workspace", DirPerms::all(), FilePerms::all())?;
    }

    // Network: only if net:* is granted
    // Note: WASI networking is still evolving, may need wasi-sockets preview
    if capabilities.has_domain("net") {
        // Allow outbound TCP/HTTP
        builder.inherit_network()?;
    }

    // Environment: pass specific env vars only
    builder.env("SKILL_NAME", &capabilities.skill_name)?;

    // Stdout/stderr: captured for result
    builder.stdout(pipe::WritePipe::new(Vec::new()));
    builder.stderr(pipe::WritePipe::new(Vec::new()));

    Ok(builder.build())
}
```

### 4. WASM skill SDK (for skill authors)

Create a minimal Rust SDK crate that skill authors can use:

```rust
// merlionclaw-skill-sdk (published to crates.io)

/// Define a skill with tools
#[macro_export]
macro_rules! skill {
    ($name:expr, $version:expr, $desc:expr, [ $($tool:expr),* $(,)? ]) => {
        // Generate manifest() export
        // Generate alloc/dealloc exports
    };
}

/// Define a tool handler
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn execute(&self, input: serde_json::Value) -> Result<String, String>;
}

/// Register tools and generate the execute() export
#[macro_export]
macro_rules! register_tools {
    ($($tool:expr),* $(,)?) => { ... };
}
```

Example skill in Rust:
```rust
// examples/wasm-skill-example/src/lib.rs
use merlionclaw_skill_sdk::*;

struct PingTool;

impl Tool for PingTool {
    fn name(&self) -> &str { "ping" }
    fn description(&self) -> &str { "Ping a host and return latency" }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "host": { "type": "string", "description": "Hostname or IP" }
            },
            "required": ["host"]
        })
    }
    fn execute(&self, input: serde_json::Value) -> Result<String, String> {
        let host = input["host"].as_str().ok_or("missing host")?;
        // WASI network call to ping
        Ok(format!("Pong from {}: 12ms", host))
    }
}

register_tools!(PingTool);
```

Build with: `cargo build --target wasm32-wasip1 --release`

### 5. Skill discovery for WASM

Extend `SkillRegistry` to also discover `.wasm` files:

```
skills/
├── k8s/SKILL.md          # traditional MD skill
├── helm/SKILL.md
├── custom-ping/
│   └── skill.wasm         # WASM skill (manifest embedded)
└── custom-checker/
    ├── skill.wasm
    └── SKILL.md            # optional: additional LLM context
```

### 6. SkillHandler implementation

```rust
#[async_trait]
impl SkillHandler for WasmSkill {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> Result<String> {
        let runtime = WasmRuntime::new()?;
        runtime.execute(self, tool_name, input, &self.allowed_capabilities).await
    }
}
```

### 7. Resource limits

Enforce limits on WASM execution:
- **Memory**: Max 64MB per skill instance
- **Time**: Max 30 seconds per tool invocation (configurable)
- **CPU**: Use epoch interruption for cooperative scheduling
- **Disk**: Max 10MB write to workspace directory
- **Network**: Rate-limited outbound requests

## Validation

```bash
cargo test -p mclaw-wasm

# Build example WASM skill:
cd examples/wasm-skill-example
cargo build --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/ping_skill.wasm ~/.merlionclaw/skills/ping/skill.wasm

# Test:
cargo run -- run
You: "ping google.com"
Bot: "Pong from google.com: 12ms"

# Permission test:
# Configure ping skill with only net:read permission
# Try a skill that needs fs:write → should be denied

# Timeout test:
# Build a WASM skill with infinite loop → should timeout after 30s
```

## Output

A WASM skill runtime that enables secure, sandboxed, language-agnostic third-party skill execution. This is a major architectural differentiator from OpenClaw's SKILL.md-only approach, especially for enterprise environments where auditable, deterministic execution matters.
