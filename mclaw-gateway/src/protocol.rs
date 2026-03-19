//! Typed JSON message protocol for WebSocket communication.

use serde::{Deserialize, Serialize};

/// Messages from channels/clients to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InboundMessage {
    /// User sent a chat message.
    #[serde(rename = "chat")]
    Chat {
        session_id: String,
        channel: ChannelKind,
        sender: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
    },
    /// Channel adapter registering itself.
    #[serde(rename = "register_channel")]
    RegisterChannel { channel: ChannelKind },
    /// Heartbeat.
    #[serde(rename = "ping")]
    Ping,
}

/// Messages from the gateway to channels/clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutboundMessage {
    /// Agent response to send to user.
    #[serde(rename = "reply")]
    Reply {
        session_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use: Option<serde_json::Value>,
    },
    /// Streaming chunk.
    #[serde(rename = "stream_chunk")]
    StreamChunk { session_id: String, delta: String },
    /// Stream complete.
    #[serde(rename = "stream_end")]
    StreamEnd { session_id: String },
    /// Heartbeat response.
    #[serde(rename = "pong")]
    Pong,
    /// Error.
    #[serde(rename = "error")]
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        message: String,
    },
}

/// Supported chat platform types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Telegram,
    Slack,
    Discord,
    Whatsapp,
    Cli,
    Webchat,
}

impl std::fmt::Display for ChannelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Telegram => write!(f, "telegram"),
            Self::Slack => write!(f, "slack"),
            Self::Discord => write!(f, "discord"),
            Self::Whatsapp => write!(f, "whatsapp"),
            Self::Cli => write!(f, "cli"),
            Self::Webchat => write!(f, "webchat"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inbound_ping_roundtrip() {
        let msg = InboundMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"ping"}"#);

        let parsed: InboundMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, InboundMessage::Ping));
    }

    #[test]
    fn test_inbound_chat_roundtrip() {
        let json = r#"{"type":"chat","session_id":"s1","channel":"cli","sender":"larry","content":"hello"}"#;
        let msg: InboundMessage = serde_json::from_str(json).unwrap();
        match msg {
            InboundMessage::Chat {
                session_id,
                channel,
                sender,
                content,
                reply_to,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(channel, ChannelKind::Cli);
                assert_eq!(sender, "larry");
                assert_eq!(content, "hello");
                assert!(reply_to.is_none());
            }
            _ => panic!("expected Chat"),
        }
    }

    #[test]
    fn test_outbound_pong() {
        let msg = OutboundMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"pong"}"#);
    }

    #[test]
    fn test_outbound_error() {
        let msg = OutboundMessage::Error {
            session_id: Some("s1".to_string()),
            message: "agent not configured".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("agent not configured"));
    }
}
