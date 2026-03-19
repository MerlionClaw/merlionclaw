//! Agent loop — receives messages, calls LLM, executes tools, returns responses.

use tracing::{debug, info, warn};

use crate::llm::{
    ChatMessage, CompletionRequest, ContentBlock, LlmProvider, MessageContent, Role,
    ToolDefinition,
};

/// Maximum number of tool call rounds before aborting.
const MAX_TOOL_ROUNDS: usize = 10;

const BASE_SYSTEM_PROMPT: &str = "\
You are MerlionClaw, an Infrastructure Agent Runtime.
You are a DevOps/SRE assistant that helps manage Kubernetes clusters, \
deployments, and infrastructure.

You have access to the following skills and tools. Use them when the user \
asks about infrastructure operations.";

const FORMATTING_PROMPT: &str = "\
When presenting results:
- Format tables with aligned columns
- Highlight warnings (CrashLoopBackOff, OOMKilled, high restart counts)
- Always mention the namespace and cluster context
- Be concise but include relevant details

If you're unsure about a destructive operation, ask for confirmation first.";

/// Trait for dispatching tool calls (implemented by SkillRegistry).
#[async_trait::async_trait]
pub trait ToolDispatcher: Send + Sync {
    /// Execute a tool call and return the result.
    async fn dispatch(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String>;

    /// Get all available tool definitions.
    fn tool_definitions(&self) -> Vec<ToolDefinition>;

    /// Get combined system prompt fragment.
    fn system_prompt(&self) -> String;

    /// Get a summary of registered skills for /skills command.
    fn skills_summary(&self) -> String;
}

/// The core agent that processes user messages via an LLM.
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    model: String,
    system_prompt: String,
    dispatcher: Option<Box<dyn ToolDispatcher>>,
}

/// A chat message from a user, ready for the agent to process.
#[derive(Debug)]
pub struct InboundChat {
    /// Session identifier.
    pub session_id: String,
    /// Sender identifier.
    pub sender: String,
    /// Message content.
    pub content: String,
    /// Conversation history for this session.
    pub history: Vec<ChatMessage>,
}

/// Agent's response to a user message.
#[derive(Debug)]
pub enum AgentResponse {
    /// A text reply.
    Reply { session_id: String, content: String },
    /// An error occurred.
    Error { session_id: String, message: String },
    /// Context was cleared.
    ContextCleared { session_id: String },
}

impl Agent {
    /// Create a new agent with the given LLM provider.
    pub fn new(provider: Box<dyn LlmProvider>, model: String) -> Self {
        Self {
            provider,
            model,
            system_prompt: format!("{BASE_SYSTEM_PROMPT}\n\n{FORMATTING_PROMPT}"),
            dispatcher: None,
        }
    }

