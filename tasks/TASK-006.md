# TASK-006: End-to-End MVP Integration

## Objective
Wire everything from TASK-001 through TASK-005 together into a working end-to-end flow: **Telegram message → Gateway → Agent → K8s Skill → LLM → Telegram reply**.

## Dependencies
- TASK-001 through TASK-005 must be complete

## This is the MVP milestone. After this task, MerlionClaw is usable.

## Steps

### 1. Integration wiring

Ensure the `cargo run -- run` command:
1. Loads config from `~/.merlionclaw/config.toml` (or `--config` path)
2. Initializes tracing
3. Creates LLM provider (Anthropic) from config
4. Creates K8s skill handler (auto-detect kubeconfig vs in-cluster)
5. Discovers SKILL.md files from `skills/` directory
6. Builds SkillRegistry with K8s skill handler registered
7. Creates Agent with provider + registry
8. Starts Gateway with agent attached
9. Starts Telegram adapter (if enabled)
10. Waits for shutdown signal

### 2. System prompt assembly

The agent's system prompt should be:

```
You are MerlionClaw 🦁, an Infrastructure Agent Runtime.
You are a DevOps/SRE assistant that helps manage Kubernetes clusters, 
deployments, and infrastructure.

You have access to the following skills and tools. Use them when the user 
asks about infrastructure operations.

{skill_system_prompts}

When presenting results:
- Format tables with aligned columns
- Highlight warnings (CrashLoopBackOff, OOMKilled, high restart counts)
- Always mention the namespace and cluster context
- Be concise but include relevant details

If you're unsure about a destructive operation, ask for confirmation first.
```

### 3. Conversation context management

Implement basic context in `mclaw-agent/src/context.rs`:
- Keep last N messages per session (default: 20)
- Include system prompt at the start
- Trim old messages when approaching token limit (rough estimate: 4 chars = 1 token)
- Reset context on `/reset` command from user

### 4. Error handling end-to-end

Make sure errors at each layer produce user-friendly messages:
- K8s API error (403) → "Permission denied. Check the ServiceAccount RBAC."
- K8s API error (connection refused) → "Cannot connect to K8s API. Is kubeconfig configured?"
- LLM API error (401) → "Invalid API key. Check ANTHROPIC_API_KEY."
- LLM API error (429) → "Rate limited. Retrying in a moment..."
- Skill not found → "I don't have a skill for that. Available skills: k8s"
- Permission denied → "This operation requires `k8s:write` permission, which is not granted."

### 5. Special commands

Handle these directly in the agent (not via LLM):
- `/status` → show gateway status, active sessions, connected channels
- `/reset` → clear conversation context for this session
- `/skills` → list registered skills and their tools
- `/help` → show available commands

### 6. Onboard wizard (stretch)

Implement `mclaw onboard`:
1. Check if config file exists, if not create `~/.merlionclaw/`
2. Prompt for Anthropic API key → write to config
3. Prompt for Telegram bot token → write to config
4. Auto-detect kubeconfig → confirm cluster name
5. Write `config.toml`
6. Run `mclaw doctor` to verify everything
7. Print "Ready! Start with: mclaw run"

### 7. Doctor command

Implement `mclaw doctor`:
- Check config file exists and parses
- Check Anthropic API key validity (make a tiny API call)
- Check Telegram bot token (call getMe)
- Check K8s connectivity (server version)
- Check skills directory exists and has parseable SKILL.md files
- Print results with ✓/✗ status

## Validation

### Happy path test
```bash
# Terminal 1:
ANTHROPIC_API_KEY=sk-xxx TELEGRAM_BOT_TOKEN=xxx cargo run -- run

# Telegram:
You: "hi, what can you do?"
Bot: "I'm MerlionClaw, your infrastructure agent. I can help with Kubernetes operations..."

You: "list all pods in kube-system"
Bot: [formatted table of pods with name, status, restarts, age]

You: "show me the logs of coredns"
Bot: [last 50 lines of coredns logs]

You: "how many deployments are there across all namespaces?"
Bot: [LLM uses k8s_list_deployments across namespaces, summarizes count]

You: /status
Bot: "Gateway: running | Sessions: 1 | Skills: k8s (4 tools) | Model: claude-sonnet-4-20250514"

You: /reset
Bot: "Context cleared."
```

### Error path test
```bash
# Without K8s cluster:
You: "list pods"
Bot: "Cannot connect to Kubernetes API. Make sure kubeconfig is configured or I'm running inside a cluster."
```

### Doctor test
```bash
cargo run -- doctor
# ✓ Config file: ~/.merlionclaw/config.toml
# ✓ Anthropic API: connected (claude-sonnet-4-20250514)
# ✓ Telegram bot: @MerlionClawBot
# ✓ Kubernetes: connected to minikube (v1.29.0)
# ✓ Skills: 1 skill loaded (k8s: 4 tools)
```

## Output

A fully working MVP where you can manage your Kubernetes cluster through Telegram using natural language. This is the demo-able milestone.
