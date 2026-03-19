//! Per-sender session management.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::protocol::ChannelKind;

/// A conversation session tied to a specific user on a specific channel.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session key: `"{channel}:{sender}"`.
    pub key: String,
    /// Which channel this session is on.
    pub channel: ChannelKind,
    /// The sender identifier.
    pub sender: String,
    /// Conversation history (message content strings).
    pub history: Vec<String>,
    /// Last activity timestamp (epoch seconds).
    pub last_active: i64,
}

impl Session {
    /// Create a new session.
    pub fn new(channel: ChannelKind, sender: String) -> Self {
        let key = format!("{}:{}", channel, sender);
        Self {
            key,
            channel,
            sender,
            history: Vec::new(),
            last_active: chrono::Utc::now().timestamp(),
        }
    }

    /// Record a message in the session history and update the timestamp.
    pub fn record_message(&mut self, content: String) {
        self.last_active = chrono::Utc::now().timestamp();
        self.history.push(content);
    }
}

/// Manages all active sessions.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    timeout_secs: u64,
}

impl SessionManager {
    /// Create a new session manager with the given timeout.
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeout_secs,
        }
    }

    /// Get or create a session for the given channel and sender.
    pub async fn get_or_create(&self, channel: ChannelKind, sender: &str) -> Session {
        let key = format!("{channel}:{sender}");
        let mut sessions = self.sessions.write().await;

        // Check for expired session
        if let Some(existing) = sessions.get(&key) {
            let now = chrono::Utc::now().timestamp();
            if (now - existing.last_active) as u64 > self.timeout_secs {
                sessions.remove(&key);
            }
        }

        sessions
            .entry(key)
            .or_insert_with(|| Session::new(channel, sender.to_string()))
            .clone()
    }

    /// Record a message in an existing session.
    pub async fn record_message(&self, key: &str, content: String) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(key) {
            session.record_message(content);
        }
    }

    /// Return the number of active (non-expired) sessions.
    pub async fn active_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        let now = chrono::Utc::now().timestamp();
        sessions
            .values()
            .filter(|s| (now - s.last_active) as u64 <= self.timeout_secs)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_or_create_session() {
        let mgr = SessionManager::new(86400);
        let session = mgr.get_or_create(ChannelKind::Cli, "larry").await;
        assert_eq!(session.key, "cli:larry");
        assert_eq!(session.sender, "larry");
        assert!(session.history.is_empty());
    }

    #[tokio::test]
    async fn test_active_count() {
        let mgr = SessionManager::new(86400);
        assert_eq!(mgr.active_count().await, 0);

        mgr.get_or_create(ChannelKind::Cli, "larry").await;
        assert_eq!(mgr.active_count().await, 1);

        mgr.get_or_create(ChannelKind::Telegram, "bob").await;
        assert_eq!(mgr.active_count().await, 2);

        // Same session again — no increase
        mgr.get_or_create(ChannelKind::Cli, "larry").await;
        assert_eq!(mgr.active_count().await, 2);
    }
}
