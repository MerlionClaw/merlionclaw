//! Agent loop — receives messages, calls LLM, executes tools, returns responses.

use tracing::{debug, info, warn};

use crate::llm::{
    ChatMessage, CompletionRequest, ContentBlock, LlmProvider, MessageContent, Role,
    ToolDefinition,
};

/// Maximum number of tool call rounds before aborting.
const MAX_TOOL_ROUNDS: usize = 10;

/// Trait for dispatching tool calls (implemented by SkillRegistry).
#[async_trait::async_trait]
pub trait ToolDispatcher: Send + Sync {
    /// Execute a tool call and return the result.
    async fn dispatch(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String>;

    /// Get all available tool definitions.
    fn tool_definitions(&self) -> Vec<ToolDefinition>;

    /// Get combined system prompt fragment.
    fn system_prompt(&self) -> String;
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
}

impl Agent {
    /// Create a new agent with the given LLM provider.
    pub fn new(provider: Box<dyn LlmProvider>, model: String) -> Self {
        Self {
            provider,
            model,
            system_prompt: "You are MerlionClaw, an infrastructure agent runtime. \
                You help with DevOps and SRE tasks including Kubernetes, Helm, \
                Istio, and observability workflows. Be concise and precise."
                .to_string(),
            dispatcher: None,
        }
    }

    /// Set the tool dispatcher (skill registry).
    pub fn with_dispatcher(mut self, dispatcher: Box<dyn ToolDispatcher>) -> Self {
        // Append skill system prompts
        let skill_prompt = dispatcher.system_prompt();
        if !skill_prompt.is_empty() {
            self.system_prompt = format!("{}\n\n{}", self.system_prompt, skill_prompt);
        }
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Process a user message and return a response.
    pub async fn handle_message(&self, chat: InboundChat) -> AgentResponse {
        info!(
            session_id = %chat.session_id,
            sender = %chat.sender,
            "processing message"
        );

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
                        message: format!("LLM error: {e}"),
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
                    // Add assistant message with the full response
                    messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: MessageContent::Blocks(response.content.clone()),
                    });

                    // Execute each tool call and build tool results
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
                                        content: format!("Error: {e}"),
                                        is_error: Some(true),
                                    }
                                }
                            };
                            tool_results.push(result);
                        }
                    }

                    // Add tool results as a user message
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
