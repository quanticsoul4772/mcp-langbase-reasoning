//! Backtracking reasoning mode - restore from checkpoints and explore alternative paths

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::ModeCore;
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
    /// Strict parsing that returns an error on parse failure
    fn from_completion_strict(completion: &str) -> Result<Self, ToolError> {
        serde_json::from_str::<BacktrackingResponse>(completion).map_err(|e| {
            let preview: String = completion.chars().take(200).collect();
            ToolError::ParseFailed {
                mode: "backtracking".to_string(),
                message: format!("JSON parse error: {} | Response preview: {}", e, preview),
            }
        })
    }

    /// Legacy parsing that falls back to defaults on parse failure (DEPRECATED)
    fn from_completion_legacy(completion: &str) -> Self {
        match serde_json::from_str::<BacktrackingResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse backtracking response, using fallback (DEPRECATED - enable STRICT_MODE)"
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

    /// Parse completion with strict mode control
    fn from_completion(completion: &str, strict_mode: bool) -> Result<Self, ToolError> {
        if strict_mode {
            Self::from_completion_strict(completion)
        } else {
            Ok(Self::from_completion_legacy(completion))
        }
    }
}

/// Backtracking mode handler for checkpoint-based exploration.
#[derive(Clone)]
pub struct BacktrackingMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for backtracking.
    pipe_name: String,
    /// Whether to use strict mode for response parsing (fails on parse errors instead of using fallbacks).
    strict_mode: bool,
}

