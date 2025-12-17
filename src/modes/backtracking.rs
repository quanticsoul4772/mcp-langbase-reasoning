//! Backtracking reasoning mode - restore from checkpoints and explore alternative paths

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::BACKTRACKING_PROMPT;
use crate::storage::{Checkpoint, SnapshotType, SqliteStorage, StateSnapshot, Storage, Thought};

/// Input parameters for backtracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktrackingParams {
    /// Checkpoint ID to restore from
    pub checkpoint_id: String,
    /// New direction or approach to try
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_direction: Option<String>,
    /// Optional session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Confidence threshold
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    0.8
}

/// Result of backtracking operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktrackingResult {
    /// The ID of the new thought created after backtracking.
    pub thought_id: String,
    /// The session ID.
    pub session_id: String,
    /// The ID of the checkpoint that was restored.
    pub checkpoint_restored: String,
    /// The new thought content generated after restoration.
    pub content: String,
    /// Confidence in the new thought (0.0-1.0).
    pub confidence: f64,
    /// Optional new branch ID if branching from checkpoint.
    pub new_branch_id: Option<String>,
    /// The ID of the state snapshot created for this backtrack.
    pub snapshot_id: String,
}

/// Langbase response for backtracking
#[allow(dead_code)] // Fields needed for deserialization but not all are used directly
#[derive(Debug, Clone, Deserialize)]
struct BacktrackingResponse {
    thought: String,
    confidence: f64,
    #[serde(default)]
    context_restored: bool,
    #[serde(default)]
    branch_from: Option<String>,
    #[serde(default)]
    new_direction: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

impl BacktrackingResponse {
    fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<BacktrackingResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse backtracking response, using fallback"
                );
                // Fallback
                Self {
                    thought: completion.to_string(),
                    confidence: 0.8,
                    context_restored: true,
                    branch_from: None,
                    new_direction: None,
                    metadata: None,
                }
            }
        }
    }
}

/// Backtracking mode handler for checkpoint-based exploration.
pub struct BacktrackingMode {
    /// Storage backend for persisting data.
    storage: SqliteStorage,
    /// Langbase client for LLM-powered reasoning.
    langbase: LangbaseClient,
    /// The Langbase pipe name for backtracking.
    pipe_name: String,
}

