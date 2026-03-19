//! WASM skill runtime using wasmtime.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use wasmtime::*;
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::{WasiCtxBuilder};

/// Manifest describing a WASM skill's tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmSkillManifest {
    /// Skill name.
    pub name: String,
    /// Skill version.
    pub version: String,
    /// Skill description.
    pub description: String,
    /// Required permissions.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Tool definitions.
    #[serde(default)]
    pub tools: Vec<WasmToolDef>,
}

/// A tool defined by a WASM skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmToolDef {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for input parameters.
    pub input_schema: serde_json::Value,
}

/// A loaded WASM skill module.
pub struct WasmSkill {
    module: Module,
    engine: Engine,
    /// Parsed manifest from the module.
    pub manifest: WasmSkillManifest,
}

/// The WASM skill runtime manages loading and executing WASM modules.
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    /// Create a new WASM runtime.
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        config.wasm_component_model(false);

        let engine = Engine::new(&config)?;

        info!("WASM runtime initialized");
        Ok(Self { engine })
    }

    /// Load a WASM skill from a file.
    pub async fn load_skill(&self, wasm_path: &Path) -> anyhow::Result<WasmSkill> {
        let bytes = tokio::fs::read(wasm_path).await?;
        let module = Module::new(&self.engine, &bytes)?;

        // Try to call manifest() to get skill metadata
        let manifest = match self.call_manifest(&module) {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "WASM module has no manifest, using defaults");
                let name = wasm_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                WasmSkillManifest {
                    name,
                    version: "0.0.0".to_string(),
                    description: "WASM skill".to_string(),
                    permissions: vec![],
                    tools: vec![],
                }
            }
        };

        debug!(name = %manifest.name, tools = manifest.tools.len(), "loaded WASM skill");

        Ok(WasmSkill {
            module,
            engine: self.engine.clone(),
            manifest,
        })
    }

    /// Call the manifest() export on a WASM module.
    fn call_manifest(&self, module: &Module) -> anyhow::Result<WasmSkillManifest> {
        let wasi = WasiCtxBuilder::new().build_p1();
        let mut store = Store::new(&self.engine, wasi);

        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx: &mut WasiP1Ctx| ctx)?;

        let instance = linker.instantiate(&mut store, module)?;

        // Call manifest() → returns ptr to JSON string
        let manifest_fn = instance
            .get_typed_func::<(), i32>(&mut store, "manifest")
            .map_err(|_| anyhow::anyhow!("module does not export manifest()"))?;

        let ptr = manifest_fn.call(&mut store, ())?;

        // Read the string from WASM memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("module has no memory export"))?;

        let json_str = read_wasm_string(&memory, &store, ptr as u32)?;
        let manifest: WasmSkillManifest = serde_json::from_str(&json_str)?;

        Ok(manifest)
    }

    /// Execute a tool in a WASM skill.
    pub async fn execute_tool(
        &self,
        skill: &WasmSkill,
        tool_name: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<String> {
        let wasi = WasiCtxBuilder::new().build_p1();
        let mut store = Store::new(&skill.engine, wasi);

        // Set execution deadline (~30 seconds)
        store.set_epoch_deadline(300);

        // Start epoch incrementer
        let engine = skill.engine.clone();
        let epoch_handle = tokio::task::spawn_blocking(move || {
            for _ in 0..300 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                engine.increment_epoch();
            }
        });

        let mut linker = Linker::new(&skill.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx: &mut WasiP1Ctx| ctx)?;

        let instance = linker.instantiate(&mut store, &skill.module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("module has no memory export"))?;

        // Allocate and write tool name
        let tool_bytes = tool_name.as_bytes();
        let tool_ptr = call_alloc(&instance, &mut store, tool_bytes.len() as u32 + 1)?;
        memory.write(&mut store, tool_ptr as usize, tool_bytes)?;
        memory.write(&mut store, tool_ptr as usize + tool_bytes.len(), &[0])?;

        // Allocate and write input JSON
        let input_str = serde_json::to_string(&input)?;
        let input_bytes = input_str.as_bytes();
        let input_ptr = call_alloc(&instance, &mut store, input_bytes.len() as u32 + 1)?;
        memory.write(&mut store, input_ptr as usize, input_bytes)?;
        memory.write(&mut store, input_ptr as usize + input_bytes.len(), &[0])?;

        // Call execute(tool_ptr, input_ptr) → result_ptr
        let execute_fn = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "execute")
            .map_err(|_| anyhow::anyhow!("module does not export execute()"))?;

        let result_ptr = execute_fn.call(&mut store, (tool_ptr, input_ptr))?;

        // Read result string
        let result = read_wasm_string(&memory, &store, result_ptr as u32)?;

        epoch_handle.abort();

        debug!(tool = tool_name, "WASM tool executed");
        Ok(result)
    }
}

/// Read a null-terminated string from WASM memory.
fn read_wasm_string(memory: &Memory, store: &Store<WasiP1Ctx>, ptr: u32) -> anyhow::Result<String> {
    let data = memory.data(store);
    let start = ptr as usize;

    if start >= data.len() {
        anyhow::bail!("string pointer out of bounds");
    }

    let end = data[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|pos| start + pos)
        .unwrap_or(data.len().min(start + 65536));

    let bytes = &data[start..end];
    Ok(String::from_utf8_lossy(bytes).to_string())
}

/// Call the alloc(size) function exported by the WASM module.
fn call_alloc(instance: &Instance, store: &mut Store<WasiP1Ctx>, size: u32) -> anyhow::Result<i32> {
    let alloc_fn = instance
        .get_typed_func::<i32, i32>(&mut *store, "alloc")
        .map_err(|_| anyhow::anyhow!("module does not export alloc()"))?;

    let ptr = alloc_fn.call(store, size as i32)?;
    Ok(ptr)
}

/// SkillHandler implementation for WASM skills.
pub struct WasmSkillHandler {
    runtime: WasmRuntime,
    skill: WasmSkill,
}

impl WasmSkillHandler {
    /// Create a handler from a loaded WASM skill.
    pub fn new(runtime: WasmRuntime, skill: WasmSkill) -> Self {
        Self { runtime, skill }
    }
}

#[async_trait]
impl mclaw_agent::agent::ToolDispatcher for WasmSkillHandler {
    async fn dispatch(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
        self.runtime.execute_tool(&self.skill, tool_name, input).await
    }

    fn tool_definitions(&self) -> Vec<mclaw_agent::llm::ToolDefinition> {
        self.skill
            .manifest
            .tools
            .iter()
            .map(|t| mclaw_agent::llm::ToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }

    fn system_prompt(&self) -> String {
        String::new()
    }

    fn skills_summary(&self) -> String {
        format!(
            "WASM skill: {} v{} ({} tools)",
            self.skill.manifest.name,
            self.skill.manifest.version,
            self.skill.manifest.tools.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_deserialization() {
        let json = r#"{
            "name": "test-skill",
            "version": "1.0.0",
            "description": "A test WASM skill",
            "permissions": ["net:read"],
            "tools": [{
                "name": "ping",
                "description": "Ping a host",
                "input_schema": {"type": "object", "properties": {"host": {"type": "string"}}}
            }]
        }"#;

        let manifest: WasmSkillManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test-skill");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.tools[0].name, "ping");
    }

    #[test]
    fn test_runtime_creation() {
        let runtime = WasmRuntime::new();
        assert!(runtime.is_ok());
    }
}