    /// Set the tool dispatcher (skill registry).
    pub fn with_dispatcher(mut self, dispatcher: Box<dyn ToolDispatcher>) -> Self {
        let skill_prompt = dispatcher.system_prompt();
        if skill_prompt.is_empty() {
            self.system_prompt = format!("{BASE_SYSTEM_PROMPT}\n\n{FORMATTING_PROMPT}");
        } else {
            self.system_prompt =
                format!("{BASE_SYSTEM_PROMPT}\n\n{skill_prompt}\n\n{FORMATTING_PROMPT}");
        }
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Get the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Handle a special slash command. Returns Some if handled, None if not a command.
    fn handle_command(&self, content: &str, session_id: &str) -> Option<AgentResponse> {
        let trimmed = content.trim();

        match trimmed {
            "/help" => Some(AgentResponse::Reply {
                session_id: session_id.to_string(),
                content: "Available commands:\n\
                    /help    - Show this help\n\
                    /status  - Show gateway status\n\
                    /skills  - List available skills and tools\n\
                    /reset   - Clear conversation context"
                    .to_string(),
            }),
            "/reset" => Some(AgentResponse::ContextCleared {
                session_id: session_id.to_string(),
            }),
            "/skills" => {
                let summary = self
                    .dispatcher
                    .as_ref()
                    .map(|d| d.skills_summary())
                    .unwrap_or_else(|| "No skills registered.".to_string());
                Some(AgentResponse::Reply {
                    session_id: session_id.to_string(),
                    content: summary,
                })
            }
            "/status" => {
                let skills_count = self
                    .dispatcher
                    .as_ref()
                    .map(|d| d.tool_definitions().len())
                    .unwrap_or(0);
                Some(AgentResponse::Reply {
                    session_id: session_id.to_string(),
                    content: format!(
                        "Gateway: running | Model: {} | Tools: {}",
                        self.model, skills_count
                    ),
                })
            }
            _ => None,
        }
    }

    /// Process a user message and return a response.
    pub async fn handle_message(&self, chat: InboundChat) -> AgentResponse {
        info!(
            session_id = %chat.session_id,
            sender = %chat.sender,
            "processing message"
        );

        // Check for slash commands first
        if let Some(response) = self.handle_command(&chat.content, &chat.session_id) {
            return response;
        }

        let mut messages = chat.history;
        messages.push(ChatMessage {
            role: Role::User,
            content: MessageContent::Text(chat.content),
        });

        let tools = self
            .dispatcher
            .as_ref()
            .map(|d| d.tool_definitions())
            .unwrap_or_default();

        for round in 0..MAX_TOOL_ROUNDS {
            let request = CompletionRequest {
                model: self.model.clone(),
                system: self.system_prompt.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
                max_tokens: 4096,
            };

            let response = match self.provider.complete(request).await {
                Ok(r) => r,
                Err(e) => {
                    return AgentResponse::Error {
                        session_id: chat.session_id,
                        message: format_llm_error(&e),
                    };
                }
            };

            debug!(
                stop_reason = %response.stop_reason,
                round,
                "LLM response received"
            );

            // If the LLM wants to use tools, execute them and continue
            if response.stop_reason == "tool_use" {
                if let Some(dispatcher) = &self.dispatcher {
                    messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: MessageContent::Blocks(response.content.clone()),
                    });

                    let mut tool_results = Vec::new();
                    for block in &response.content {
                        if let ContentBlock::ToolUse { id, name, input } = block {
                            debug!(tool = %name, "executing tool call");
                            let result = match dispatcher.dispatch(name, input.clone()).await {
                                Ok(output) => ContentBlock::ToolResult {
                                    tool_use_id: id.clone(),
                                    content: output,
                                    is_error: None,
                                },
                                Err(e) => {
                                    warn!(tool = %name, error = %e, "tool execution failed");
                                    ContentBlock::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: format_skill_error(name, &e),
                                        is_error: Some(true),
                                    }
                                }
                            };
                            tool_results.push(result);
                        }
                    }

                    messages.push(ChatMessage {
                        role: Role::User,
                        content: MessageContent::Blocks(tool_results),
                    });

                    continue;
                }
            }

            // Extract text from the final response
            let text: String = response
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");

            return AgentResponse::Reply {
                session_id: chat.session_id,
                content: text,
            };
        }

        AgentResponse::Error {
            session_id: chat.session_id,
            message: "max tool call rounds exceeded".to_string(),
        }
    }
}

/// Format LLM errors into user-friendly messages.
fn format_llm_error(error: &anyhow::Error) -> String {
    let msg = error.to_string();
    if msg.contains("401") {
        "Invalid API key. Check ANTHROPIC_API_KEY.".to_string()
    } else if msg.contains("429") {
        "Rate limited. Please try again in a moment.".to_string()
    } else if msg.contains("529") || msg.contains("overloaded") {
        "The API is overloaded. Please try again shortly.".to_string()
    } else {
        format!("LLM error: {msg}")
    }
}

/// Format skill execution errors into user-friendly messages.
fn format_skill_error(tool_name: &str, error: &anyhow::Error) -> String {
    let msg = error.to_string();
    if msg.contains("403") || msg.contains("Forbidden") {
        format!("Permission denied for {tool_name}. Check ServiceAccount RBAC.")
    } else if msg.contains("connection refused") || msg.contains("Connection refused") {
        "Cannot connect to Kubernetes API. Is kubeconfig configured?".to_string()
    } else if msg.contains("not found") {
        format!("Resource not found: {msg}")
    } else {
        format!("Error executing {tool_name}: {msg}")
    }
}