impl BacktrackingMode {
    /// Create a new backtracking mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            pipe_name: config
                .pipes
                .backtracking
                .clone()
                .unwrap_or_else(|| "backtracking-reasoning-v1".to_string()),
        }
    }

    /// Process a backtracking request
    pub async fn process(&self, params: BacktrackingParams) -> AppResult<BacktrackingResult> {
        let start = Instant::now();

        // Get the checkpoint
        let checkpoint = self
            .storage
            .get_checkpoint(&params.checkpoint_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "checkpoint_id".to_string(),
                reason: format!("Checkpoint not found: {}", params.checkpoint_id),
            })?;

        debug!(checkpoint_id = %checkpoint.id, "Restoring from checkpoint");

        // Get or verify session
        let session = match &params.session_id {
            Some(id) => {
                if id != &checkpoint.session_id {
                    return Err(ToolError::Validation {
                        field: "session_id".to_string(),
                        reason: "Session ID does not match checkpoint".to_string(),
                    }
                    .into());
                }
                self.storage
                    .get_session(id)
                    .await?
                    .ok_or_else(|| ToolError::Validation {
                        field: "session_id".to_string(),
                        reason: format!("Session not found: {}", id),
                    })?
            }
            None => self
                .storage
                .get_session(&checkpoint.session_id)
                .await?
                .ok_or_else(|| ToolError::Validation {
                    field: "checkpoint_id".to_string(),
                    reason: format!(
                        "Session for checkpoint not found: {}",
                        checkpoint.session_id
                    ),
                })?,
        };

        // Create a state snapshot before backtracking
        let snapshot = StateSnapshot::new(&session.id, checkpoint.snapshot.clone())
            .with_type(SnapshotType::Branch)
            .with_description(format!("Backtrack from checkpoint: {}", checkpoint.name));

        self.storage.create_snapshot(&snapshot).await?;

        // Build context for Langbase
        let messages = self.build_messages(&checkpoint, params.new_direction.as_deref());

        // Call Langbase pipe
        let request = PipeRequest::new(&self.pipe_name, messages);
        let response = self.langbase.call_pipe(request).await?;

        // Parse response
        let backtrack_response = BacktrackingResponse::from_completion(&response.completion);

        // Create the new thought
        let thought = Thought::new(&session.id, &backtrack_response.thought, "backtracking")
            .with_confidence(backtrack_response.confidence.max(params.confidence));

        self.storage.create_thought(&thought).await?;

        let latency = start.elapsed().as_millis() as i64;
        info!(
            session_id = %session.id,
            thought_id = %thought.id,
            checkpoint_id = %checkpoint.id,
            latency_ms = latency,
            "Backtracking completed"
        );

        Ok(BacktrackingResult {
            thought_id: thought.id,
            session_id: session.id,
            checkpoint_restored: checkpoint.id,
            content: backtrack_response.thought,
            confidence: backtrack_response.confidence,
            new_branch_id: checkpoint.branch_id,
            snapshot_id: snapshot.id,
        })
    }

    /// Build messages for the Langbase pipe
    fn build_messages(&self, checkpoint: &Checkpoint, new_direction: Option<&str>) -> Vec<Message> {
        let mut messages = Vec::new();

        messages.push(Message::system(BACKTRACKING_PROMPT));

        // Add checkpoint context
        let checkpoint_context = format!(
            "Restoring from checkpoint: {}\n\nCheckpoint state:\n{}\n\n{}",
            checkpoint.name,
            serde_json::to_string_pretty(&checkpoint.snapshot).unwrap_or_default(),
            checkpoint
                .description
                .as_ref()
                .map(|d| format!("Description: {}", d))
                .unwrap_or_default()
        );

        messages.push(Message::user(checkpoint_context));

        // Add new direction if provided
        if let Some(direction) = new_direction {
            messages.push(Message::user(format!(
                "New direction to explore: {}",
                direction
            )));
        } else {
            messages.push(Message::user(
                "Please continue reasoning from this checkpoint, exploring an alternative approach."
                    .to_string(),
            ));
        }

        messages
    }

    /// Create a checkpoint at the current state
    pub async fn create_checkpoint(
        &self,
        session_id: &str,
        name: &str,
        description: Option<&str>,
    ) -> AppResult<Checkpoint> {
        // Get current session state
        let thoughts = self.storage.get_session_thoughts(session_id).await?;
        let branches = self.storage.get_session_branches(session_id).await?;

        // Serialize state
        let state = serde_json::json!({
            "thoughts": thoughts,
            "branches": branches,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        let mut checkpoint = Checkpoint::new(session_id, name, state);
        if let Some(desc) = description {
            checkpoint = checkpoint.with_description(desc);
        }

        self.storage.create_checkpoint(&checkpoint).await?;

        info!(
            session_id = %session_id,
            checkpoint_id = %checkpoint.id,
            "Checkpoint created"
        );

        Ok(checkpoint)
    }

    /// List available checkpoints for a session
    pub async fn list_checkpoints(&self, session_id: &str) -> AppResult<Vec<Checkpoint>> {
        Ok(self.storage.get_session_checkpoints(session_id).await?)
    }
}

impl BacktrackingParams {
    /// Create new params with checkpoint ID
    pub fn new(checkpoint_id: impl Into<String>) -> Self {
        Self {
            checkpoint_id: checkpoint_id.into(),
            new_direction: None,
            session_id: None,
            confidence: default_confidence(),
        }
    }

    /// Set new direction
    pub fn with_direction(mut self, direction: impl Into<String>) -> Self {
        self.new_direction = Some(direction.into());
        self
    }

    /// Set session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // BacktrackingParams Tests
    // ============================================================================

    #[test]
    fn test_backtracking_params_new() {
        let params = BacktrackingParams::new("checkpoint-123");
        assert_eq!(params.checkpoint_id, "checkpoint-123");
        assert!(params.new_direction.is_none());
        assert!(params.session_id.is_none());
        assert_eq!(params.confidence, 0.8);
    }

    #[test]
    fn test_backtracking_params_with_direction() {
        let params = BacktrackingParams::new("cp-1").with_direction("Try a different approach");
        assert_eq!(
            params.new_direction,
            Some("Try a different approach".to_string())
        );
    }

    #[test]
    fn test_backtracking_params_with_session() {
        let params = BacktrackingParams::new("cp-1").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_backtracking_params_builder_chain() {
        let params = BacktrackingParams::new("cp-abc")
            .with_direction("Alternative path")
            .with_session("session-xyz");

        assert_eq!(params.checkpoint_id, "cp-abc");
        assert_eq!(params.new_direction, Some("Alternative path".to_string()));
        assert_eq!(params.session_id, Some("session-xyz".to_string()));
        assert_eq!(params.confidence, 0.8);
    }

    #[test]
    fn test_backtracking_params_serialize() {
        let params = BacktrackingParams::new("cp-1")
            .with_direction("New direction")
            .with_session("sess-1");

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("cp-1"));
        assert!(json.contains("New direction"));
        assert!(json.contains("sess-1"));
    }

    #[test]
    fn test_backtracking_params_deserialize() {
        let json = r#"{
            "checkpoint_id": "cp-123",
            "new_direction": "Try option B",
            "session_id": "sess-456",
            "confidence": 0.9
        }"#;

        let params: BacktrackingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.checkpoint_id, "cp-123");
        assert_eq!(params.new_direction, Some("Try option B".to_string()));
        assert_eq!(params.session_id, Some("sess-456".to_string()));
        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_backtracking_params_deserialize_minimal() {
        let json = r#"{"checkpoint_id": "cp-only"}"#;

        let params: BacktrackingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.checkpoint_id, "cp-only");
        assert!(params.new_direction.is_none());
        assert!(params.session_id.is_none());
        assert_eq!(params.confidence, 0.8); // default
    }

    #[test]
    fn test_backtracking_params_round_trip() {
        let original = BacktrackingParams::new("cp-round")
            .with_direction("Direction X")
            .with_session("sess-round");

        let json = serde_json::to_string(&original).unwrap();
        let parsed: BacktrackingParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.checkpoint_id, original.checkpoint_id);
        assert_eq!(parsed.new_direction, original.new_direction);
        assert_eq!(parsed.session_id, original.session_id);
    }

    // ============================================================================
    // BacktrackingResponse Tests
    // ============================================================================

    #[test]
    fn test_backtracking_response_from_json() {
        let json = r#"{"thought": "New approach", "confidence": 0.9, "context_restored": true}"#;
        let resp = BacktrackingResponse::from_completion(json);
        assert_eq!(resp.thought, "New approach");
        assert_eq!(resp.confidence, 0.9);
        assert!(resp.context_restored);
    }

    #[test]
    fn test_backtracking_response_from_plain_text() {
        let text = "Just plain text";
        let resp = BacktrackingResponse::from_completion(text);
        assert_eq!(resp.thought, "Just plain text");
        assert_eq!(resp.confidence, 0.8);
        assert!(resp.context_restored);
    }

    #[test]
    fn test_backtracking_response_with_all_fields() {
        let json = r#"{
            "thought": "Complete response",
            "confidence": 0.95,
            "context_restored": true,
            "branch_from": "branch-abc",
            "new_direction": "Exploring option C",
            "metadata": {"key": "value"}
        }"#;

        let resp = BacktrackingResponse::from_completion(json);
        assert_eq!(resp.thought, "Complete response");
        assert_eq!(resp.confidence, 0.95);
        assert!(resp.context_restored);
        assert_eq!(resp.branch_from, Some("branch-abc".to_string()));
        assert_eq!(resp.new_direction, Some("Exploring option C".to_string()));
        assert!(resp.metadata.is_some());
    }

    #[test]
    fn test_backtracking_response_defaults() {
        let json = r#"{"thought": "Minimal", "confidence": 0.7}"#;

        let resp = BacktrackingResponse::from_completion(json);
        assert_eq!(resp.thought, "Minimal");
        assert_eq!(resp.confidence, 0.7);
        assert!(!resp.context_restored); // default is false
        assert!(resp.branch_from.is_none());
        assert!(resp.new_direction.is_none());
        assert!(resp.metadata.is_none());
    }

    #[test]
    fn test_backtracking_response_invalid_json() {
        let invalid = "{ invalid json }";

        let resp = BacktrackingResponse::from_completion(invalid);
        assert_eq!(resp.thought, invalid);
        assert_eq!(resp.confidence, 0.8); // fallback default
        assert!(resp.context_restored); // fallback sets this to true
    }

    #[test]
    fn test_backtracking_response_empty_string() {
        let empty = "";

        let resp = BacktrackingResponse::from_completion(empty);
        assert_eq!(resp.thought, "");
        assert_eq!(resp.confidence, 0.8);
    }

    // ============================================================================
    // BacktrackingResult Tests
    // ============================================================================

    #[test]
    fn test_backtracking_result_serialize() {
        let result = BacktrackingResult {
            thought_id: "thought-123".to_string(),
            session_id: "sess-456".to_string(),
            checkpoint_restored: "cp-789".to_string(),
            content: "Backtracked content".to_string(),
            confidence: 0.85,
            new_branch_id: Some("branch-abc".to_string()),
            snapshot_id: "snap-xyz".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("thought-123"));
        assert!(json.contains("cp-789"));
        assert!(json.contains("Backtracked content"));
        assert!(json.contains("0.85"));
        assert!(json.contains("branch-abc"));
    }

    #[test]
    fn test_backtracking_result_deserialize() {
        let json = r#"{
            "thought_id": "t-1",
            "session_id": "s-1",
            "checkpoint_restored": "cp-1",
            "content": "Result content",
            "confidence": 0.9,
            "new_branch_id": "b-1",
            "snapshot_id": "snap-1"
        }"#;

        let result: BacktrackingResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.thought_id, "t-1");
        assert_eq!(result.session_id, "s-1");
        assert_eq!(result.checkpoint_restored, "cp-1");
        assert_eq!(result.content, "Result content");
        assert_eq!(result.confidence, 0.9);
        assert_eq!(result.new_branch_id, Some("b-1".to_string()));
        assert_eq!(result.snapshot_id, "snap-1");
    }

    #[test]
    fn test_backtracking_result_without_branch() {
        let result = BacktrackingResult {
            thought_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            checkpoint_restored: "cp-1".to_string(),
            content: "No branch".to_string(),
            confidence: 0.75,
            new_branch_id: None,
            snapshot_id: "snap-1".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: BacktrackingResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.new_branch_id.is_none());
    }

    #[test]
    fn test_backtracking_result_round_trip() {
        let original = BacktrackingResult {
            thought_id: "round-t".to_string(),
            session_id: "round-s".to_string(),
            checkpoint_restored: "round-cp".to_string(),
            content: "Round trip test".to_string(),
            confidence: 0.88,
            new_branch_id: Some("round-b".to_string()),
            snapshot_id: "round-snap".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: BacktrackingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.thought_id, original.thought_id);
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.checkpoint_restored, original.checkpoint_restored);
        assert_eq!(parsed.content, original.content);
        assert_eq!(parsed.confidence, original.confidence);
        assert_eq!(parsed.new_branch_id, original.new_branch_id);
        assert_eq!(parsed.snapshot_id, original.snapshot_id);
    }

    // ============================================================================
    // Default Function Tests
    // ============================================================================

    #[test]
    fn test_default_confidence() {
        assert_eq!(default_confidence(), 0.8);
    }

    // ============================================================================
    // Edge Case Tests
    // ============================================================================

    #[test]
    fn test_backtracking_params_empty_checkpoint_id() {
        let params = BacktrackingParams::new("");
        assert_eq!(params.checkpoint_id, "");
    }

    #[test]
    fn test_backtracking_params_unicode_direction() {
        let params = BacktrackingParams::new("cp-1").with_direction("探索新方向 🔄");

        assert_eq!(params.new_direction, Some("探索新方向 🔄".to_string()));
    }

    #[test]
    fn test_backtracking_response_high_confidence() {
        let json = r#"{"thought": "Very confident", "confidence": 1.0}"#;

        let resp = BacktrackingResponse::from_completion(json);
        assert_eq!(resp.confidence, 1.0);
    }

    #[test]
    fn test_backtracking_response_zero_confidence() {
        let json = r#"{"thought": "No confidence", "confidence": 0.0}"#;

        let resp = BacktrackingResponse::from_completion(json);
        assert_eq!(resp.confidence, 0.0);
    }

    #[test]
    fn test_backtracking_response_with_complex_metadata() {
        let json = r#"{
            "thought": "With metadata",
            "confidence": 0.8,
            "metadata": {
                "nested": {"key": "value"},
                "array": [1, 2, 3],
                "boolean": true
            }
        }"#;

        let resp = BacktrackingResponse::from_completion(json);
        assert!(resp.metadata.is_some());
        let meta = resp.metadata.unwrap();
        assert!(meta.get("nested").is_some());
        assert!(meta.get("array").is_some());
    }
}
