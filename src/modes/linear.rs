//! Linear reasoning mode - sequential step-by-step reasoning.
//!
//! This module provides linear reasoning for step-by-step thought processing:
//! - Single-pass sequential reasoning
//! - Session continuity with thought history
//! - Confidence tracking

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use super::serialize_for_log;
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest, ReasoningResponse};
use crate::prompts::LINEAR_REASONING_PROMPT;
use crate::storage::{Invocation, SqliteStorage, Storage, Thought};

/// Input parameters for linear reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearParams {
    /// The thought content to process
    pub content: String,
    /// Optional session ID (creates new if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Confidence threshold (0.0-1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    0.8
}

/// Result of linear reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearResult {
    /// The ID of the created thought.
    pub thought_id: String,
    /// The session ID.
    pub session_id: String,
    /// The processed thought content.
    pub content: String,
    /// Confidence in the reasoning (0.0-1.0).
    pub confidence: f64,
    /// The ID of the previous thought in the chain, if any.
    pub previous_thought: Option<String>,
}

/// Linear reasoning mode handler for sequential reasoning.
#[derive(Clone)]
pub struct LinearMode {
    /// Storage backend for persisting data.
    storage: SqliteStorage,
    /// Langbase client for LLM-powered reasoning.
    langbase: LangbaseClient,
    /// The Langbase pipe name for linear reasoning.
    pipe_name: String,
}

impl LinearMode {
    /// Create a new linear mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            pipe_name: config.pipes.linear.clone(),
        }
    }

    /// Process a linear reasoning request
    pub async fn process(&self, params: LinearParams) -> AppResult<LinearResult> {
        let start = Instant::now();

        // Validate input
        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Content cannot be empty".to_string(),
            }
            .into());
        }

        // Get or create session
        let session = self
            .storage
            .get_or_create_session(&params.session_id, "linear")
            .await?;

        debug!(session_id = %session.id, "Processing linear reasoning");

        // Get previous thoughts for context
        let previous_thoughts = self.storage.get_session_thoughts(&session.id).await?;
        let previous_thought = previous_thoughts.last().cloned();

        // Build context for Langbase
        let messages = self.build_messages(&params.content, &previous_thoughts);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.linear",
            serialize_for_log(&params, "reasoning.linear input"),
        )
        .with_session(&session.id)
        .with_pipe(&self.pipe_name);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.pipe_name, messages);
        let response = match self.langbase.call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                self.storage.log_invocation(&invocation).await?;
                return Err(e.into());
            }
        };

        // Parse response
        let reasoning = ReasoningResponse::from_completion(&response.completion);

        // Create and store thought
        let thought = Thought::new(&session.id, &reasoning.thought, "linear")
            .with_confidence(reasoning.confidence.max(params.confidence));

        self.storage.create_thought(&thought).await?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&reasoning, "reasoning.linear output"),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

        info!(
            session_id = %session.id,
            thought_id = %thought.id,
            latency_ms = latency,
            "Linear reasoning completed"
        );

        Ok(LinearResult {
            thought_id: thought.id,
            session_id: session.id,
            content: reasoning.thought,
            confidence: reasoning.confidence,
            previous_thought: previous_thought.map(|t| t.id),
        })
    }

    /// Build messages for the Langbase pipe
    fn build_messages(&self, content: &str, history: &[Thought]) -> Vec<Message> {
        let mut messages = Vec::new();

        // System prompt for linear reasoning (from centralized prompts module)
        messages.push(Message::system(LINEAR_REASONING_PROMPT));

        // Add history context if available
        if !history.is_empty() {
            let history_text: Vec<String> =
                history.iter().map(|t| format!("- {}", t.content)).collect();

            messages.push(Message::user(format!(
                "Previous reasoning steps:\n{}\n\nNow process this thought:",
                history_text.join("\n")
            )));
        }

        // Add current content
        messages.push(Message::user(content.to_string()));

        messages
    }
}

impl LinearParams {
    /// Create new params with just content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            session_id: None,
            confidence: default_confidence(),
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the confidence threshold
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // LinearParams Tests
    // ============================================================================

    #[test]
    fn test_linear_params_new() {
        let params = LinearParams::new("Test content");
        assert_eq!(params.content, "Test content");
        assert!(params.session_id.is_none());
        assert_eq!(params.confidence, 0.8);
    }

    #[test]
    fn test_linear_params_with_session() {
        let params = LinearParams::new("Content").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_linear_params_with_confidence() {
        let params = LinearParams::new("Content").with_confidence(0.9);
        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_linear_params_confidence_clamped_high() {
        let params = LinearParams::new("Content").with_confidence(1.5);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_linear_params_confidence_clamped_low() {
        let params = LinearParams::new("Content").with_confidence(-0.5);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_linear_params_builder_chain() {
        let params = LinearParams::new("Chained")
            .with_session("my-session")
            .with_confidence(0.75);

        assert_eq!(params.content, "Chained");
        assert_eq!(params.session_id, Some("my-session".to_string()));
        assert_eq!(params.confidence, 0.75);
    }

    #[test]
    fn test_linear_params_serialize() {
        let params = LinearParams::new("Test")
            .with_session("sess-1")
            .with_confidence(0.85);

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("sess-1"));
        assert!(json.contains("0.85"));
    }

    #[test]
    fn test_linear_params_deserialize() {
        let json = r#"{"content": "Parsed", "session_id": "s-1", "confidence": 0.9}"#;
        let params: LinearParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.content, "Parsed");
        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_linear_params_deserialize_minimal() {
        let json = r#"{"content": "Only content"}"#;
        let params: LinearParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.content, "Only content");
        assert!(params.session_id.is_none());
        assert_eq!(params.confidence, 0.8); // default
    }

    // ============================================================================
    // LinearResult Tests
    // ============================================================================

    #[test]
    fn test_linear_result_serialize() {
        let result = LinearResult {
            thought_id: "thought-123".to_string(),
            session_id: "sess-456".to_string(),
            content: "Reasoning output".to_string(),
            confidence: 0.88,
            previous_thought: Some("thought-122".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("thought-123"));
        assert!(json.contains("sess-456"));
        assert!(json.contains("Reasoning output"));
        assert!(json.contains("0.88"));
    }

    #[test]
    fn test_linear_result_deserialize() {
        let json = r#"{
            "thought_id": "t-1",
            "session_id": "s-1",
            "content": "Result content",
            "confidence": 0.95,
            "previous_thought": "t-0"
        }"#;

        let result: LinearResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.thought_id, "t-1");
        assert_eq!(result.session_id, "s-1");
        assert_eq!(result.content, "Result content");
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.previous_thought, Some("t-0".to_string()));
    }

    #[test]
    fn test_linear_result_without_previous() {
        let result = LinearResult {
            thought_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            content: "First thought".to_string(),
            confidence: 0.8,
            previous_thought: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: LinearResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.previous_thought.is_none());
    }

    // ============================================================================
    // Default Function Tests
    // ============================================================================

    #[test]
    fn test_default_confidence() {
        assert_eq!(default_confidence(), 0.8);
    }
}
