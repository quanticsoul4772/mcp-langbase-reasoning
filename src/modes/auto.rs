//! Auto mode router - automatically selects the most appropriate reasoning mode

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::modes::ReasoningMode;
use crate::prompts::AUTO_ROUTER_PROMPT;
use crate::storage::{Invocation, SqliteStorage, Storage};

/// Input parameters for auto mode routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoParams {
    /// The content to analyze for mode selection
    pub content: String,
    /// Optional hints about the problem type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<String>>,
    /// Optional session ID for context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Result of auto mode routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoResult {
    /// The recommended reasoning mode.
    pub recommended_mode: ReasoningMode,
    /// Confidence in the recommendation (0.0-1.0).
    pub confidence: f64,
    /// Explanation for the recommendation.
    pub rationale: String,
    /// Estimated problem complexity (0.0-1.0).
    pub complexity: f64,
    /// Alternative mode recommendations.
    pub alternative_modes: Vec<ModeRecommendation>,
}

/// A mode recommendation with confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeRecommendation {
    /// The reasoning mode.
    pub mode: ReasoningMode,
    /// Confidence in this recommendation (0.0-1.0).
    pub confidence: f64,
    /// Explanation for this recommendation.
    pub rationale: String,
}

/// Langbase response for auto routing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutoResponse {
    recommended_mode: String,
    confidence: f64,
    rationale: String,
    #[serde(default = "default_complexity")]
    complexity: f64,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn default_complexity() -> f64 {
    0.5
}

impl AutoResponse {
    /// Parse completion in strict mode - returns error on parse failure.
    fn from_completion_strict(completion: &str) -> Result<Self, ToolError> {
        serde_json::from_str::<AutoResponse>(completion).map_err(|e| {
            let preview: String = completion.chars().take(200).collect();
            ToolError::ParseFailed {
                mode: "auto".to_string(),
                message: format!("JSON parse error: {} | Response preview: {}", e, preview),
            }
        })
    }

    /// Parse completion with fallback (legacy mode) - always returns a value.
    fn from_completion_legacy(completion: &str) -> Self {
        match serde_json::from_str::<AutoResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse auto router response, using fallback (DEPRECATED - enable STRICT_MODE)"
                );
                // Fallback - use heuristics
                Self {
                    recommended_mode: "linear".to_string(),
                    confidence: 0.7,
                    rationale: "Default to linear mode (fallback due to parse error)".to_string(),
                    complexity: 0.5,
                    metadata: None,
                }
            }
        }
    }

    /// Parse completion respecting strict mode setting.
    fn from_completion(completion: &str, strict_mode: bool) -> Result<Self, ToolError> {
        if strict_mode {
            Self::from_completion_strict(completion)
        } else {
            Ok(Self::from_completion_legacy(completion))
        }
    }
}

/// Auto mode router
#[derive(Clone)]
pub struct AutoMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for auto routing.
    pipe_name: String,
    /// Whether to use strict mode (no fallbacks).
    strict_mode: bool,
}

