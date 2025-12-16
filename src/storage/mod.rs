mod sqlite;

pub use sqlite::SqliteStorage;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::StorageResult;

/// Session represents a reasoning context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Thought represents a single reasoning step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub confidence: f64,
    pub mode: String,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Invocation log entry for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invocation {
    pub id: String,
    pub session_id: Option<String>,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub pipe_name: Option<String>,
    pub latency_ms: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Session {
    /// Create a new session with the given mode
    pub fn new(mode: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            mode: mode.into(),
            created_at: now,
            updated_at: now,
            metadata: None,
        }
    }
}

impl Thought {
    /// Create a new thought in a session
    pub fn new(
        session_id: impl Into<String>,
        content: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            content: content.into(),
            confidence: 0.8,
            mode: mode.into(),
            parent_id: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set the confidence level
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the parent thought
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl Invocation {
    /// Create a new invocation log entry
    pub fn new(tool_name: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: None,
            tool_name: tool_name.into(),
            input,
            output: None,
            pipe_name: None,
            latency_ms: None,
            success: true,
            error: None,
            created_at: Utc::now(),
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the pipe name
    pub fn with_pipe(mut self, pipe_name: impl Into<String>) -> Self {
        self.pipe_name = Some(pipe_name.into());
        self
    }

    /// Mark as successful with output
    pub fn success(mut self, output: serde_json::Value, latency_ms: i64) -> Self {
        self.success = true;
        self.output = Some(output);
        self.latency_ms = Some(latency_ms);
        self
    }

    /// Mark as failed with error
    pub fn failure(mut self, error: impl Into<String>, latency_ms: i64) -> Self {
        self.success = false;
        self.error = Some(error.into());
        self.latency_ms = Some(latency_ms);
        self
    }
}

/// Storage trait for database operations
#[async_trait]
pub trait Storage: Send + Sync {
    // Session operations
    async fn create_session(&self, session: &Session) -> StorageResult<()>;
    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>>;
    async fn update_session(&self, session: &Session) -> StorageResult<()>;
    async fn delete_session(&self, id: &str) -> StorageResult<()>;

    // Thought operations
    async fn create_thought(&self, thought: &Thought) -> StorageResult<()>;
    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>>;
    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>>;
    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>>;

    // Invocation logging
    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;
}
