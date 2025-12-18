//! Linear reasoning mode - sequential step-by-step reasoning.
//!
//! This module provides linear reasoning for step-by-step thought processing:
//! - Single-pass sequential reasoning
//! - Session continuity with thought history
//! - Confidence tracking

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use super::{serialize_for_log, ModeCore};
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
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for linear reasoning.
    pipe_name: String,
}

impl LinearMode {
    /// Create a new linear mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
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
            .core
            .storage()
            .get_or_create_session(&params.session_id, "linear")
            .await?;

        debug!(session_id = %session.id, "Processing linear reasoning");

        // Get previous thoughts for context
        let previous_thoughts = self
            .core
            .storage()
            .get_session_thoughts(&session.id)
            .await?;
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
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                self.core.storage().log_invocation(&invocation).await?;
                return Err(e.into());
            }
        };

        // Parse response
        let reasoning = ReasoningResponse::from_completion(&response.completion);

        // Create and store thought
        let thought = Thought::new(&session.id, &reasoning.thought, "linear")
            .with_confidence(reasoning.confidence.max(params.confidence));

        self.core.storage().create_thought(&thought).await?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&reasoning, "reasoning.linear output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

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
    use crate::config::RequestConfig;
    use crate::langbase::MessageRole;

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

    // ============================================================================
    // Edge Cases - Content Handling
    // ============================================================================

    #[test]
    fn test_linear_params_empty_content() {
        let params = LinearParams::new("");
        assert_eq!(params.content, "");
    }

    #[test]
    fn test_linear_params_very_long_content() {
        let long_content = "a".repeat(10000);
        let params = LinearParams::new(long_content.clone());
        assert_eq!(params.content, long_content);
        assert_eq!(params.content.len(), 10000);
    }

    #[test]
    fn test_linear_params_special_characters() {
        let special = "Test with special: \n\t\r\"'\\{}[]()!@#$%^&*";
        let params = LinearParams::new(special);
        assert_eq!(params.content, special);
    }

    #[test]
    fn test_linear_params_unicode_content() {
        let unicode = "Hello 世界 🌍 Привет مرحبا";
        let params = LinearParams::new(unicode);
        assert_eq!(params.content, unicode);
    }

    #[test]
    fn test_linear_params_multiline_content() {
        let multiline = "Line 1\nLine 2\nLine 3\nLine 4";
        let params = LinearParams::new(multiline);
        assert_eq!(params.content, multiline);
        assert!(params.content.contains('\n'));
    }

    #[test]
    fn test_linear_params_whitespace_only() {
        let whitespace = "   \t\n  ";
        let params = LinearParams::new(whitespace);
        assert_eq!(params.content, whitespace);
    }

    // ============================================================================
    // Confidence Edge Cases
    // ============================================================================

    #[test]
    fn test_linear_params_confidence_exactly_zero() {
        let params = LinearParams::new("Test").with_confidence(0.0);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_linear_params_confidence_exactly_one() {
        let params = LinearParams::new("Test").with_confidence(1.0);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_linear_params_confidence_exactly_half() {
        let params = LinearParams::new("Test").with_confidence(0.5);
        assert_eq!(params.confidence, 0.5);
    }

    #[test]
    fn test_linear_params_confidence_very_negative() {
        let params = LinearParams::new("Test").with_confidence(-999.9);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_linear_params_confidence_very_positive() {
        let params = LinearParams::new("Test").with_confidence(999.9);
        assert_eq!(params.confidence, 1.0);
    }

    // ============================================================================
    // ReasoningResponse::from_completion Tests
    // ============================================================================

    #[test]
    fn test_reasoning_response_valid_json() {
        let json = r#"{"thought": "Test thought", "confidence": 0.95, "metadata": null}"#;
        let response = ReasoningResponse::from_completion(json);
        assert_eq!(response.thought, "Test thought");
        assert_eq!(response.confidence, 0.95);
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_reasoning_response_with_metadata() {
        let json =
            r#"{"thought": "Meta thought", "confidence": 0.88, "metadata": {"key": "value"}}"#;
        let response = ReasoningResponse::from_completion(json);
        assert_eq!(response.thought, "Meta thought");
        assert_eq!(response.confidence, 0.88);
        assert!(response.metadata.is_some());
    }

    #[test]
    fn test_reasoning_response_invalid_json_fallback() {
        let invalid = "This is not JSON at all";
        let response = ReasoningResponse::from_completion(invalid);
        assert_eq!(response.thought, invalid);
        assert_eq!(response.confidence, 0.8);
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_reasoning_response_partial_json_fallback() {
        let partial = r#"{"thought": "incomplete""#;
        let response = ReasoningResponse::from_completion(partial);
        assert_eq!(response.thought, partial);
        assert_eq!(response.confidence, 0.8);
    }

    #[test]
    fn test_reasoning_response_empty_string_fallback() {
        let empty = "";
        let response = ReasoningResponse::from_completion(empty);
        assert_eq!(response.thought, empty);
        assert_eq!(response.confidence, 0.8);
    }

    #[test]
    fn test_reasoning_response_json_with_special_chars() {
        let json = r#"{"thought": "Special: \n\t\"quote\"", "confidence": 0.9, "metadata": null}"#;
        let response = ReasoningResponse::from_completion(json);
        assert!(response.thought.contains("Special"));
        assert_eq!(response.confidence, 0.9);
    }

    #[test]
    fn test_reasoning_response_minimal_valid_json() {
        let json = r#"{"thought": "T", "confidence": 0.1}"#;
        let response = ReasoningResponse::from_completion(json);
        assert_eq!(response.thought, "T");
        assert_eq!(response.confidence, 0.1);
    }

    #[test]
    fn test_reasoning_response_unicode_in_json() {
        let json = r#"{"thought": "Unicode: 世界 🌍", "confidence": 0.85, "metadata": null}"#;
        let response = ReasoningResponse::from_completion(json);
        assert!(response.thought.contains("世界"));
        assert!(response.thought.contains("🌍"));
        assert_eq!(response.confidence, 0.85);
    }

    // ============================================================================
    // Note: build_messages Tests
    // ============================================================================
    // build_messages is adequately tested through integration tests.
    // Direct unit testing requires complex setup (async runtime, proper Config
    // construction) and is better suited for integration test files.

    // ============================================================================
    // Serialization Edge Cases
    // ============================================================================

    #[test]
    fn test_linear_params_skip_none_session() {
        let params = LinearParams::new("Test");
        let json = serde_json::to_string(&params).unwrap();
        // session_id should not appear in JSON when None
        assert!(!json.contains("session_id"));
    }

    #[test]
    fn test_linear_params_roundtrip() {
        let original = LinearParams::new("Roundtrip test")
            .with_session("sess-rt")
            .with_confidence(0.77);

        let json = serde_json::to_string(&original).unwrap();
        let parsed: LinearParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.content, original.content);
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.confidence, original.confidence);
    }

    #[test]
    fn test_linear_result_roundtrip() {
        let original = LinearResult {
            thought_id: "t-123".to_string(),
            session_id: "s-456".to_string(),
            content: "Test content".to_string(),
            confidence: 0.92,
            previous_thought: Some("t-122".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: LinearResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.thought_id, original.thought_id);
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.content, original.content);
        assert_eq!(parsed.confidence, original.confidence);
        assert_eq!(parsed.previous_thought, original.previous_thought);
    }

    #[test]
    fn test_linear_params_deserialize_with_extra_fields() {
        // Should ignore unknown fields
        let json = r#"{
            "content": "Test",
            "session_id": "s-1",
            "confidence": 0.9,
            "unknown_field": "should be ignored"
        }"#;

        let params: LinearParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, "Test");
        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_linear_result_serialize_with_none_previous() {
        let result = LinearResult {
            thought_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.8,
            previous_thought: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // previous_thought should be null in JSON
        assert_eq!(parsed["previous_thought"], serde_json::Value::Null);
    }

    // ============================================================================
    // Builder Pattern Edge Cases
    // ============================================================================

    #[test]
    fn test_linear_params_multiple_session_overwrites() {
        let params = LinearParams::new("Test")
            .with_session("first")
            .with_session("second")
            .with_session("third");

        assert_eq!(params.session_id, Some("third".to_string()));
    }

    #[test]
    fn test_linear_params_multiple_confidence_overwrites() {
        let params = LinearParams::new("Test")
            .with_confidence(0.5)
            .with_confidence(0.7)
            .with_confidence(0.9);

        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_linear_params_string_types() {
        // Test that Into<String> works for various string types
        let owned = String::from("owned");
        let params1 = LinearParams::new(owned);
        assert_eq!(params1.content, "owned");

        let borrowed = "borrowed";
        let params2 = LinearParams::new(borrowed);
        assert_eq!(params2.content, "borrowed");

        let params3 = LinearParams::new("literal".to_string());
        assert_eq!(params3.content, "literal");
    }

    #[test]
    fn test_linear_params_session_string_types() {
        let params = LinearParams::new("Test")
            .with_session("literal")
            .with_session(String::from("owned"));

        assert_eq!(params.session_id, Some("owned".to_string()));
    }

    // ============================================================================
    // LinearMode Tests
    // ============================================================================

    fn create_test_config() -> Config {
        use crate::config::{
            DatabaseConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig, RequestConfig,
        };
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
            request: RequestConfig::default(),
            pipes: PipeConfig::default(),
        }
    }

    #[test]
    fn test_linear_mode_new() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "linear-reasoning-v1");
    }

    #[test]
    fn test_linear_mode_new_with_custom_pipe() {
        let mut config = create_test_config();
        config.pipes.linear = "custom-linear-pipe".to_string();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "custom-linear-pipe");
    }

    #[test]
    fn test_linear_mode_clone() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);
        let cloned = mode.clone();
        assert_eq!(mode.pipe_name, cloned.pipe_name);
    }

    #[test]
    fn test_build_messages_empty_history() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);
        let messages = mode.build_messages("Test content", &[]);

        // Should have 2 messages: system prompt + user content
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert_eq!(messages[1].content, "Test content");
    }

    #[test]
    fn test_build_messages_with_history() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        // Create mock history thoughts
        let history = vec![
            Thought::new("sess-1", "First thought", "linear"),
            Thought::new("sess-1", "Second thought", "linear"),
        ];

        let messages = mode.build_messages("New content", &history);

        // Should have 3 messages: system prompt + history context + user content
        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert!(messages[1].content.contains("Previous reasoning steps:"));
        assert!(messages[1].content.contains("First thought"));
        assert!(messages[1].content.contains("Second thought"));
        assert!(matches!(messages[2].role, MessageRole::User));
        assert_eq!(messages[2].content, "New content");
    }

    #[test]
    fn test_build_messages_with_single_history() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let history = vec![Thought::new("sess-1", "Only thought", "linear")];

        let messages = mode.build_messages("Content", &history);

        assert_eq!(messages.len(), 3);
        assert!(messages[1].content.contains("Only thought"));
    }

    #[test]
    fn test_build_messages_with_unicode_content() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let unicode_content = "Hello 世界 🌍";
        let messages = mode.build_messages(unicode_content, &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, unicode_content);
    }

    #[test]
    fn test_build_messages_with_multiline_content() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let multiline = "Line 1\nLine 2\nLine 3";
        let messages = mode.build_messages(multiline, &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, multiline);
        assert!(messages[1].content.contains('\n'));
    }

    #[test]
    fn test_build_messages_with_special_characters() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let special = "Test with: \n\t\r\"'\\{}[]()!@#$%^&*";
        let messages = mode.build_messages(special, &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, special);
    }

    #[test]
    fn test_build_messages_history_formatting() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let history = vec![
            Thought::new("sess-1", "Thought A", "linear"),
            Thought::new("sess-1", "Thought B", "linear"),
            Thought::new("sess-1", "Thought C", "linear"),
        ];

        let messages = mode.build_messages("Query", &history);

        // Check that history is formatted correctly with bullets
        let history_msg = &messages[1].content;
        assert!(history_msg.contains("- Thought A"));
        assert!(history_msg.contains("- Thought B"));
        assert!(history_msg.contains("- Thought C"));
        assert!(history_msg.contains("Previous reasoning steps:"));
        assert!(history_msg.contains("Now process this thought:"));
    }

    #[test]
    fn test_build_messages_empty_content() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let messages = mode.build_messages("", &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, "");
    }

    #[test]
    fn test_build_messages_whitespace_only() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let whitespace = "   \t\n  ";
        let messages = mode.build_messages(whitespace, &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, whitespace);
    }

    #[test]
    fn test_build_messages_very_long_content() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let long_content = "x".repeat(10000);
        let messages = mode.build_messages(&long_content, &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content.len(), 10000);
    }

    #[test]
    fn test_build_messages_many_history_items() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let history: Vec<Thought> = (0..50)
            .map(|i| Thought::new("sess-1", &format!("Thought {}", i), "linear"))
            .collect();

        let messages = mode.build_messages("Final query", &history);

        assert_eq!(messages.len(), 3);
        // Verify all thoughts are included
        for i in 0..50 {
            assert!(messages[1].content.contains(&format!("Thought {}", i)));
        }
    }

    #[test]
    fn test_build_messages_history_with_special_chars() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let history = vec![
            Thought::new("sess-1", "Thought with \"quotes\"", "linear"),
            Thought::new("sess-1", "Thought with\nnewlines", "linear"),
        ];

        let messages = mode.build_messages("Query", &history);

        assert_eq!(messages.len(), 3);
        assert!(messages[1].content.contains("\"quotes\""));
        assert!(messages[1].content.contains("newlines"));
    }

    // ============================================================================
    // Clone and Debug Trait Tests
    // ============================================================================

    #[test]
    fn test_linear_params_clone_trait() {
        let original = LinearParams::new("Original")
            .with_session("sess-1")
            .with_confidence(0.85);

        let cloned = original.clone();

        assert_eq!(original.content, cloned.content);
        assert_eq!(original.session_id, cloned.session_id);
        assert_eq!(original.confidence, cloned.confidence);
    }

    #[test]
    fn test_linear_params_debug_trait() {
        let params = LinearParams::new("Debug test")
            .with_session("sess-123")
            .with_confidence(0.9);

        let debug_str = format!("{:?}", params);

        assert!(debug_str.contains("LinearParams"));
        assert!(debug_str.contains("Debug test"));
        assert!(debug_str.contains("sess-123"));
        assert!(debug_str.contains("0.9"));
    }

    #[test]
    fn test_linear_result_clone_trait() {
        let original = LinearResult {
            thought_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.88,
            previous_thought: Some("t-0".to_string()),
        };

        let cloned = original.clone();

        assert_eq!(original.thought_id, cloned.thought_id);
        assert_eq!(original.session_id, cloned.session_id);
        assert_eq!(original.content, cloned.content);
        assert_eq!(original.confidence, cloned.confidence);
        assert_eq!(original.previous_thought, cloned.previous_thought);
    }

    #[test]
    fn test_linear_result_debug_trait() {
        let result = LinearResult {
            thought_id: "t-123".to_string(),
            session_id: "s-456".to_string(),
            content: "Debug result".to_string(),
            confidence: 0.92,
            previous_thought: None,
        };

        let debug_str = format!("{:?}", result);

        assert!(debug_str.contains("LinearResult"));
        assert!(debug_str.contains("t-123"));
        assert!(debug_str.contains("s-456"));
        assert!(debug_str.contains("Debug result"));
    }

    // ============================================================================
    // Message Role Tests (from langbase module)
    // ============================================================================

    #[test]
    fn test_message_roles_in_build_messages() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);
        let messages = mode.build_messages("Test", &[]);

        // First message should be system
        assert!(matches!(messages[0].role, MessageRole::System));
        // Second message should be user
        assert!(matches!(messages[1].role, MessageRole::User));
    }

    #[test]
    fn test_message_roles_with_history() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);
        let history = vec![Thought::new("sess-1", "Previous", "linear")];
        let messages = mode.build_messages("Current", &history);

        // System, history user message, current user message
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert!(matches!(messages[2].role, MessageRole::User));
    }

    // ============================================================================
    // Additional Edge Cases
    // ============================================================================

    #[test]
    fn test_reasoning_response_confidence_bounds() {
        // Test valid confidence values
        let json_low = r#"{"thought": "Low confidence", "confidence": 0.0, "metadata": null}"#;
        let response_low = ReasoningResponse::from_completion(json_low);
        assert_eq!(response_low.confidence, 0.0);

        let json_high = r#"{"thought": "High confidence", "confidence": 1.0, "metadata": null}"#;
        let response_high = ReasoningResponse::from_completion(json_high);
        assert_eq!(response_high.confidence, 1.0);
    }

    #[test]
    fn test_reasoning_response_multiline_thought() {
        let json = r#"{"thought": "Line 1\nLine 2\nLine 3", "confidence": 0.85, "metadata": null}"#;
        let response = ReasoningResponse::from_completion(json);
        assert!(response.thought.contains("Line 1"));
        assert!(response.thought.contains("Line 2"));
        assert!(response.thought.contains("Line 3"));
        assert_eq!(response.confidence, 0.85);
    }

    #[test]
    fn test_reasoning_response_empty_thought() {
        let json = r#"{"thought": "", "confidence": 0.5, "metadata": null}"#;
        let response = ReasoningResponse::from_completion(json);
        assert_eq!(response.thought, "");
        assert_eq!(response.confidence, 0.5);
    }

    #[test]
    fn test_reasoning_response_very_long_thought() {
        let long_thought = "a".repeat(10000);
        let json = format!(
            r#"{{"thought": "{}", "confidence": 0.8, "metadata": null}}"#,
            long_thought
        );
        let response = ReasoningResponse::from_completion(&json);
        assert_eq!(response.thought.len(), 10000);
        assert_eq!(response.confidence, 0.8);
    }

    #[test]
    fn test_default_confidence_function() {
        // Test that the default confidence function is consistent
        assert_eq!(default_confidence(), 0.8);
        assert_eq!(default_confidence(), default_confidence());
    }

    #[test]
    fn test_linear_params_new_from_string() {
        let s = String::from("Test string");
        let params = LinearParams::new(s);
        assert_eq!(params.content, "Test string");
    }

    #[test]
    fn test_linear_params_new_from_str() {
        let params = LinearParams::new("String slice");
        assert_eq!(params.content, "String slice");
    }

    #[test]
    fn test_linear_params_confidence_precision() {
        // Test precise confidence values
        let params1 = LinearParams::new("Test").with_confidence(0.123456789);
        assert_eq!(params1.confidence, 0.123456789);

        let params2 = LinearParams::new("Test").with_confidence(0.999999999);
        assert_eq!(params2.confidence, 0.999999999);
    }

    #[test]
    fn test_linear_result_all_fields() {
        // Test that all fields are properly stored
        let result = LinearResult {
            thought_id: "id-1".to_string(),
            session_id: "sid-2".to_string(),
            content: "Test content".to_string(),
            confidence: 0.777,
            previous_thought: Some("prev-id".to_string()),
        };

        assert_eq!(result.thought_id, "id-1");
        assert_eq!(result.session_id, "sid-2");
        assert_eq!(result.content, "Test content");
        assert_eq!(result.confidence, 0.777);
        assert_eq!(result.previous_thought, Some("prev-id".to_string()));
    }

    #[test]
    fn test_linear_params_session_none_by_default() {
        let params = LinearParams::new("Test");
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_linear_params_confidence_default_value() {
        let params = LinearParams::new("Test");
        assert_eq!(params.confidence, 0.8);
    }
}
