//! Auto mode router - automatically selects the most appropriate reasoning mode

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::AppResult;
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
    fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<AutoResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse auto router response, using fallback"
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
}

/// Auto mode router
#[derive(Clone)]
pub struct AutoMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for auto routing.
    pipe_name: String,
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
        let auto_response = AutoResponse::from_completion(&response.completion);

        // Convert mode string to enum
        let recommended_mode = auto_response
            .recommended_mode
            .parse()
            .unwrap_or_else(|_| {
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
        let resp = AutoResponse::from_completion(json);
        assert_eq!(resp.recommended_mode, "tree");
        assert_eq!(resp.confidence, 0.9);
        assert_eq!(resp.complexity, 0.6);
    }

    #[test]
    fn test_auto_response_from_plain_text() {
        let text = "Invalid JSON";
        let resp = AutoResponse::from_completion(text);
        assert_eq!(resp.recommended_mode, "linear");
        assert_eq!(resp.confidence, 0.7);
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
        let resp = AutoResponse::from_completion(json);
        assert_eq!(resp.recommended_mode, "divergent");
        assert!(resp.metadata.is_some());
    }

    #[test]
    fn test_auto_response_default_complexity() {
        let json = r#"{"recommended_mode": "linear", "confidence": 0.8, "rationale": "Test"}"#;
        let resp = AutoResponse::from_completion(json);
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
            let resp = AutoResponse::from_completion(&json);
            assert_eq!(resp.recommended_mode, mode_str);
        }
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
        let resp = AutoResponse::from_completion(json);
        assert_eq!(resp.confidence, 0.0);
    }

    #[test]
    fn test_auto_response_max_confidence() {
        let json =
            r#"{"recommended_mode": "linear", "confidence": 1.0, "rationale": "Full confidence"}"#;
        let resp = AutoResponse::from_completion(json);
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
}