impl BacktrackingMode {
    /// Create a new backtracking mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            pipe_name: config
                .pipes
                .backtracking
                .clone()
                .unwrap_or_else(|| "backtracking-reasoning-v1".to_string()),
            strict_mode: config.error_handling.strict_mode,
        }
    }

    /// Process a backtracking request
    pub async fn process(&self, params: BacktrackingParams) -> AppResult<BacktrackingResult> {
        let start = Instant::now();

        // Get the checkpoint
        let checkpoint = self
            .core
            .storage()
            .get_checkpoint(&params.checkpoint_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "checkpoint_id".to_string(),
                reason: format!("Checkpoint not found: {}", params.checkpoint_id),
            })?;

        debug!(checkpoint_id = %checkpoint.id, "Restoring from checkpoint");

        // Get or verify session
        let session =
            match &params.session_id {
                Some(id) => {
                    if id != &checkpoint.session_id {
                        return Err(ToolError::Validation {
                            field: "session_id".to_string(),
                            reason: "Session ID does not match checkpoint".to_string(),
                        }
                        .into());
                    }
                    self.core.storage().get_session(id).await?.ok_or_else(|| {
                        ToolError::Validation {
                            field: "session_id".to_string(),
                            reason: format!("Session not found: {}", id),
                        }
                    })?
                }
                None => self
                    .core
                    .storage()
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

        self.core.storage().create_snapshot(&snapshot).await?;

        // Build context for Langbase
        let messages = self.build_messages(&checkpoint, params.new_direction.as_deref());

        // Call Langbase pipe
        let request = PipeRequest::new(&self.pipe_name, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let backtrack_response = BacktrackingResponse::from_completion(&response.completion, self.strict_mode)?;

        // Create the new thought
        let thought = Thought::new(&session.id, &backtrack_response.thought, "backtracking")
            .with_confidence(backtrack_response.confidence.max(params.confidence));

        self.core.storage().create_thought(&thought).await?;

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
        let thoughts = self.core.storage().get_session_thoughts(session_id).await?;
        let branches = self.core.storage().get_session_branches(session_id).await?;

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

        self.core.storage().create_checkpoint(&checkpoint).await?;

        info!(
            session_id = %session_id,
            checkpoint_id = %checkpoint.id,
            "Checkpoint created"
        );

        Ok(checkpoint)
    }

    /// List available checkpoints for a session
    pub async fn list_checkpoints(&self, session_id: &str) -> AppResult<Vec<Checkpoint>> {
        Ok(self
            .core
            .storage()
            .get_session_checkpoints(session_id)
            .await?)
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
    // Helper Functions
    // ============================================================================

    fn create_test_config() -> Config {
        use crate::config::{DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig};
        use std::path::PathBuf;

        Config {
            langbase: LangbaseConfig {
                api_key: "test-key".to_string(),
                base_url: "https://api.langbase.com".to_string(),
            },
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Pretty,
            },
            request: crate::config::RequestConfig::default(),
            pipes: PipeConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
        }
    }

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
        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "New approach");
        assert_eq!(resp.confidence, 0.9);
        assert!(resp.context_restored);
    }

    #[test]
    fn test_backtracking_response_from_plain_text() {
        let text = "Just plain text";
        let resp = BacktrackingResponse::from_completion(text, false).unwrap();
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

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
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

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "Minimal");
        assert_eq!(resp.confidence, 0.7);
        assert!(!resp.context_restored); // default is false
        assert!(resp.branch_from.is_none());
        assert!(resp.new_direction.is_none());
        assert!(resp.metadata.is_none());
    }

    #[test]
    fn test_backtracking_response_invalid_json_legacy() {
        let invalid = "{ invalid json }";

        // Legacy mode (strict_mode = false) uses fallback
        let resp = BacktrackingResponse::from_completion(invalid, false).unwrap();
        assert_eq!(resp.thought, invalid);
        assert_eq!(resp.confidence, 0.8); // fallback default
        assert!(resp.context_restored); // fallback sets this to true
    }

    #[test]
    fn test_backtracking_response_invalid_json_strict() {
        let invalid = "{ invalid json }";

        // Strict mode should return error
        let result = BacktrackingResponse::from_completion(invalid, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::ParseFailed { mode, .. } if mode == "backtracking"));
    }

    #[test]
    fn test_backtracking_response_empty_string() {
        let empty = "";

        let resp = BacktrackingResponse::from_completion(empty, false).unwrap();
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

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, 1.0);
    }

    #[test]
    fn test_backtracking_response_zero_confidence() {
        let json = r#"{"thought": "No confidence", "confidence": 0.0}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
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

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert!(resp.metadata.is_some());
        let meta = resp.metadata.unwrap();
        assert!(meta.get("nested").is_some());
        assert!(meta.get("array").is_some());
    }

    // ============================================================================
    // Additional Edge Cases
    // ============================================================================

    #[test]
    fn test_backtracking_params_confidence_edge_values() {
        let json_min = r#"{"checkpoint_id": "cp-1", "confidence": 0.0}"#;
        let json_max = r#"{"checkpoint_id": "cp-2", "confidence": 1.0}"#;

        let params_min: BacktrackingParams = serde_json::from_str(json_min).unwrap();
        let params_max: BacktrackingParams = serde_json::from_str(json_max).unwrap();

        assert_eq!(params_min.confidence, 0.0);
        assert_eq!(params_max.confidence, 1.0);
    }

    #[test]
    fn test_backtracking_params_very_long_direction() {
        let long_direction = "A".repeat(10000);
        let params = BacktrackingParams::new("cp-1").with_direction(long_direction.clone());

        assert_eq!(params.new_direction, Some(long_direction));
    }

    #[test]
    fn test_backtracking_params_special_characters() {
        let special = "Test with \n newlines \t tabs and \"quotes\" and 'apostrophes'";
        let params = BacktrackingParams::new("cp-1").with_direction(special);

        let json = serde_json::to_string(&params).unwrap();
        let parsed: BacktrackingParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.new_direction, Some(special.to_string()));
    }

    #[test]
    fn test_backtracking_result_confidence_bounds() {
        let result_low = BacktrackingResult {
            thought_id: "t-low".to_string(),
            session_id: "s-1".to_string(),
            checkpoint_restored: "cp-1".to_string(),
            content: "Low confidence".to_string(),
            confidence: 0.0,
            new_branch_id: None,
            snapshot_id: "snap-1".to_string(),
        };

        let result_high = BacktrackingResult {
            thought_id: "t-high".to_string(),
            session_id: "s-1".to_string(),
            checkpoint_restored: "cp-1".to_string(),
            content: "High confidence".to_string(),
            confidence: 1.0,
            new_branch_id: None,
            snapshot_id: "snap-1".to_string(),
        };

        assert_eq!(result_low.confidence, 0.0);
        assert_eq!(result_high.confidence, 1.0);
    }

    #[test]
    fn test_backtracking_response_malformed_but_valid_json() {
        let json = r#"{"thought":"No spaces","confidence":0.5,"context_restored":false}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "No spaces");
        assert_eq!(resp.confidence, 0.5);
        assert!(!resp.context_restored);
    }

    #[test]
    fn test_backtracking_params_skip_serializing_none_fields() {
        let params = BacktrackingParams::new("cp-1");

        let json = serde_json::to_string(&params).unwrap();
        // Fields with None should be skipped
        assert!(!json.contains("new_direction"));
        assert!(!json.contains("session_id"));
    }

    #[test]
    fn test_backtracking_params_negative_confidence() {
        let json = r#"{"checkpoint_id": "cp-1", "confidence": -0.5}"#;
        let params: BacktrackingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.confidence, -0.5);
    }

    #[test]
    fn test_backtracking_params_very_high_confidence() {
        let json = r#"{"checkpoint_id": "cp-1", "confidence": 99.9}"#;
        let params: BacktrackingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.confidence, 99.9);
    }

    #[test]
    fn test_backtracking_response_with_null_metadata() {
        let json = r#"{"thought": "Test", "confidence": 0.8, "metadata": null}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "Test");
        assert!(resp.metadata.is_none());
    }

    #[test]
    fn test_backtracking_response_negative_confidence() {
        let json = r#"{"thought": "Negative", "confidence": -1.0}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, -1.0);
    }

    #[test]
    fn test_backtracking_result_empty_strings() {
        let result = BacktrackingResult {
            thought_id: "".to_string(),
            session_id: "".to_string(),
            checkpoint_restored: "".to_string(),
            content: "".to_string(),
            confidence: 0.0,
            new_branch_id: Some("".to_string()),
            snapshot_id: "".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: BacktrackingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.thought_id, "");
        assert_eq!(parsed.content, "");
    }

    #[test]
    fn test_backtracking_params_builder_idempotency() {
        let params1 = BacktrackingParams::new("cp-1")
            .with_direction("dir-1")
            .with_session("sess-1");

        let params2 = BacktrackingParams::new("cp-1")
            .with_direction("dir-1")
            .with_session("sess-1");

        assert_eq!(params1.checkpoint_id, params2.checkpoint_id);
        assert_eq!(params1.new_direction, params2.new_direction);
        assert_eq!(params1.session_id, params2.session_id);
    }

    #[test]
    fn test_backtracking_response_extra_fields() {
        let json = r#"{
            "thought": "Test",
            "confidence": 0.9,
            "extra_unknown_field": "should be ignored",
            "another_field": 123
        }"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "Test");
        assert_eq!(resp.confidence, 0.9);
    }

    #[test]
    fn test_backtracking_params_overwrite_values() {
        let params = BacktrackingParams::new("cp-original")
            .with_direction("dir-1")
            .with_direction("dir-2") // Overwrite
            .with_session("sess-1")
            .with_session("sess-2"); // Overwrite

        assert_eq!(params.new_direction, Some("dir-2".to_string()));
        assert_eq!(params.session_id, Some("sess-2".to_string()));
    }

    #[test]
    fn test_backtracking_response_unicode_content() {
        let json = r#"{"thought": "思考 🤔 émoji", "confidence": 0.8}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "思考 🤔 émoji");
    }

    // ============================================================================
    // BacktrackingMode Constructor Tests
    // ============================================================================

    #[test]
    fn test_backtracking_mode_new() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = BacktrackingMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "backtracking-reasoning-v1");
    }

    #[test]
    fn test_backtracking_mode_new_with_custom_pipe() {
        use crate::config::{PipeConfig, RequestConfig};
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let mut config = create_test_config();
        config.pipes = PipeConfig {
            backtracking: Some("custom-backtracking-pipe".to_string()),
            ..Default::default()
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = BacktrackingMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "custom-backtracking-pipe");
    }

    // ============================================================================
    // build_messages Tests
    // ============================================================================

    #[test]
    fn test_build_messages_without_direction() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{Checkpoint, SqliteStorage};
        use serde_json::json;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();
        let mode = BacktrackingMode::new(storage, langbase, &config);

        let checkpoint = Checkpoint::new(
            "session-1",
            "test-checkpoint",
            json!({"thoughts": [], "branches": []}),
        );

        let messages = mode.build_messages(&checkpoint, None);

        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages[0].role,
            crate::langbase::MessageRole::System
        ));
        assert!(matches!(
            messages[1].role,
            crate::langbase::MessageRole::User
        ));
        assert!(messages[1].content.contains("test-checkpoint"));
        assert!(matches!(
            messages[2].role,
            crate::langbase::MessageRole::User
        ));
        assert!(messages[2]
            .content
            .contains("Please continue reasoning from this checkpoint"));
    }

    #[test]
    fn test_build_messages_with_direction() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{Checkpoint, SqliteStorage};
        use serde_json::json;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();
        let mode = BacktrackingMode::new(storage, langbase, &config);

        let checkpoint = Checkpoint::new(
            "session-1",
            "test-checkpoint",
            json!({"thoughts": [], "branches": []}),
        );

        let messages = mode.build_messages(&checkpoint, Some("Try a different approach"));

        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages[0].role,
            crate::langbase::MessageRole::System
        ));
        assert!(matches!(
            messages[1].role,
            crate::langbase::MessageRole::User
        ));
        assert!(messages[1].content.contains("test-checkpoint"));
        assert!(matches!(
            messages[2].role,
            crate::langbase::MessageRole::User
        ));
        assert!(messages[2].content.contains("New direction to explore"));
        assert!(messages[2].content.contains("Try a different approach"));
    }

    #[test]
    fn test_build_messages_with_description() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{Checkpoint, SqliteStorage};
        use serde_json::json;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();
        let mode = BacktrackingMode::new(storage, langbase, &config);

        let checkpoint = Checkpoint::new(
            "session-1",
            "test-checkpoint",
            json!({"thoughts": [], "branches": []}),
        )
        .with_description("Important checkpoint");

        let messages = mode.build_messages(&checkpoint, None);

        assert_eq!(messages.len(), 3);
        assert!(messages[1].content.contains("Important checkpoint"));
        assert!(messages[1].content.contains("Description:"));
    }

    #[test]
    fn test_build_messages_with_complex_snapshot() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{Checkpoint, SqliteStorage};
        use serde_json::json;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();
        let mode = BacktrackingMode::new(storage, langbase, &config);

        let complex_snapshot = json!({
            "thoughts": [
                {"id": "t1", "content": "First thought"},
                {"id": "t2", "content": "Second thought"}
            ],
            "branches": ["branch-a", "branch-b"],
            "metadata": {"key": "value"}
        });

        let checkpoint =
            Checkpoint::new("session-1", "complex-checkpoint", complex_snapshot.clone());

        let messages = mode.build_messages(&checkpoint, None);

        assert_eq!(messages.len(), 3);
        // The snapshot should be serialized as pretty JSON in the message
        assert!(messages[1].content.contains("complex-checkpoint"));
        assert!(messages[1].content.contains("thoughts"));
        assert!(messages[1].content.contains("branches"));
    }

    #[test]
    fn test_build_messages_unicode_checkpoint_name() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{Checkpoint, SqliteStorage};
        use serde_json::json;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();
        let mode = BacktrackingMode::new(storage, langbase, &config);

        let checkpoint = Checkpoint::new("session-1", "检查点 🔖", json!({"data": "test"}));

        let messages = mode.build_messages(&checkpoint, Some("新方向 🚀"));

        assert_eq!(messages.len(), 3);
        assert!(messages[1].content.contains("检查点 🔖"));
        assert!(messages[2].content.contains("新方向 🚀"));
    }

    // ============================================================================
    // Clone Tests
    // ============================================================================

    #[test]
    fn test_backtracking_mode_clone() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode1 = BacktrackingMode::new(storage, langbase, &config);
        let mode2 = mode1.clone();

        assert_eq!(mode1.pipe_name, mode2.pipe_name);
    }

    // ============================================================================
    // Debug Trait Tests
    // ============================================================================

    #[test]
    fn test_backtracking_params_debug() {
        let params = BacktrackingParams::new("cp-debug")
            .with_direction("debug direction")
            .with_session("sess-debug");

        let debug_str = format!("{:?}", params);
        assert!(debug_str.contains("cp-debug"));
        assert!(debug_str.contains("debug direction"));
        assert!(debug_str.contains("sess-debug"));
    }

    #[test]
    fn test_backtracking_result_debug() {
        let result = BacktrackingResult {
            thought_id: "t-debug".to_string(),
            session_id: "s-debug".to_string(),
            checkpoint_restored: "cp-debug".to_string(),
            content: "Debug content".to_string(),
            confidence: 0.9,
            new_branch_id: Some("b-debug".to_string()),
            snapshot_id: "snap-debug".to_string(),
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("t-debug"));
        assert!(debug_str.contains("s-debug"));
        assert!(debug_str.contains("cp-debug"));
    }

    // ============================================================================
    // Additional Serialization Edge Cases
    // ============================================================================

    #[test]
    fn test_backtracking_params_with_float_precision() {
        let json = r#"{"checkpoint_id": "cp-1", "confidence": 0.123456789}"#;
        let params: BacktrackingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.confidence, 0.123456789);
    }

    #[test]
    fn test_backtracking_response_with_escaped_quotes() {
        let json = r#"{"thought": "She said \"hello\"", "confidence": 0.8}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "She said \"hello\"");
    }

    #[test]
    fn test_backtracking_response_with_newlines() {
        let json = r#"{"thought": "Line 1\nLine 2\nLine 3", "confidence": 0.8}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.thought, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_backtracking_result_with_unicode_ids() {
        let result = BacktrackingResult {
            thought_id: "思考-123".to_string(),
            session_id: "会话-456".to_string(),
            checkpoint_restored: "检查点-789".to_string(),
            content: "Unicode content".to_string(),
            confidence: 0.85,
            new_branch_id: None,
            snapshot_id: "快照-xyz".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: BacktrackingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.thought_id, "思考-123");
        assert_eq!(parsed.session_id, "会话-456");
        assert_eq!(parsed.checkpoint_restored, "检查点-789");
        assert_eq!(parsed.snapshot_id, "快照-xyz");
    }

    #[test]
    fn test_backtracking_params_from_into_string() {
        // Test that Into<String> trait works with &str
        let params1 = BacktrackingParams::new("checkpoint-from-str");
        let params2 = BacktrackingParams::new(String::from("checkpoint-from-string"));

        assert_eq!(params1.checkpoint_id, "checkpoint-from-str");
        assert_eq!(params2.checkpoint_id, "checkpoint-from-string");
    }

    #[test]
    fn test_backtracking_response_missing_required_fields_legacy() {
        // Test when required fields are missing - should fallback in legacy mode
        let json = r#"{"confidence": 0.9}"#; // Missing "thought"

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        // Should use fallback because parsing will fail
        assert!(resp.thought.len() > 0); // Will contain the original JSON
        assert_eq!(resp.confidence, 0.8); // Fallback default
    }

    #[test]
    fn test_backtracking_response_very_long_thought() {
        let long_thought = "A".repeat(100000);
        let json = format!(r#"{{"thought": "{}", "confidence": 0.8}}"#, long_thought);

        let resp = BacktrackingResponse::from_completion(&json, false).unwrap();
        assert_eq!(resp.thought, long_thought);
    }

    #[test]
    fn test_backtracking_params_clone() {
        let params1 = BacktrackingParams::new("cp-clone")
            .with_direction("Clone test")
            .with_session("sess-clone");

        let params2 = params1.clone();

        assert_eq!(params1.checkpoint_id, params2.checkpoint_id);
        assert_eq!(params1.new_direction, params2.new_direction);
        assert_eq!(params1.session_id, params2.session_id);
        assert_eq!(params1.confidence, params2.confidence);
    }

    #[test]
    fn test_backtracking_result_clone() {
        let result1 = BacktrackingResult {
            thought_id: "t-clone".to_string(),
            session_id: "s-clone".to_string(),
            checkpoint_restored: "cp-clone".to_string(),
            content: "Clone test".to_string(),
            confidence: 0.9,
            new_branch_id: Some("b-clone".to_string()),
            snapshot_id: "snap-clone".to_string(),
        };

        let result2 = result1.clone();

        assert_eq!(result1.thought_id, result2.thought_id);
        assert_eq!(result1.session_id, result2.session_id);
        assert_eq!(result1.checkpoint_restored, result2.checkpoint_restored);
        assert_eq!(result1.content, result2.content);
        assert_eq!(result1.confidence, result2.confidence);
        assert_eq!(result1.new_branch_id, result2.new_branch_id);
        assert_eq!(result1.snapshot_id, result2.snapshot_id);
    }

    #[test]
    fn test_backtracking_response_clone() {
        let json = r#"{"thought": "Clone test", "confidence": 0.9, "context_restored": true}"#;
        let resp1 = BacktrackingResponse::from_completion(json, false).unwrap();
        let resp2 = resp1.clone();

        assert_eq!(resp1.thought, resp2.thought);
        assert_eq!(resp1.confidence, resp2.confidence);
        assert_eq!(resp1.context_restored, resp2.context_restored);
    }

    // ============================================================================
    // Confidence Value Tests
    // ============================================================================

    #[test]
    fn test_backtracking_params_fractional_confidence() {
        let json = r#"{"checkpoint_id": "cp-1", "confidence": 0.123}"#;
        let params: BacktrackingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.confidence, 0.123);
    }

    #[test]
    fn test_backtracking_response_scientific_notation() {
        let json = r#"{"thought": "Test", "confidence": 1e-10}"#;

        let resp = BacktrackingResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, 1e-10);
    }

    #[test]
    fn test_backtracking_result_with_very_small_confidence() {
        let result = BacktrackingResult {
            thought_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            checkpoint_restored: "cp-1".to_string(),
            content: "Very low confidence".to_string(),
            confidence: 0.0001,
            new_branch_id: None,
            snapshot_id: "snap-1".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: BacktrackingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.confidence, 0.0001);
    }
}
