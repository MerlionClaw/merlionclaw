//! Agent loop — receives messages, calls LLM, executes tools, returns responses.

use tracing::{debug, info};

use crate::llm::{
    ChatMessage, CompletionRequest, ContentBlock, LlmProvider, MessageContent, Role,
};

/// The core agent that processes user messages via an LLM.
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    model: String,
    system_prompt: String,
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
    Reply {
        session_id: String,
        content: String,
    },
    /// An error occurred.
    Error {
        session_id: String,
        message: String,
    },
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
        }
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

        let request = CompletionRequest {
            model: self.model.clone(),
            system: self.system_prompt.clone(),
            messages,
            tools: vec![], // Skills will be added in TASK-004
            max_tokens: 4096,
        };

        match self.provider.complete(request).await {
            Ok(response) => {
                debug!(stop_reason = %response.stop_reason, "LLM response received");

                // Extract text from response content blocks
                let text: String = response
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                AgentResponse::Reply {
                    session_id: chat.session_id,
                    content: text,
                }
            }
            Err(e) => AgentResponse::Error {
                session_id: chat.session_id,
                message: format!("LLM error: {e}"),
            },
        }
    }
}