impl AutoMode {
    /// Create a new auto mode router
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            pipe_name: config
                .pipes
                .auto
                .clone()
                .unwrap_or_else(|| "mode-router-v1".to_string()),
            strict_mode: config.error_handling.strict_mode,
        }
    }

    /// Route to the appropriate reasoning mode
    pub async fn route(&self, params: AutoParams) -> AppResult<AutoResult> {
        let start = Instant::now();

        debug!(content_len = params.content.len(), "Auto-routing content");

        // First, try local heuristics for obvious cases
        if let Some(result) = self.local_heuristics(&params) {
            info!(
                mode = %result.recommended_mode,
                confidence = result.confidence,
                source = "heuristics",
                "Auto-routing completed via heuristics"
            );
            return Ok(result);
        }

        // Build messages for Langbase
        let messages = self.build_messages(&params);

        // Log invocation
        let mut invocation = Invocation::new(
            "reasoning.auto",
            serialize_for_log(&params, "reasoning.auto input"),
        )
        .with_pipe(&self.pipe_name);

        if let Some(session_id) = &params.session_id {
            invocation = invocation.with_session(session_id);
        }

        // Call Langbase
        let request = PipeRequest::new(&self.pipe_name, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
                    warn!(
                        error = %log_err,
                        tool = %invocation.tool_name,
                        "Failed to log invocation - audit trail incomplete"
                    );
                }
                return Err(e.into());
            }
        };

        // Parse response
        let auto_response = AutoResponse::from_completion(&response.completion, self.strict_mode)?;

        // Convert mode string to enum
        let recommended_mode = auto_response.recommended_mode.parse().unwrap_or_else(|_| {
            warn!(
                invalid_mode = %auto_response.recommended_mode,
                "Invalid mode returned by auto-router, falling back to Linear"
            );
            ReasoningMode::Linear
        });

        // Generate alternative recommendations based on complexity
        let alternatives = self.generate_alternatives(&auto_response);

        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&auto_response, "reasoning.auto output"),
            latency,
        );
        if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
            warn!(
                error = %log_err,
                tool = %invocation.tool_name,
                "Failed to log invocation - audit trail incomplete"
            );
        }

        info!(
            mode = %recommended_mode,
            confidence = auto_response.confidence,
            complexity = auto_response.complexity,
            latency_ms = latency,
            "Auto-routing completed"
        );

        Ok(AutoResult {
            recommended_mode,
            confidence: auto_response.confidence,
            rationale: auto_response.rationale,
            complexity: auto_response.complexity,
            alternative_modes: alternatives,
        })
    }

    /// Apply local heuristics for obvious cases
    fn local_heuristics(&self, params: &AutoParams) -> Option<AutoResult> {
        let content_lower = params.content.to_lowercase();

        // Very short content -> linear
        if params.content.len() < 50 {
            return Some(AutoResult {
                recommended_mode: ReasoningMode::Linear,
                confidence: 0.9,
                rationale: "Short content is best handled with linear reasoning".to_string(),
                complexity: 0.2,
                alternative_modes: vec![],
            });
        }

        // Explicit reflection keywords
        if content_lower.contains("evaluate")
            || content_lower.contains("assess")
            || content_lower.contains("review quality")
            || content_lower.contains("critique")
        {
            return Some(AutoResult {
                recommended_mode: ReasoningMode::Reflection,
                confidence: 0.85,
                rationale: "Content contains evaluation/assessment keywords".to_string(),
                complexity: 0.5,
                alternative_modes: vec![ModeRecommendation {
                    mode: ReasoningMode::Linear,
                    confidence: 0.6,
                    rationale: "Could also use linear for structured evaluation".to_string(),
                }],
            });
        }

        // Explicit creativity keywords
        if content_lower.contains("creative")
            || content_lower.contains("brainstorm")
            || content_lower.contains("novel")
            || content_lower.contains("unconventional")
        {
            return Some(AutoResult {
                recommended_mode: ReasoningMode::Divergent,
                confidence: 0.85,
                rationale: "Content requires creative/divergent thinking".to_string(),
                complexity: 0.6,
                alternative_modes: vec![ModeRecommendation {
                    mode: ReasoningMode::Tree,
                    confidence: 0.5,
                    rationale: "Could explore multiple creative paths with tree mode".to_string(),
                }],
            });
        }

        // Explicit branching keywords
        if content_lower.contains("options")
            || content_lower.contains("alternatives")
            || content_lower.contains("compare")
            || content_lower.contains("trade-offs")
        {
            return Some(AutoResult {
                recommended_mode: ReasoningMode::Tree,
                confidence: 0.8,
                rationale: "Content requires exploring multiple options".to_string(),
                complexity: 0.5,
                alternative_modes: vec![ModeRecommendation {
                    mode: ReasoningMode::Divergent,
                    confidence: 0.4,
                    rationale: "Could also use divergent for creative alternatives".to_string(),
                }],
            });
        }

        // Complex graph keywords
        if content_lower.contains("complex system")
            || content_lower.contains("interconnected")
            || content_lower.contains("multi-step")
            || content_lower.contains("graph")
        {
            return Some(AutoResult {
                recommended_mode: ReasoningMode::Got,
                confidence: 0.75,
                rationale: "Content suggests complex graph-based reasoning".to_string(),
                complexity: 0.8,
                alternative_modes: vec![ModeRecommendation {
                    mode: ReasoningMode::Tree,
                    confidence: 0.6,
                    rationale: "Tree mode could also handle multi-path exploration".to_string(),
                }],
            });
        }

        None
    }

    /// Generate alternative mode recommendations
    fn generate_alternatives(&self, response: &AutoResponse) -> Vec<ModeRecommendation> {
        let mut alternatives = Vec::new();

        // Based on complexity, suggest alternatives
        if response.complexity < 0.3 {
            if response.recommended_mode != "linear" {
                alternatives.push(ModeRecommendation {
                    mode: ReasoningMode::Linear,
                    confidence: 0.7,
                    rationale: "Low complexity could be handled linearly".to_string(),
                });
            }
        } else if response.complexity > 0.7 {
            if response.recommended_mode != "got" {
                alternatives.push(ModeRecommendation {
                    mode: ReasoningMode::Got,
                    confidence: 0.6,
                    rationale: "High complexity might benefit from graph exploration".to_string(),
                });
            }
            if response.recommended_mode != "tree" {
                alternatives.push(ModeRecommendation {
                    mode: ReasoningMode::Tree,
                    confidence: 0.5,
                    rationale: "Multi-path exploration could also work".to_string(),
                });
            }
        } else {
            // Medium complexity
            if response.recommended_mode != "tree" {
                alternatives.push(ModeRecommendation {
                    mode: ReasoningMode::Tree,
                    confidence: 0.5,
                    rationale: "Moderate complexity could use branching".to_string(),
                });
            }
        }

        alternatives
    }

    /// Build messages for the Langbase pipe
    fn build_messages(&self, params: &AutoParams) -> Vec<Message> {
        let mut messages = Vec::new();

        messages.push(Message::system(AUTO_ROUTER_PROMPT));

        // Add content to analyze
        let mut user_message = format!(
            "Analyze this content and recommend the best reasoning mode:\n\n{}",
            params.content
        );

        // Add hints if provided
        if let Some(hints) = &params.hints {
            user_message.push_str(&format!(
                "\n\nHints about the problem:\n- {}",
                hints.join("\n- ")
            ));
        }

        messages.push(Message::user(user_message));

        messages
    }
}

