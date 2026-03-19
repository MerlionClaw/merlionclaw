//! Conversation context management.
//!
//! Keeps a bounded history of messages per session and trims
//! when approaching token limits.

use crate::llm::ChatMessage;

/// Manages conversation history for a session.
#[derive(Debug, Clone)]
pub struct ConversationContext {
    /// Messages in this conversation.
    messages: Vec<ChatMessage>,
    /// Maximum number of messages to retain.
    max_messages: usize,
    /// Rough max tokens (4 chars ≈ 1 token).
    max_tokens: usize,
}

impl ConversationContext {
    /// Create a new context with default limits.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            max_messages: 20,
            max_tokens: 100_000,
        }
    }

    /// Create a context with custom limits.
    pub fn with_limits(max_messages: usize, max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_messages,
            max_tokens,
        }
    }

    /// Add a message and trim if needed.
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
        self.trim();
    }

    /// Get the current messages.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Clear all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Rough token estimate for all messages.
    fn estimated_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let content_len = match &m.content {
                    crate::llm::MessageContent::Text(t) => t.len(),
                    crate::llm::MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .map(|b| match b {
                            crate::llm::ContentBlock::Text { text } => text.len(),
                            crate::llm::ContentBlock::ToolUse { input, .. } => {
                                input.to_string().len()
                            }
                            crate::llm::ContentBlock::ToolResult { content, .. } => content.len(),
                        })
                        .sum(),
                };
                content_len / 4 + 1
            })
            .sum()
    }

    /// Trim old messages to stay within limits.
    fn trim(&mut self) {
        // Trim by count
        while self.messages.len() > self.max_messages {
            self.messages.remove(0);
        }

        // Trim by estimated tokens
        while self.estimated_tokens() > self.max_tokens && self.messages.len() > 2 {
            self.messages.remove(0);
        }
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatMessage, MessageContent, Role};

    #[test]
    fn test_push_and_get() {
        let mut ctx = ConversationContext::new();
        ctx.push(ChatMessage {
            role: Role::User,
            content: MessageContent::Text("hello".to_string()),
        });
        assert_eq!(ctx.messages().len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut ctx = ConversationContext::new();
        ctx.push(ChatMessage {
            role: Role::User,
            content: MessageContent::Text("hello".to_string()),
        });
        ctx.clear();
        assert!(ctx.messages().is_empty());
    }

    #[test]
    fn test_trim_by_count() {
        let mut ctx = ConversationContext::with_limits(3, 100_000);
        for i in 0..5 {
            ctx.push(ChatMessage {
                role: Role::User,
                content: MessageContent::Text(format!("msg {i}")),
            });
        }
        assert_eq!(ctx.messages().len(), 3);
    }
}
