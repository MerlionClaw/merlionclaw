# TASK-003: LLM Abstraction Layer (Anthropic Client + Streaming)

## Objective
Implement the LLM provider abstraction and a working Anthropic Messages API client with tool calling and streaming support.

## Dependencies
- TASK-001 must be complete
- TASK-002 is nice-to-have but not required (can test LLM client independently)

## Steps

### 1. Define LLM provider trait (mclaw-agent/src/llm/mod.rs)

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a message and get a complete response
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Send a message and get a streaming response
    async fn stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>>;

    /// Provider name for logging
    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Content can be text or tool_use/tool_result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: String, #[serde(skip_serializing_if = "Option::is_none")] is_error: Option<bool> },
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUseStart { id: String, name: String },
    ToolUseInputDelta(String),
    ToolUseEnd,
    MessageEnd { stop_reason: String },
}

#[derive(Debug)]
pub struct CompletionResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: String,
    pub usage: Usage,
}

#[derive(Debug)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
```

### 2. Implement Anthropic client (mclaw-agent/src/llm/anthropic.rs)

- Use `reqwest` to call `https://api.anthropic.com/v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
- For streaming: use SSE with `stream: true`, parse `event: content_block_delta` etc.
- Map Anthropic's response format to our `CompletionResponse` / `StreamEvent` types
- Handle rate limits with exponential backoff (3 retries)
- Handle API errors gracefully: 401 (bad key), 429 (rate limit), 529 (overloaded)

**Streaming SSE events to handle:**
- `message_start` → extract model, usage
- `content_block_start` → if type=tool_use, emit ToolUseStart
- `content_block_delta` → if type=text_delta, emit TextDelta; if type=input_json_delta, emit ToolUseInputDelta
- `content_block_stop` → if was tool_use, emit ToolUseEnd
- `message_delta` → extract stop_reason
- `message_stop` → emit MessageEnd

### 3. Implement OpenAI client (mclaw-agent/src/llm/openai.rs)

- Call `https://api.openai.com/v1/chat/completions`
- Map OpenAI's tool_calls format to our unified types
- Lower priority than Anthropic — stub it with `todo!()` if needed, but the trait should compile

### 4. Implement agent loop skeleton (mclaw-agent/src/loop.rs)

The loop receives messages from the gateway and processes them:

```rust
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    skills: SkillRegistry,    // will come from mclaw-skills
    // permissions: PermissionEngine,  // Phase 2
    // memory: MemoryStore,            // Phase 2
}

impl Agent {
    pub async fn handle_message(&mut self, msg: InboundChat) -> Result<Vec<OutboundMessage>> {
        // 1. Build system prompt with available tools
        // 2. Retrieve relevant memory (Phase 2)
        // 3. Build messages array
        // 4. Call LLM
        // 5. If tool_use: execute skill → feed result → call LLM again (loop)
        // 6. If text: return as reply
    }
}
```

For now, the agent should work without skills — just echo the LLM's text response back.

### 5. Write tests

- Unit test: parse a real Anthropic SSE response (fixture in tests/fixtures/)
- Unit test: serialize a CompletionRequest to the correct Anthropic JSON format
- Integration test: if ANTHROPIC_API_KEY is set, make a real API call and verify response

## Validation

```bash
cargo test -p mclaw-agent
# Unit tests pass

# If ANTHROPIC_API_KEY is set:
cargo test -p mclaw-agent -- --ignored  # runs integration tests

# Manual test:
ANTHROPIC_API_KEY=sk-xxx cargo run -- run
# Connect via websocat, send a chat message
# Should get back an LLM-generated response
```

## Output

A working LLM abstraction that can make Anthropic API calls with tool calling support and SSE streaming. Agent loop skeleton that can process a simple chat message end-to-end.