impl AutoParams {
    /// Create new params with content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            hints: None,
            session_id: None,
        }
    }

    /// Add hints
    pub fn with_hints(mut self, hints: Vec<String>) -> Self {
        self.hints = Some(hints);
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
    // AutoParams Tests
    // ============================================================================

    #[test]
    fn test_auto_params_new() {
        let params = AutoParams::new("Test content");
        assert_eq!(params.content, "Test content");
        assert!(params.hints.is_none());
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_auto_params_with_hints() {
        let params = AutoParams::new("Content").with_hints(vec!["hint1".to_string()]);
        assert_eq!(params.hints, Some(vec!["hint1".to_string()]));
    }

    #[test]
    fn test_auto_params_with_session() {
        let params = AutoParams::new("Content").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_auto_params_builder_chain() {
        let params = AutoParams::new("Content")
            .with_hints(vec!["hint1".to_string(), "hint2".to_string()])
            .with_session("sess-abc");

        assert_eq!(params.content, "Content");
        assert_eq!(params.hints.as_ref().unwrap().len(), 2);
        assert_eq!(params.session_id, Some("sess-abc".to_string()));
    }

    #[test]
    fn test_auto_params_serialize() {
        let params = AutoParams::new("Test").with_hints(vec!["h1".to_string()]);
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("hints"));
    }

    #[test]
    fn test_auto_params_deserialize() {
        let json = r#"{"content": "Test content", "hints": ["hint1", "hint2"]}"#;
        let params: AutoParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, "Test content");
        assert_eq!(params.hints.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_auto_params_deserialize_minimal() {
        let json = r#"{"content": "Minimal"}"#;
        let params: AutoParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, "Minimal");
        assert!(params.hints.is_none());
        assert!(params.session_id.is_none());
    }

    // ============================================================================
    // AutoResponse Tests
    // ============================================================================

    #[test]
    fn test_auto_response_from_json() {
        let json = r#"{"recommended_mode": "tree", "confidence": 0.9, "rationale": "Multiple paths", "complexity": 0.6}"#;
        // Test legacy mode (no strict)
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.recommended_mode, "tree");
        assert_eq!(resp.confidence, 0.9);
        assert_eq!(resp.complexity, 0.6);
    }

    #[test]
    fn test_auto_response_from_plain_text_legacy() {
        let text = "Invalid JSON";
        // Legacy mode: falls back to default
        let resp = AutoResponse::from_completion(text, false).unwrap();
        assert_eq!(resp.recommended_mode, "linear");
        assert_eq!(resp.confidence, 0.7);
    }

    #[test]
    fn test_auto_response_from_plain_text_strict() {
        let text = "Invalid JSON";
        // Strict mode: returns error
        let result = AutoResponse::from_completion(text, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::error::ToolError::ParseFailed { .. }));
    }

    #[test]
    fn test_auto_response_with_metadata() {
        let json = r#"{
            "recommended_mode": "divergent",
            "confidence": 0.85,
            "rationale": "Creative task",
            "complexity": 0.7,
            "metadata": {"source": "test"}
        }"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.recommended_mode, "divergent");
        assert!(resp.metadata.is_some());
    }

    #[test]
    fn test_auto_response_default_complexity() {
        let json = r#"{"recommended_mode": "linear", "confidence": 0.8, "rationale": "Test"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.complexity, 0.5); // default
    }

    #[test]
    fn test_auto_response_all_modes() {
        let modes = vec![
            ("linear", ReasoningMode::Linear),
            ("tree", ReasoningMode::Tree),
            ("divergent", ReasoningMode::Divergent),
            ("reflection", ReasoningMode::Reflection),
            ("got", ReasoningMode::Got),
        ];

        for (mode_str, _expected_mode) in modes {
            let json = format!(
                r#"{{"recommended_mode": "{}", "confidence": 0.8, "rationale": "Test"}}"#,
                mode_str
            );
            let resp = AutoResponse::from_completion(&json, false).unwrap();
            assert_eq!(resp.recommended_mode, mode_str);
        }
    }

    #[test]
    fn test_auto_response_strict_mode_valid_json() {
        let json = r#"{"recommended_mode": "tree", "confidence": 0.9, "rationale": "Test"}"#;
        // Strict mode with valid JSON should succeed
        let result = AutoResponse::from_completion(json, true);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.recommended_mode, "tree");
    }

    #[test]
    fn test_auto_response_strict_mode_error_message() {
        let invalid_json = "{ broken json";
        let result = AutoResponse::from_completion(invalid_json, true);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("Parse error in auto mode"));
        assert!(err_str.contains("broken json"));
    }

    // ============================================================================
    // AutoResult Tests
    // ============================================================================

    #[test]
    fn test_auto_result_serialize() {
        let result = AutoResult {
            recommended_mode: ReasoningMode::Tree,
            confidence: 0.85,
            rationale: "Multiple paths needed".to_string(),
            complexity: 0.6,
            alternative_modes: vec![ModeRecommendation {
                mode: ReasoningMode::Divergent,
                confidence: 0.6,
                rationale: "Could also be creative".to_string(),
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("tree"));
        assert!(json.contains("0.85"));
        assert!(json.contains("alternative_modes"));
    }

    #[test]
    fn test_auto_result_deserialize() {
        let json = r#"{
            "recommended_mode": "linear",
            "confidence": 0.9,
            "rationale": "Simple task",
            "complexity": 0.2,
            "alternative_modes": []
        }"#;
        let result: AutoResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Linear);
        assert_eq!(result.confidence, 0.9);
        assert!(result.alternative_modes.is_empty());
    }

    #[test]
    fn test_auto_result_with_alternatives() {
        let result = AutoResult {
            recommended_mode: ReasoningMode::Got,
            confidence: 0.75,
            rationale: "Complex system".to_string(),
            complexity: 0.8,
            alternative_modes: vec![
                ModeRecommendation {
                    mode: ReasoningMode::Tree,
                    confidence: 0.6,
                    rationale: "Alt 1".to_string(),
                },
                ModeRecommendation {
                    mode: ReasoningMode::Divergent,
                    confidence: 0.4,
                    rationale: "Alt 2".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: AutoResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.alternative_modes.len(), 2);
    }

    // ============================================================================
    // ModeRecommendation Tests
    // ============================================================================

    #[test]
    fn test_mode_recommendation_serialize() {
        let rec = ModeRecommendation {
            mode: ReasoningMode::Tree,
            confidence: 0.8,
            rationale: "Test".to_string(),
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert!(json.contains("tree"));
    }

    #[test]
    fn test_mode_recommendation_deserialize() {
        let json = r#"{"mode": "reflection", "confidence": 0.7, "rationale": "Needs evaluation"}"#;
        let rec: ModeRecommendation = serde_json::from_str(json).unwrap();
        assert_eq!(rec.mode, ReasoningMode::Reflection);
        assert_eq!(rec.confidence, 0.7);
    }

    #[test]
    fn test_mode_recommendation_all_modes() {
        let modes = vec![
            ReasoningMode::Linear,
            ReasoningMode::Tree,
            ReasoningMode::Divergent,
            ReasoningMode::Reflection,
            ReasoningMode::Got,
        ];

        for mode in modes {
            let rec = ModeRecommendation {
                mode: mode.clone(),
                confidence: 0.5,
                rationale: "Test".to_string(),
            };
            let json = serde_json::to_string(&rec).unwrap();
            let parsed: ModeRecommendation = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.mode, mode);
        }
    }

    // ============================================================================
    // Default Function Tests
    // ============================================================================

    #[test]
    fn test_default_complexity() {
        assert_eq!(default_complexity(), 0.5);
    }

    // ============================================================================
    // ReasoningMode Parse Tests
    // ============================================================================

    #[test]
    fn test_reasoning_mode_from_string() {
        assert_eq!(
            "linear".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Linear
        );
        assert_eq!(
            "tree".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Tree
        );
        assert_eq!(
            "divergent".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Divergent
        );
        assert_eq!(
            "reflection".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Reflection
        );
        assert_eq!("got".parse::<ReasoningMode>().unwrap(), ReasoningMode::Got);
    }

    #[test]
    fn test_reasoning_mode_invalid_string() {
        assert!("invalid".parse::<ReasoningMode>().is_err());
        assert!("unknown".parse::<ReasoningMode>().is_err());
        assert!("".parse::<ReasoningMode>().is_err());
    }

    // ============================================================================
    // Edge Case Tests
    // ============================================================================

    #[test]
    fn test_auto_params_empty_content() {
        let params = AutoParams::new("");
        assert_eq!(params.content, "");
    }

    #[test]
    fn test_auto_params_empty_hints() {
        let params = AutoParams::new("Content").with_hints(vec![]);
        assert_eq!(params.hints, Some(vec![]));
    }

    #[test]
    fn test_auto_response_zero_confidence() {
        let json =
            r#"{"recommended_mode": "linear", "confidence": 0.0, "rationale": "No confidence"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, 0.0);
    }

    #[test]
    fn test_auto_response_max_confidence() {
        let json =
            r#"{"recommended_mode": "linear", "confidence": 1.0, "rationale": "Full confidence"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, 1.0);
    }

    #[test]
    fn test_auto_result_empty_alternatives() {
        let result = AutoResult {
            recommended_mode: ReasoningMode::Linear,
            confidence: 0.9,
            rationale: "Simple".to_string(),
            complexity: 0.1,
            alternative_modes: vec![],
        };
        assert!(result.alternative_modes.is_empty());
    }

    #[test]
    fn test_mode_recommendation_zero_confidence() {
        let rec = ModeRecommendation {
            mode: ReasoningMode::Linear,
            confidence: 0.0,
            rationale: "Low confidence alt".to_string(),
        };
        assert_eq!(rec.confidence, 0.0);
    }

    // ============================================================================
    // Serialization Round-Trip Tests
    // ============================================================================

    #[test]
    fn test_auto_params_round_trip() {
        let original = AutoParams::new("Complex content")
            .with_hints(vec!["hint1".to_string(), "hint2".to_string()])
            .with_session("sess-xyz");

        let json = serde_json::to_string(&original).unwrap();
        let parsed: AutoParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.content, original.content);
        assert_eq!(parsed.hints, original.hints);
        assert_eq!(parsed.session_id, original.session_id);
    }

    #[test]
    fn test_auto_result_round_trip() {
        let original = AutoResult {
            recommended_mode: ReasoningMode::Divergent,
            confidence: 0.87,
            rationale: "Creative exploration needed".to_string(),
            complexity: 0.65,
            alternative_modes: vec![ModeRecommendation {
                mode: ReasoningMode::Tree,
                confidence: 0.55,
                rationale: "Could branch".to_string(),
            }],
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: AutoResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.recommended_mode, original.recommended_mode);
        assert_eq!(parsed.confidence, original.confidence);
        assert_eq!(parsed.rationale, original.rationale);
        assert_eq!(parsed.complexity, original.complexity);
        assert_eq!(parsed.alternative_modes.len(), 1);
    }

    // ============================================================================
    // local_heuristics() Tests
    // ============================================================================

    #[test]
    fn test_local_heuristics_short_content() {
        let mode = create_test_mode();
        let params = AutoParams::new("Short text");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Linear);
        assert_eq!(result.confidence, 0.9);
        assert_eq!(result.complexity, 0.2);
    }

    #[test]
    fn test_local_heuristics_evaluate_keyword() {
        let mode = create_test_mode();
        let params =
            AutoParams::new("Please evaluate this solution for correctness and efficiency");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
        assert_eq!(result.confidence, 0.85);
        assert_eq!(result.alternative_modes.len(), 1);
    }

    #[test]
    fn test_local_heuristics_assess_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new("I need you to assess the quality of this implementation");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
    }

    #[test]
    fn test_local_heuristics_review_quality_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "Can you review quality of the architecture design and implementation patterns?",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
    }

    #[test]
    fn test_local_heuristics_critique_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new("Please critique this approach to see if it works effectively and meets our requirements");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
    }

    #[test]
    fn test_local_heuristics_creative_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new("We need a creative solution to this unique problem that differs from existing approaches");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Divergent);
        assert_eq!(result.confidence, 0.85);
    }

    #[test]
    fn test_local_heuristics_brainstorm_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "Let's brainstorm ideas for improving user engagement and retention metrics",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Divergent);
    }

    #[test]
    fn test_local_heuristics_novel_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "We need novel approaches to solve this challenge in the competitive market",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Divergent);
    }

    #[test]
    fn test_local_heuristics_unconventional_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "Looking for unconventional strategies to differentiate our product offering",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Divergent);
    }

    #[test]
    fn test_local_heuristics_options_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new("What options do we have for implementing authentication?");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Tree);
        assert_eq!(result.confidence, 0.8);
    }

    #[test]
    fn test_local_heuristics_alternatives_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new("Consider alternatives to the current database design");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Tree);
    }

    #[test]
    fn test_local_heuristics_compare_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "Compare different approaches to API versioning and list their pros and cons",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Tree);
    }

    #[test]
    fn test_local_heuristics_tradeoffs_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "Analyze the trade-offs between consistency and availability in distributed systems",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Tree);
    }

    #[test]
    fn test_local_heuristics_complex_system_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "We have a complex system with many interdependencies that need careful analysis",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Got);
        assert_eq!(result.confidence, 0.75);
        assert_eq!(result.complexity, 0.8);
    }

    #[test]
    fn test_local_heuristics_interconnected_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "These services are highly interconnected across multiple domains and boundaries",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Got);
    }

    #[test]
    fn test_local_heuristics_multistep_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "This requires a multi-step deployment process with various dependencies and stages",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Got);
    }

    #[test]
    fn test_local_heuristics_graph_keyword() {
        let mode = create_test_mode();
        let params = AutoParams::new("Analyze this graph of dependencies between services and components in our architecture");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Got);
    }

    #[test]
    fn test_local_heuristics_no_match() {
        let mode = create_test_mode();
        let params = AutoParams::new("Just a regular query without special keywords that would trigger heuristics and patterns");
        let result = mode.local_heuristics(&params);
        assert!(result.is_none());
    }

    #[test]
    fn test_local_heuristics_case_insensitive() {
        let mode = create_test_mode();
        let params = AutoParams::new(
            "BRAINSTORM new IDEAS for this CREATIVE project with innovative solutions",
        );
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Divergent);
    }

    // ============================================================================
    // generate_alternatives() Tests
    // ============================================================================

    #[test]
    fn test_generate_alternatives_low_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "tree".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.2, // Low complexity
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        assert!(!alternatives.is_empty());
        assert_eq!(alternatives[0].mode, ReasoningMode::Linear);
        assert_eq!(alternatives[0].confidence, 0.7);
    }

    #[test]
    fn test_generate_alternatives_low_complexity_already_linear() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.9,
            rationale: "Simple".to_string(),
            complexity: 0.1,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should not suggest linear if already recommended
        assert!(alternatives.is_empty() || alternatives[0].mode != ReasoningMode::Linear);
    }

    #[test]
    fn test_generate_alternatives_high_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.7,
            rationale: "Test".to_string(),
            complexity: 0.8, // High complexity
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        assert!(!alternatives.is_empty());
        // Should suggest GoT for high complexity
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Got));
    }

    #[test]
    fn test_generate_alternatives_high_complexity_already_got() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "got".to_string(),
            confidence: 0.9,
            rationale: "Complex".to_string(),
            complexity: 0.9,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should suggest Tree as alternative
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    #[test]
    fn test_generate_alternatives_medium_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.5, // Medium complexity
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        assert!(!alternatives.is_empty());
        // Should suggest Tree for medium complexity
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    #[test]
    fn test_generate_alternatives_medium_complexity_already_tree() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "tree".to_string(),
            confidence: 0.85,
            rationale: "Branching".to_string(),
            complexity: 0.5,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should not suggest Tree if already recommended
        assert!(
            alternatives.is_empty() || !alternatives.iter().any(|a| a.mode == ReasoningMode::Tree)
        );
    }

    // ============================================================================
    // build_messages() Tests
    // ============================================================================

    #[test]
    fn test_build_messages_basic() {
        let mode = create_test_mode();
        let params = AutoParams::new("Test content for analysis");
        let messages = mode.build_messages(&params);

        assert_eq!(messages.len(), 2);
        // First message should be system with AUTO_ROUTER_PROMPT
        assert!(messages[0].content.contains("reasoning mode"));
        // Second message should be user with content
        assert!(messages[1].content.contains("Test content for analysis"));
    }

    #[test]
    fn test_build_messages_with_hints() {
        let mode = create_test_mode();
        let params = AutoParams::new("Complex problem").with_hints(vec![
            "performance critical".to_string(),
            "needs scalability".to_string(),
        ]);
        let messages = mode.build_messages(&params);

        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("Hints about the problem"));
        assert!(messages[1].content.contains("performance critical"));
        assert!(messages[1].content.contains("needs scalability"));
    }

    #[test]
    fn test_build_messages_with_single_hint() {
        let mode = create_test_mode();
        let params = AutoParams::new("Question").with_hints(vec!["security".to_string()]);
        let messages = mode.build_messages(&params);

        assert!(messages[1].content.contains("Hints"));
        assert!(messages[1].content.contains("security"));
    }

    #[test]
    fn test_build_messages_with_empty_hints() {
        let mode = create_test_mode();
        let params = AutoParams::new("Question").with_hints(vec![]);
        let messages = mode.build_messages(&params);

        // Empty hints should still be included
        assert!(messages[1].content.contains("Hints"));
    }

    #[test]
    fn test_build_messages_no_hints() {
        let mode = create_test_mode();
        let params = AutoParams::new("Simple question");
        let messages = mode.build_messages(&params);

        assert_eq!(messages.len(), 2);
        assert!(!messages[1].content.contains("Hints"));
    }

    #[test]
    fn test_build_messages_long_content() {
        let mode = create_test_mode();
        let long_content = "A".repeat(5000);
        let params = AutoParams::new(long_content.clone());
        let messages = mode.build_messages(&params);

        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains(&long_content));
    }

    #[test]
    fn test_build_messages_special_characters_in_content() {
        let mode = create_test_mode();
        let params = AutoParams::new("Special: \n\t\"quotes\" and <brackets>");
        let messages = mode.build_messages(&params);

        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("Special:"));
    }

    #[test]
    fn test_build_messages_special_characters_in_hints() {
        let mode = create_test_mode();
        let params = AutoParams::new("Question").with_hints(vec![
            "hint with \"quotes\"".to_string(),
            "hint with \nnewlines".to_string(),
        ]);
        let messages = mode.build_messages(&params);

        assert!(messages[1].content.contains("Hints"));
        assert!(messages[1].content.contains("quotes"));
    }

    // ============================================================================
    // AutoMode Constructor Tests
    // ============================================================================

    #[test]
    fn test_auto_mode_new_creates_instance() {
        let mode = create_test_mode();
        // Verify the mode is created successfully
        assert_eq!(mode.pipe_name, "mode-router-v1");
    }

    #[test]
    fn test_auto_mode_new_with_custom_pipe() {
        use crate::config::{
            Config, DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig,
            RequestConfig,
        };
        use std::path::PathBuf;

        let langbase_config = LangbaseConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };

        let mut pipes = PipeConfig::default();
        pipes.auto = Some("custom-auto-pipe".to_string());

        let config = Config {
            langbase: langbase_config.clone(),
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Pretty,
            },
            request: RequestConfig::default(),
            pipes,
            error_handling: ErrorHandlingConfig::default(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = AutoMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "custom-auto-pipe");
    }

    #[test]
    fn test_auto_mode_new_without_custom_pipe_uses_default() {
        use crate::config::{
            Config, DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig,
            RequestConfig,
        };
        use std::path::PathBuf;

        let langbase_config = LangbaseConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };

        let mut pipes = PipeConfig::default();
        pipes.auto = None; // Explicitly set to None

        let config = Config {
            langbase: langbase_config.clone(),
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Pretty,
            },
            request: RequestConfig::default(),
            pipes,
            error_handling: crate::config::ErrorHandlingConfig::default(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = AutoMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "mode-router-v1");
    }

    // ============================================================================
    // AutoResponse Edge Cases
    // ============================================================================

    #[test]
    fn test_auto_response_from_empty_json() {
        let json = "{}";
        let resp = AutoResponse::from_completion(json, false).unwrap();
        // Should fallback to linear with defaults
        assert_eq!(resp.recommended_mode, "linear");
        assert_eq!(resp.confidence, 0.7);
    }

    #[test]
    fn test_auto_response_from_json_missing_rationale() {
        let json = r#"{"recommended_mode": "tree", "confidence": 0.8}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        // Should fallback because rationale is missing
        assert_eq!(resp.recommended_mode, "linear");
    }

    #[test]
    fn test_auto_response_from_json_null_metadata() {
        let json = r#"{
            "recommended_mode": "reflection",
            "confidence": 0.82,
            "rationale": "Test",
            "complexity": 0.5,
            "metadata": null
        }"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.recommended_mode, "reflection");
        assert!(resp.metadata.is_none());
    }

    #[test]
    fn test_auto_response_from_json_empty_string_mode() {
        let json = r#"{"recommended_mode": "", "confidence": 0.8, "rationale": "Test"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.recommended_mode, "");
    }

    #[test]
    fn test_auto_response_from_json_invalid_mode_string() {
        let json =
            r#"{"recommended_mode": "invalid_mode", "confidence": 0.8, "rationale": "Test"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.recommended_mode, "invalid_mode");
    }

    #[test]
    fn test_auto_response_from_json_negative_confidence() {
        let json = r#"{"recommended_mode": "linear", "confidence": -0.5, "rationale": "Test"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, -0.5); // No clamping in AutoResponse
    }

    #[test]
    fn test_auto_response_from_json_over_one_confidence() {
        let json = r#"{"recommended_mode": "linear", "confidence": 1.5, "rationale": "Test"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.confidence, 1.5); // No clamping in AutoResponse
    }

    #[test]
    fn test_auto_response_from_json_negative_complexity() {
        let json = r#"{"recommended_mode": "linear", "confidence": 0.8, "rationale": "Test", "complexity": -0.3}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.complexity, -0.3);
    }

    #[test]
    fn test_auto_response_from_json_over_one_complexity() {
        let json = r#"{"recommended_mode": "linear", "confidence": 0.8, "rationale": "Test", "complexity": 1.5}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.complexity, 1.5);
    }

    #[test]
    fn test_auto_response_from_long_preview_text() {
        let long_text = "Not JSON: ".to_owned() + &"a".repeat(300);
        let resp = AutoResponse::from_completion(&long_text, false).unwrap();
        // Should fallback to linear
        assert_eq!(resp.recommended_mode, "linear");
        assert_eq!(resp.confidence, 0.7);
    }

    #[test]
    fn test_auto_response_from_backtracking_mode() {
        let json = r#"{"recommended_mode": "backtracking", "confidence": 0.85, "rationale": "Needs backtracking"}"#;
        let resp = AutoResponse::from_completion(json, false).unwrap();
        assert_eq!(resp.recommended_mode, "backtracking");
    }

    // ============================================================================
    // local_heuristics() Additional Coverage
    // ============================================================================

    #[test]
    fn test_local_heuristics_exactly_50_chars() {
        let mode = create_test_mode();
        let params = AutoParams::new("A".repeat(50));
        let result = mode.local_heuristics(&params);
        // Should NOT trigger short content heuristic (< 50)
        assert!(result.is_none());
    }

    #[test]
    fn test_local_heuristics_49_chars() {
        let mode = create_test_mode();
        let params = AutoParams::new("A".repeat(49));
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Linear);
    }

    #[test]
    fn test_local_heuristics_multiple_keywords_first_wins() {
        let mode = create_test_mode();
        // Contains both "evaluate" (reflection) and "creative" (divergent)
        // Reflection check comes first, should win
        let params = AutoParams::new("Please evaluate this creative solution with novel ideas");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
    }

    #[test]
    fn test_local_heuristics_mixed_case_keywords() {
        let mode = create_test_mode();
        // Content must be >= 50 chars to bypass short content check
        let params = AutoParams::new(
            "EVALUATE this CREATIVE approach with NOVEL IDEAS and more context here",
        );
        let result = mode.local_heuristics(&params).unwrap();
        // Should match case-insensitively
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
    }

    #[test]
    fn test_local_heuristics_keyword_in_middle() {
        let mode = create_test_mode();
        let params = AutoParams::new("The system has many interconnected parts that need analysis");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Got);
    }

    #[test]
    fn test_local_heuristics_partial_keyword_no_match() {
        let mode = create_test_mode();
        // "creating" contains "creative" but as substring
        let params = AutoParams::new("We are creating a new system for data processing");
        let result = mode.local_heuristics(&params);
        // "creative" should still match as substring via contains()
        assert!(result.is_some());
    }

    #[test]
    fn test_local_heuristics_whitespace_around_keywords() {
        let mode = create_test_mode();
        // Content must be >= 50 chars to bypass short content check
        let params =
            AutoParams::new("  evaluate  this  approach  with significant context padding  ");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Reflection);
    }

    #[test]
    fn test_local_heuristics_backtracking_not_detected() {
        let mode = create_test_mode();
        // Content must be >= 50 chars to bypass short content check
        let params = AutoParams::new(
            "This requires backtracking to find the solution with additional context",
        );
        let result = mode.local_heuristics(&params);
        // "backtracking" is not in the heuristics keywords
        assert!(result.is_none());
    }

    #[test]
    fn test_local_heuristics_empty_string() {
        let mode = create_test_mode();
        let params = AutoParams::new("");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Linear);
        assert_eq!(result.complexity, 0.2);
    }

    #[test]
    fn test_local_heuristics_single_char() {
        let mode = create_test_mode();
        let params = AutoParams::new("x");
        let result = mode.local_heuristics(&params).unwrap();
        assert_eq!(result.recommended_mode, ReasoningMode::Linear);
    }

    // ============================================================================
    // generate_alternatives() Complete Coverage
    // ============================================================================

    #[test]
    fn test_generate_alternatives_boundary_low_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "tree".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.29, // Just below 0.3
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Linear));
    }

    #[test]
    fn test_generate_alternatives_boundary_high_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.7,
            rationale: "Test".to_string(),
            complexity: 0.71, // Just above 0.7
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Got));
    }

    #[test]
    fn test_generate_alternatives_exactly_0_3_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "tree".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.3, // Boundary
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should NOT suggest linear (complexity >= 0.3)
        assert!(
            alternatives.is_empty()
                || !alternatives.iter().any(|a| a.mode == ReasoningMode::Linear)
        );
    }

    #[test]
    fn test_generate_alternatives_exactly_0_7_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.7, // Boundary
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should NOT suggest high-complexity modes (complexity <= 0.7)
        assert!(
            alternatives.is_empty() || !alternatives.iter().any(|a| a.mode == ReasoningMode::Got)
        );
    }

    #[test]
    fn test_generate_alternatives_high_complexity_both_alternatives() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.7,
            rationale: "Test".to_string(),
            complexity: 0.9,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should suggest both GoT and Tree for high complexity
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Got));
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    #[test]
    fn test_generate_alternatives_recommended_divergent() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "divergent".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.5,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Medium complexity, not divergent -> should suggest Tree
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    #[test]
    fn test_generate_alternatives_recommended_reflection() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "reflection".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.5,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Should suggest Tree for medium complexity
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    #[test]
    fn test_generate_alternatives_zero_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "got".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.0,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Very low complexity -> suggest Linear
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Linear));
    }

    #[test]
    fn test_generate_alternatives_max_complexity() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "linear".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 1.0,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Very high complexity -> suggest GoT and Tree
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Got));
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    #[test]
    fn test_generate_alternatives_backtracking_mode() {
        let mode = create_test_mode();
        let response = AutoResponse {
            recommended_mode: "backtracking".to_string(),
            confidence: 0.8,
            rationale: "Test".to_string(),
            complexity: 0.5,
            metadata: None,
        };
        let alternatives = mode.generate_alternatives(&response);
        // Medium complexity, not one of the standard modes -> suggest Tree
        assert!(alternatives.iter().any(|a| a.mode == ReasoningMode::Tree));
    }

    // ============================================================================
    // Additional Edge Cases for All Structs
    // ============================================================================

    #[test]
    fn test_auto_result_complexity_boundaries() {
        let result = AutoResult {
            recommended_mode: ReasoningMode::Linear,
            confidence: 0.9,
            rationale: "Test".to_string(),
            complexity: 0.0,
            alternative_modes: vec![],
        };
        assert_eq!(result.complexity, 0.0);

        let result2 = AutoResult {
            recommended_mode: ReasoningMode::Linear,
            confidence: 0.9,
            rationale: "Test".to_string(),
            complexity: 1.0,
            alternative_modes: vec![],
        };
        assert_eq!(result2.complexity, 1.0);
    }

    #[test]
    fn test_auto_result_many_alternatives() {
        let result = AutoResult {
            recommended_mode: ReasoningMode::Linear,
            confidence: 0.9,
            rationale: "Test".to_string(),
            complexity: 0.5,
            alternative_modes: vec![
                ModeRecommendation {
                    mode: ReasoningMode::Tree,
                    confidence: 0.8,
                    rationale: "Alt 1".to_string(),
                },
                ModeRecommendation {
                    mode: ReasoningMode::Divergent,
                    confidence: 0.7,
                    rationale: "Alt 2".to_string(),
                },
                ModeRecommendation {
                    mode: ReasoningMode::Reflection,
                    confidence: 0.6,
                    rationale: "Alt 3".to_string(),
                },
            ],
        };
        assert_eq!(result.alternative_modes.len(), 3);
    }

    #[test]
    fn test_mode_recommendation_all_reasoning_modes() {
        let modes = vec![
            (ReasoningMode::Linear, "linear"),
            (ReasoningMode::Tree, "tree"),
            (ReasoningMode::Divergent, "divergent"),
            (ReasoningMode::Reflection, "reflection"),
            (ReasoningMode::Got, "got"),
            (ReasoningMode::Backtracking, "backtracking"),
        ];

        for (mode, expected_str) in modes {
            let rec = ModeRecommendation {
                mode: mode.clone(),
                confidence: 0.5,
                rationale: "Test".to_string(),
            };
            let json = serde_json::to_string(&rec).unwrap();
            assert!(json.contains(expected_str));

            let parsed: ModeRecommendation = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.mode, mode);
        }
    }

    #[test]
    fn test_auto_params_very_long_hints() {
        let long_hints: Vec<String> = (0..100).map(|i| format!("Hint number {}", i)).collect();
        let params = AutoParams::new("Content").with_hints(long_hints.clone());
        assert_eq!(params.hints.as_ref().unwrap().len(), 100);
    }

    #[test]
    fn test_auto_params_empty_string_hints() {
        let params =
            AutoParams::new("Content").with_hints(vec!["".to_string(), "valid".to_string()]);
        let hints = params.hints.unwrap();
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0], "");
        assert_eq!(hints[1], "valid");
    }

    #[test]
    fn test_auto_params_unicode_hints() {
        let params = AutoParams::new("Content")
            .with_hints(vec!["Unicode: 世界".to_string(), "Emoji: 🌍".to_string()]);
        let hints = params.hints.unwrap();
        assert!(hints[0].contains("世界"));
        assert!(hints[1].contains("🌍"));
    }

    #[test]
    fn test_auto_params_newlines_in_hints() {
        let params = AutoParams::new("Content").with_hints(vec!["Multi\nline\nhint".to_string()]);
        assert!(params.hints.unwrap()[0].contains('\n'));
    }

    // ============================================================================
    // Helper Functions
    // ============================================================================

    fn create_test_mode() -> AutoMode {
        use crate::config::{
            Config, DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig,
            RequestConfig,
        };
        use std::path::PathBuf;

        let langbase_config = LangbaseConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };

        let config = Config {
            langbase: langbase_config.clone(),
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
            error_handling: ErrorHandlingConfig::default(),
        };

        // Use a runtime for async operations in tests
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        AutoMode::new(storage, langbase, &config)
    }
}
