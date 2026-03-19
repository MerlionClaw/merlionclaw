//! LLM provider abstraction layer.
//!
//! Defines a unified trait for calling language models, with support for
//! tool calling and streaming responses.

pub mod anthropic;

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use serde::{Deserialize, Serialize};

/// Trait implemented by each LLM provider (Anthropic, OpenAI, etc.).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a message and get a complete response.
    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse>;

    /// Send a message and get a streaming response.
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamEvent>> + Send>>>;

    /// Provider name for logging.
    fn name(&self) -> &str;
}

/// A request to an LLM.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Model identifier (e.g., "claude-sonnet-4-20250514").
    pub model: String,
    /// System prompt.
    pub system: String,
    /// Conversation messages.
    pub messages: Vec<ChatMessage>,
    /// Tool definitions available to the model.
    pub tools: Vec<ToolDefinition>,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
}

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message role.
    pub role: Role,
    /// Message content.
    pub content: MessageContent,
}

/// The role of a message sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Message content — either plain text or structured content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content.
    Text(String),
    /// Structured content blocks (text, tool_use, tool_result).
    Blocks(Vec<ContentBlock>),
}

/// A single content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content.
    #[serde(rename = "text")]
    Text { text: String },
    /// Tool use request from the assistant.
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result from the user.
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// A tool definition exposed to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// A streaming event from the LLM.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of text output.
    TextDelta(String),
    /// A tool use block started.
    ToolUseStart { id: String, name: String },
    /// A chunk of tool input JSON.
    ToolUseInputDelta(String),
    /// A tool use block completed.
    ToolUseEnd,
    /// The message is complete.
    MessageEnd { stop_reason: String },
}

/// A complete response from the LLM.
#[derive(Debug)]
pub struct CompletionResponse {
    /// Response content blocks.
    pub content: Vec<ContentBlock>,
    /// Why the model stopped generating.
    pub stop_reason: String,
    /// Token usage.
    pub usage: Usage,
}

/// Token usage information.
#[derive(Debug, Clone)]
pub struct Usage {
    /// Tokens in the input.
    pub input_tokens: u32,
    /// Tokens in the output.
    pub output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_text_serialization() {
        let msg = ChatMessage {
            role: Role::User,
            content: MessageContent::Text("hello".to_string()),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hello");
    }

    #[test]
    fn test_chat_message_blocks_serialization() {
        let msg = ChatMessage {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::Text {
                text: "hi".to_string(),
            }]),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "hi");
    }

    #[test]
    fn test_tool_use_block_serialization() {
        let block = ContentBlock::ToolUse {
            id: "tu_1".to_string(),
            name: "k8s_list_pods".to_string(),
            input: serde_json::json!({"namespace": "default"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["name"], "k8s_list_pods");
        assert_eq!(json["input"]["namespace"], "default");
    }

    #[test]
    fn test_tool_result_block_serialization() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tu_1".to_string(),
            content: "pod-1, pod-2".to_string(),
            is_error: None,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert!(json.get("is_error").is_none());
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "k8s_list_pods".to_string(),
            description: "List pods".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "namespace": {"type": "string"}
                }
            }),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "k8s_list_pods");
        assert_eq!(json["input_schema"]["type"], "object");
    }
}
