# TASK-004: Skill Engine v1 (SKILL.md Parser + Tool Dispatch)

## Objective
Implement skill discovery, SKILL.md parsing, and the tool dispatch mechanism that connects LLM tool_use calls to actual skill execution.

## Dependencies
- TASK-003 must be complete (agent loop + LLM types)

## Steps

### 1. Define SKILL.md format

A skill is a directory containing a SKILL.md file:

```markdown
---
name: k8s
description: Manage Kubernetes clusters - list pods, deployments, services, view logs, exec into containers
version: 0.1.0
permissions:
  - k8s:read
  - k8s:write
tools:
  - name: k8s_list_pods
    description: List pods in a namespace
    parameters:
      namespace:
        type: string
        description: Kubernetes namespace
        default: default
      label_selector:
        type: string
        description: Label selector (e.g., app=nginx)
        required: false
  - name: k8s_get_logs
    description: Get logs from a pod
    parameters:
      pod:
        type: string
        description: Pod name
        required: true
      namespace:
        type: string
        description: Kubernetes namespace
        default: default
      container:
        type: string
        description: Container name (if multi-container pod)
        required: false
      lines:
        type: integer
        description: Number of lines to return
        default: 50
---

# Kubernetes Skill

You are a Kubernetes operations assistant. When managing pods:
- Always confirm the namespace before destructive operations
- Show pod status, restarts, and age
- For logs, default to the last 50 lines unless specified
- Warn if a pod is in CrashLoopBackOff before showing logs
```

### 2. Implement SKILL.md parser (mclaw-skills/src/parser.rs)

- Split on `---` to extract YAML frontmatter and markdown body
- Parse frontmatter with serde_yaml into `SkillManifest` struct
- Parse tool definitions into `ToolDefinition` (from mclaw-agent)
- The markdown body becomes the skill's system prompt fragment

```rust
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub version: String,
    pub permissions: Vec<String>,
    pub tools: Vec<SkillToolDef>,
}

pub struct SkillToolDef {
    pub name: String,
    pub description: String,
    pub parameters: IndexMap<String, ParameterDef>,
}

pub struct ParameterDef {
    pub r#type: String,          // string, integer, boolean, array
    pub description: String,
    #[serde(default)]
    pub required: Option<bool>,  // default true if not specified
    pub default: Option<serde_json::Value>,
}

pub struct ParsedSkill {
    pub manifest: SkillManifest,
    pub system_prompt_fragment: String,  // the markdown body
}
```

### 3. Implement skill registry (mclaw-skills/src/registry.rs)

```rust
pub struct SkillRegistry {
    skills: HashMap<String, RegisteredSkill>,
}

pub struct RegisteredSkill {
    pub parsed: ParsedSkill,
    pub handler: Box<dyn SkillHandler>,
}

#[async_trait]
pub trait SkillHandler: Send + Sync {
    /// Execute a tool call and return the result as a string
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> Result<String>;
}

impl SkillRegistry {
    /// Discover skills from a directory
    pub fn discover(skills_dir: &Path) -> Result<Self>;

    /// Convert all registered tools to LLM ToolDefinitions
    pub fn tool_definitions(&self) -> Vec<ToolDefinition>;

    /// Find the skill that owns a tool name and execute it
    pub async fn dispatch(&self, tool_name: &str, input: serde_json::Value) -> Result<String>;

    /// Build combined system prompt fragment from all active skills
    pub fn system_prompt(&self) -> String;
}
```

### 4. Implement K8s skill handler (mclaw-skills/src/k8s.rs)

This is the first real skill. Use `kube` crate:

```rust
pub struct K8sSkill {
    client: kube::Client,
}

impl K8sSkill {
    pub async fn new() -> Result<Self> {
        // Try in-cluster config first, then kubeconfig
        let client = kube::Client::try_default().await?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SkillHandler for K8sSkill {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> Result<String> {
        match tool_name {
            "k8s_list_pods" => self.list_pods(input).await,
            "k8s_get_logs" => self.get_logs(input).await,
            "k8s_list_deployments" => self.list_deployments(input).await,
            "k8s_describe_pod" => self.describe_pod(input).await,
            _ => Err(anyhow!("unknown k8s tool: {}", tool_name)),
        }
    }
}
```

Implement at minimum:
- `k8s_list_pods` → Api::<Pod>::list with optional namespace + label_selector
- `k8s_get_logs` → Api::<Pod>::logs with params
- `k8s_list_deployments` → Api::<Deployment>::list
- `k8s_describe_pod` → get pod + format status, conditions, container statuses, events

Format output as a readable text table or structured text (not raw JSON) so the LLM can present it nicely to the user.

### 5. Wire skills into agent loop

Update `mclaw-agent` loop to:
1. Get `tool_definitions()` from registry → pass to LLM
2. Get `system_prompt()` from registry → append to system prompt
3. When LLM returns `tool_use`: call `registry.dispatch(name, input)` → create `tool_result` → continue loop

### 6. Create SKILL.md files

Write the actual SKILL.md files in `skills/k8s/SKILL.md` with all tool definitions for the K8s skill.

## Validation

```bash
cargo test -p mclaw-skills
# Parser tests pass, registry tests pass

# With a running K8s cluster (minikube/kind/real):
ANTHROPIC_API_KEY=sk-xxx cargo run -- run
# Connect via websocat:
# {"type":"chat","session_id":"test","channel":"cli","sender":"larry","content":"list all pods in the default namespace"}
# Should see LLM call k8s_list_pods, get real pod data, format a nice response
```

## Output

A working skill engine that parses SKILL.md files, registers tools with the LLM, dispatches tool calls to skill handlers, and returns results. K8s skill with at least 4 working tools.
