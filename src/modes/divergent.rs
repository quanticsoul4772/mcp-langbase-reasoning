//! Divergent reasoning mode - creative exploration with multiple perspectives.
//!
//! This module provides creative reasoning capabilities:
//! - Multiple perspective generation (2-5 perspectives)
//! - Assumption challenging
//! - Rebellion/contrarian mode for maximum creativity
//! - Novelty and viability scoring

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::DIVERGENT_REASONING_PROMPT;
use crate::storage::{Invocation, SqliteStorage, Storage, Thought};

/// Input parameters for divergent reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergentParams {
    /// The thought content to process
    pub content: String,
    /// Optional session ID (creates new if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional branch ID for tree mode integration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    /// Number of perspectives to generate (2-5)
    #[serde(default = "default_num_perspectives")]
    pub num_perspectives: usize,
    /// Whether to challenge assumptions aggressively
    #[serde(default)]
    pub challenge_assumptions: bool,
    /// Whether to force unconventional/rebellious thinking
    #[serde(default)]
    pub force_rebellion: bool,
    /// Confidence threshold (0.0-1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    0.7 // Lower default for creative mode
}

fn default_num_perspectives() -> usize {
    3
}

/// Response from divergent reasoning Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergentResponse {
    /// The generated perspectives.
    pub perspectives: Vec<Perspective>,
    /// Synthesis combining insights from all perspectives.
    pub synthesis: String,
    /// Additional metadata from the response.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Individual perspective in divergent response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perspective {
    /// The thought content for this perspective.
    pub thought: String,
    /// Novelty score for this perspective (0.0-1.0).
    pub novelty: f64,
    /// Viability score for this perspective (0.0-1.0).
    pub viability: f64,
    /// Assumptions that were challenged to generate this perspective.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assumptions_challenged: Option<Vec<String>>,
}

/// Result of divergent reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergentResult {
    /// The session ID.
    pub session_id: String,
    /// The ID of the main thought.
    pub thought_id: String,
    /// Information about each generated perspective.
    pub perspectives: Vec<PerspectiveInfo>,
    /// Synthesis combining all perspectives.
    pub synthesis: String,
    /// The ID of the synthesis thought.
    pub synthesis_thought_id: String,
    /// Average novelty score across all perspectives (0.0-1.0).
    pub total_novelty_score: f64,
    /// Index of the most viable perspective (0-based).
    pub most_viable_perspective: usize,
    /// Index of the most novel perspective (0-based).
    pub most_novel_perspective: usize,
    /// Optional branch ID for tree mode integration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Perspective information in result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveInfo {
    /// The ID of the perspective thought.
    pub thought_id: String,
    /// The thought content.
    pub content: String,
    /// Novelty score (0.0-1.0).
    pub novelty: f64,
    /// Viability score (0.0-1.0).
    pub viability: f64,
    /// Assumptions that were challenged (None if not analyzed by AI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assumptions_challenged: Option<Vec<String>>,
}

/// Divergent reasoning mode handler for creative exploration.
#[derive(Clone)]
pub struct DivergentMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for divergent reasoning.
    pipe_name: String,
}

impl DivergentMode {
    /// Create a new divergent mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            pipe_name: config.pipes.divergent.clone(),
        }
    }

    /// Process a divergent reasoning request
    pub async fn process(&self, params: DivergentParams) -> AppResult<DivergentResult> {
        let start = Instant::now();

        // Validate input
        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Content cannot be empty".to_string(),
            }
            .into());
        }

        let num_perspectives = params.num_perspectives.clamp(2, 5);

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "divergent")
            .await?;
        debug!(session_id = %session.id, "Processing divergent reasoning");

        // Get previous context
        let previous_thoughts = self
            .core
            .storage()
            .get_session_thoughts(&session.id)
            .await?;

        // Build messages for Langbase
        let messages = self.build_messages(
            &params.content,
            &previous_thoughts,
            num_perspectives,
            params.challenge_assumptions,
            params.force_rebellion,
        );

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.divergent",
            serialize_for_log(&params, "reasoning.divergent input"),
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
        let divergent_response = self.parse_response(&response.completion)?;

        // Create main thought for the original input
        let main_thought = Thought::new(&session.id, &params.content, "divergent")
            .with_confidence(params.confidence);
        let main_thought = if let Some(ref branch_id) = params.branch_id {
            main_thought.with_branch(branch_id)
        } else {
            main_thought
        };
        self.core.storage().create_thought(&main_thought).await?;

        // Create thoughts for each perspective
        let mut perspectives = Vec::new();
        let mut total_novelty = 0.0;
        let mut most_viable_idx = 0;
        let mut most_novel_idx = 0;
        let mut max_viability = 0.0;
        let mut max_novelty = 0.0;

        for (i, p) in divergent_response.perspectives.iter().enumerate() {
            let perspective_thought = Thought::new(&session.id, &p.thought, "divergent")
                .with_confidence((p.novelty + p.viability) / 2.0)
                .with_parent(&main_thought.id)
                .with_metadata(serde_json::json!({
                    "novelty": p.novelty,
                    "viability": p.viability,
                    "perspective_index": i,
                    "assumptions_challenged": p.assumptions_challenged
                }));

            let perspective_thought = if let Some(ref branch_id) = params.branch_id {
                perspective_thought.with_branch(branch_id)
            } else {
                perspective_thought
            };

            self.core
                .storage()
                .create_thought(&perspective_thought)
                .await?;

            total_novelty += p.novelty;

            if p.viability > max_viability {
                max_viability = p.viability;
                most_viable_idx = i;
            }
            if p.novelty > max_novelty {
                max_novelty = p.novelty;
                most_novel_idx = i;
            }

            perspectives.push(PerspectiveInfo {
                thought_id: perspective_thought.id,
                content: p.thought.clone(),
                novelty: p.novelty,
                viability: p.viability,
                assumptions_challenged: p.assumptions_challenged.clone(),
            });
        }

        // Create synthesis thought
        let synthesis_thought =
            Thought::new(&session.id, &divergent_response.synthesis, "divergent")
                .with_confidence(params.confidence)
                .with_parent(&main_thought.id)
                .with_metadata(serde_json::json!({
                    "is_synthesis": true,
                    "source_perspectives": perspectives.len()
                }));

        let synthesis_thought = if let Some(ref branch_id) = params.branch_id {
            synthesis_thought.with_branch(branch_id)
        } else {
            synthesis_thought
        };

        self.core
            .storage()
            .create_thought(&synthesis_thought)
            .await?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&divergent_response, "reasoning.divergent output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

        let avg_novelty = if !perspectives.is_empty() {
            total_novelty / perspectives.len() as f64
        } else {
            0.0
        };

        info!(
            session_id = %session.id,
            thought_id = %main_thought.id,
            num_perspectives = perspectives.len(),
            avg_novelty = avg_novelty,
            latency_ms = latency,
            "Divergent reasoning completed"
        );

        Ok(DivergentResult {
            session_id: session.id,
            thought_id: main_thought.id,
            perspectives,
            synthesis: divergent_response.synthesis,
            synthesis_thought_id: synthesis_thought.id,
            total_novelty_score: avg_novelty,
            most_viable_perspective: most_viable_idx,
            most_novel_perspective: most_novel_idx,
            branch_id: params.branch_id,
        })
    }

    fn build_messages(
        &self,
        content: &str,
        history: &[Thought],
        num_perspectives: usize,
        challenge_assumptions: bool,
        force_rebellion: bool,
    ) -> Vec<Message> {
        let mut messages = Vec::new();

        // Build enhanced system prompt
        let mut system_prompt = DIVERGENT_REASONING_PROMPT.to_string();

        if challenge_assumptions {
            system_prompt.push_str("\n\nIMPORTANT: For each perspective, explicitly identify and challenge at least one underlying assumption. Include these in the 'assumptions_challenged' field.");
        }

        if force_rebellion {
            system_prompt.push_str("\n\nREBELLION MODE: Actively seek contrarian viewpoints. Question the premise of the input. Consider perspectives that might seem absurd or unconventional at first glance - they often lead to breakthrough insights.");
        }

        // Adjust number of perspectives in prompt
        system_prompt = system_prompt.replace(
            "Generate diverse, non-obvious perspectives",
            &format!(
                "Generate {} diverse, non-obvious perspectives",
                num_perspectives
            ),
        );

        messages.push(Message::system(system_prompt));

        // Add history context if available
        if !history.is_empty() {
            let history_text: Vec<String> = history
                .iter()
                .take(5) // Limit history for divergent mode
                .map(|t| format!("- {}", t.content))
                .collect();

            messages.push(Message::user(format!(
                "Recent context (don't let this constrain your creativity):\n{}\n\nNow think divergently about:",
                history_text.join("\n")
            )));
        }

        // Add current content
        messages.push(Message::user(content.to_string()));

        messages
    }

    fn parse_response(&self, completion: &str) -> AppResult<DivergentResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from divergent response"
            );
            ToolError::Reasoning {
                message: format!("Divergent response extraction failed: {}", e),
            }
        })?;

        serde_json::from_str::<DivergentResponse>(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse divergent response: {}", e),
            }
            .into()
        })
    }
}

impl DivergentParams {
    /// Create new params with just content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            session_id: None,
            branch_id: None,
            num_perspectives: default_num_perspectives(),
            challenge_assumptions: false,
            force_rebellion: false,
            confidence: default_confidence(),
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the branch ID
    pub fn with_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = Some(branch_id.into());
        self
    }

    /// Set the number of perspectives to generate
    pub fn with_num_perspectives(mut self, num: usize) -> Self {
        self.num_perspectives = num.clamp(2, 5);
        self
    }

    /// Enable assumption challenging
    pub fn with_assumption_challenging(mut self) -> Self {
        self.challenge_assumptions = true;
        self
    }

    /// Enable rebellion mode for maximum creativity
    pub fn with_rebellion(mut self) -> Self {
        self.force_rebellion = true;
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
    // Default Function Tests
    // ============================================================================

    #[test]
    fn test_default_confidence() {
        assert_eq!(default_confidence(), 0.7);
    }

    #[test]
    fn test_default_num_perspectives() {
        assert_eq!(default_num_perspectives(), 3);
    }

    // ============================================================================
    // DivergentParams Tests
    // ============================================================================

    #[test]
    fn test_divergent_params_new() {
        let params = DivergentParams::new("Test content");
        assert_eq!(params.content, "Test content");
        assert!(params.session_id.is_none());
        assert!(params.branch_id.is_none());
        assert_eq!(params.num_perspectives, 3);
        assert!(!params.challenge_assumptions);
        assert!(!params.force_rebellion);
        assert_eq!(params.confidence, 0.7);
    }

    #[test]
    fn test_divergent_params_with_session() {
        let params = DivergentParams::new("Content").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_divergent_params_with_branch() {
        let params = DivergentParams::new("Content").with_branch("branch-456");
        assert_eq!(params.branch_id, Some("branch-456".to_string()));
    }

    #[test]
    fn test_divergent_params_with_num_perspectives() {
        let params = DivergentParams::new("Content").with_num_perspectives(4);
        assert_eq!(params.num_perspectives, 4);
    }

    #[test]
    fn test_divergent_params_num_perspectives_clamped_high() {
        let params = DivergentParams::new("Content").with_num_perspectives(10);
        assert_eq!(params.num_perspectives, 5); // max is 5
    }

    #[test]
    fn test_divergent_params_num_perspectives_clamped_low() {
        let params = DivergentParams::new("Content").with_num_perspectives(1);
        assert_eq!(params.num_perspectives, 2); // min is 2
    }

    #[test]
    fn test_divergent_params_with_assumption_challenging() {
        let params = DivergentParams::new("Content").with_assumption_challenging();
        assert!(params.challenge_assumptions);
    }

    #[test]
    fn test_divergent_params_with_rebellion() {
        let params = DivergentParams::new("Content").with_rebellion();
        assert!(params.force_rebellion);
    }

    #[test]
    fn test_divergent_params_with_confidence() {
        let params = DivergentParams::new("Content").with_confidence(0.85);
        assert_eq!(params.confidence, 0.85);
    }

    #[test]
    fn test_divergent_params_confidence_clamped_high() {
        let params = DivergentParams::new("Content").with_confidence(1.5);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_divergent_params_confidence_clamped_low() {
        let params = DivergentParams::new("Content").with_confidence(-0.3);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_divergent_params_builder_chain() {
        let params = DivergentParams::new("Chained")
            .with_session("my-session")
            .with_branch("my-branch")
            .with_num_perspectives(4)
            .with_assumption_challenging()
            .with_rebellion()
            .with_confidence(0.9);

        assert_eq!(params.content, "Chained");
        assert_eq!(params.session_id, Some("my-session".to_string()));
        assert_eq!(params.branch_id, Some("my-branch".to_string()));
        assert_eq!(params.num_perspectives, 4);
        assert!(params.challenge_assumptions);
        assert!(params.force_rebellion);
        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_divergent_params_serialize() {
        let params = DivergentParams::new("Test")
            .with_session("sess-1")
            .with_num_perspectives(4)
            .with_assumption_challenging();

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("sess-1"));
        assert!(json.contains("\"num_perspectives\":4"));
        assert!(json.contains("\"challenge_assumptions\":true"));
    }

    #[test]
    fn test_divergent_params_deserialize() {
        let json = r#"{
            "content": "Parsed",
            "session_id": "s-1",
            "branch_id": "b-1",
            "num_perspectives": 5,
            "challenge_assumptions": true,
            "force_rebellion": true,
            "confidence": 0.8
        }"#;
        let params: DivergentParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.content, "Parsed");
        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.branch_id, Some("b-1".to_string()));
        assert_eq!(params.num_perspectives, 5);
        assert!(params.challenge_assumptions);
        assert!(params.force_rebellion);
        assert_eq!(params.confidence, 0.8);
    }

    #[test]
    fn test_divergent_params_deserialize_minimal() {
        let json = r#"{"content": "Only content"}"#;
        let params: DivergentParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.content, "Only content");
        assert!(params.session_id.is_none());
        assert!(params.branch_id.is_none());
        assert_eq!(params.num_perspectives, 3); // default
        assert!(!params.challenge_assumptions);
        assert!(!params.force_rebellion);
        assert_eq!(params.confidence, 0.7); // default
    }

    // ============================================================================
    // Perspective Tests
    // ============================================================================

    #[test]
    fn test_perspective_serialize() {
        let perspective = Perspective {
            thought: "A novel thought".to_string(),
            novelty: 0.85,
            viability: 0.7,
            assumptions_challenged: Some(vec!["Assumption 1".to_string()]),
        };

        let json = serde_json::to_string(&perspective).unwrap();
        assert!(json.contains("A novel thought"));
        assert!(json.contains("0.85"));
        assert!(json.contains("0.7"));
        assert!(json.contains("Assumption 1"));
    }

    #[test]
    fn test_perspective_deserialize() {
        let json = r#"{
            "thought": "Creative idea",
            "novelty": 0.9,
            "viability": 0.6,
            "assumptions_challenged": ["Challenge 1", "Challenge 2"]
        }"#;
        let perspective: Perspective = serde_json::from_str(json).unwrap();

        assert_eq!(perspective.thought, "Creative idea");
        assert_eq!(perspective.novelty, 0.9);
        assert_eq!(perspective.viability, 0.6);
        assert_eq!(perspective.assumptions_challenged.unwrap().len(), 2);
    }

    #[test]
    fn test_perspective_without_assumptions() {
        let json = r#"{
            "thought": "Simple idea",
            "novelty": 0.5,
            "viability": 0.5
        }"#;
        let perspective: Perspective = serde_json::from_str(json).unwrap();

        assert_eq!(perspective.thought, "Simple idea");
        assert!(perspective.assumptions_challenged.is_none());
    }

    // ============================================================================
    // PerspectiveInfo Tests
    // ============================================================================

    #[test]
    fn test_perspective_info_serialize() {
        let info = PerspectiveInfo {
            thought_id: "thought-123".to_string(),
            content: "Perspective content".to_string(),
            novelty: 0.8,
            viability: 0.75,
            assumptions_challenged: Some(vec!["Assumption A".to_string()]),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("thought-123"));
        assert!(json.contains("Perspective content"));
        assert!(json.contains("0.8"));
        assert!(json.contains("0.75"));
        assert!(json.contains("Assumption A"));
    }

    #[test]
    fn test_perspective_info_deserialize() {
        let json = r#"{
            "thought_id": "t-1",
            "content": "Info content",
            "novelty": 0.6,
            "viability": 0.9,
            "assumptions_challenged": ["A", "B"]
        }"#;
        let info: PerspectiveInfo = serde_json::from_str(json).unwrap();

        assert_eq!(info.thought_id, "t-1");
        assert_eq!(info.content, "Info content");
        assert_eq!(info.novelty, 0.6);
        assert_eq!(info.viability, 0.9);
        assert_eq!(info.assumptions_challenged.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_perspective_info_assumptions_none() {
        let info = PerspectiveInfo {
            thought_id: "thought-1".to_string(),
            content: "A perspective".to_string(),
            novelty: 0.7,
            viability: 0.8,
            assumptions_challenged: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        // Verify assumptions_challenged is omitted when None (due to skip_serializing_if)
        assert!(!json.contains("assumptions_challenged"));

        let parsed: PerspectiveInfo = serde_json::from_str(&json).unwrap();
        assert!(parsed.assumptions_challenged.is_none());
    }

    // ============================================================================
    // DivergentResponse Tests
    // ============================================================================

    #[test]
    fn test_divergent_response_serialize() {
        let response = DivergentResponse {
            perspectives: vec![Perspective {
                thought: "Perspective 1".to_string(),
                novelty: 0.8,
                viability: 0.7,
                assumptions_challenged: None,
            }],
            synthesis: "Combined insight".to_string(),
            metadata: serde_json::json!({"key": "value"}),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Perspective 1"));
        assert!(json.contains("Combined insight"));
    }

    #[test]
    fn test_divergent_response_deserialize() {
        let json = r#"{
            "perspectives": [
                {"thought": "P1", "novelty": 0.7, "viability": 0.8},
                {"thought": "P2", "novelty": 0.9, "viability": 0.6}
            ],
            "synthesis": "Synthesis text"
        }"#;
        let response: DivergentResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.perspectives.len(), 2);
        assert_eq!(response.synthesis, "Synthesis text");
    }

    // ============================================================================
    // DivergentResult Tests
    // ============================================================================

    #[test]
    fn test_divergent_result_serialize() {
        let result = DivergentResult {
            session_id: "sess-1".to_string(),
            thought_id: "t-main".to_string(),
            perspectives: vec![PerspectiveInfo {
                thought_id: "t-p1".to_string(),
                content: "First perspective".to_string(),
                novelty: 0.85,
                viability: 0.7,
                assumptions_challenged: Some(vec!["Challenge 1".to_string()]),
            }],
            synthesis: "Final synthesis".to_string(),
            synthesis_thought_id: "t-synth".to_string(),
            total_novelty_score: 0.85,
            most_viable_perspective: 0,
            most_novel_perspective: 0,
            branch_id: Some("branch-1".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("sess-1"));
        assert!(json.contains("t-main"));
        assert!(json.contains("First perspective"));
        assert!(json.contains("Final synthesis"));
        assert!(json.contains("branch-1"));
    }

    #[test]
    fn test_divergent_result_deserialize() {
        let json = r#"{
            "session_id": "s-1",
            "thought_id": "t-1",
            "perspectives": [],
            "synthesis": "Synth",
            "synthesis_thought_id": "t-synth",
            "total_novelty_score": 0.75,
            "most_viable_perspective": 1,
            "most_novel_perspective": 2
        }"#;
        let result: DivergentResult = serde_json::from_str(json).unwrap();

        assert_eq!(result.session_id, "s-1");
        assert_eq!(result.thought_id, "t-1");
        assert_eq!(result.synthesis, "Synth");
        assert_eq!(result.total_novelty_score, 0.75);
        assert_eq!(result.most_viable_perspective, 1);
        assert_eq!(result.most_novel_perspective, 2);
        assert!(result.branch_id.is_none());
    }

    #[test]
    fn test_divergent_result_without_branch() {
        let result = DivergentResult {
            session_id: "s-1".to_string(),
            thought_id: "t-1".to_string(),
            perspectives: vec![],
            synthesis: "No branch".to_string(),
            synthesis_thought_id: "t-s".to_string(),
            total_novelty_score: 0.5,
            most_viable_perspective: 0,
            most_novel_perspective: 0,
            branch_id: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        // branch_id should be omitted due to skip_serializing_if
        assert!(!json.contains("branch_id"));
    }

    // ============================================================================
    // DivergentMode build_messages() Tests
    // ============================================================================

    /// Helper to create a test mode instance
    fn create_test_mode() -> DivergentMode {
        use crate::config::{
            DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LoggingConfig, PipeConfig,
            RequestConfig,
        };
        use std::path::PathBuf;

        let config = Config {
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            langbase: LangbaseConfig {
                api_key: "test_key".to_string(),
                base_url: "https://api.langbase.com".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: crate::config::LogFormat::Pretty,
            },
            request: RequestConfig::default(),
            pipes: PipeConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        DivergentMode::new(storage, langbase, &config)
    }

    #[test]
    fn test_build_messages_basic() {
        use crate::langbase::MessageRole;
        let mode = create_test_mode();

        let messages = mode.build_messages("Test content", &[], 3, false, false);

        // Should have system prompt + user message
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert_eq!(messages[1].content, "Test content");
    }

    #[test]
    fn test_build_messages_with_history() {
        use crate::langbase::MessageRole;
        let mode = create_test_mode();

        // Create some fake thought history
        let thoughts = vec![
            Thought::new("session-1", "Previous thought 1", "divergent"),
            Thought::new("session-1", "Previous thought 2", "divergent"),
        ];

        let messages = mode.build_messages("New content", &thoughts, 3, false, false);

        // Should have system + history context + user content
        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert!(messages[1].content.contains("Recent context"));
        assert!(messages[1].content.contains("Previous thought 1"));
        assert!(messages[1].content.contains("Previous thought 2"));
        assert!(matches!(messages[2].role, MessageRole::User));
        assert_eq!(messages[2].content, "New content");
    }

    #[test]
    fn test_build_messages_history_limited_to_5() {
        let mode = create_test_mode();

        // Create 10 thoughts
        let thoughts: Vec<Thought> = (0..10)
            .map(|i| Thought::new("session-1", format!("Thought {}", i), "divergent"))
            .collect();

        let messages = mode.build_messages("New", &thoughts, 3, false, false);

        // History message should only include first 5 thoughts
        let history_msg = &messages[1];
        assert!(history_msg.content.contains("Thought 0"));
        assert!(history_msg.content.contains("Thought 4"));
        assert!(!history_msg.content.contains("Thought 5")); // Should not include 6th+
    }

    #[test]
    fn test_build_messages_with_challenge_assumptions() {
        let mode = create_test_mode();

        let messages = mode.build_messages("Content", &[], 3, true, false);

        let system_msg = &messages[0];
        assert!(system_msg
            .content
            .contains("explicitly identify and challenge"));
        assert!(system_msg.content.contains("assumptions_challenged"));
    }

    #[test]
    fn test_build_messages_with_force_rebellion() {
        let mode = create_test_mode();

        let messages = mode.build_messages("Content", &[], 3, false, true);

        let system_msg = &messages[0];
        assert!(system_msg.content.contains("REBELLION MODE"));
        assert!(system_msg.content.contains("contrarian viewpoints"));
        assert!(system_msg.content.contains("unconventional"));
    }

    #[test]
    fn test_build_messages_with_both_flags() {
        let mode = create_test_mode();

        let messages = mode.build_messages("Content", &[], 3, true, true);

        let system_msg = &messages[0];
        // Should contain both enhancements
        assert!(system_msg.content.contains("assumptions_challenged"));
        assert!(system_msg.content.contains("REBELLION MODE"));
    }

    #[test]
    fn test_build_messages_num_perspectives_in_prompt() {
        let mode = create_test_mode();

        let messages = mode.build_messages("Content", &[], 5, false, false);

        let system_msg = &messages[0];
        assert!(system_msg.content.contains("Generate 5 diverse"));
    }

    #[test]
    fn test_build_messages_with_different_num_perspectives() {
        let mode = create_test_mode();

        let messages_2 = mode.build_messages("Content", &[], 2, false, false);
        let messages_4 = mode.build_messages("Content", &[], 4, false, false);

        assert!(messages_2[0].content.contains("Generate 2 diverse"));
        assert!(messages_4[0].content.contains("Generate 4 diverse"));
    }

    #[test]
    fn test_build_messages_empty_history() {
        let mode = create_test_mode();

        let messages = mode.build_messages("Content", &[], 3, false, false);

        // Should only have system + user (no history message)
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_build_messages_system_prompt_structure() {
        let mode = create_test_mode();

        let messages = mode.build_messages("Test", &[], 3, false, false);

        let system_msg = &messages[0];
        // Should contain base prompt
        assert!(system_msg.content.contains("diverse"));
        assert!(system_msg.content.contains("perspectives"));
    }

    // ============================================================================
    // Edge Cases and Integration Tests
    // ============================================================================

    #[test]
    fn test_perspective_zero_scores() {
        let perspective = Perspective {
            thought: "Zero scores".to_string(),
            novelty: 0.0,
            viability: 0.0,
            assumptions_challenged: None,
        };

        let json = serde_json::to_string(&perspective).unwrap();
        assert!(json.contains("\"novelty\":0.0"));
        assert!(json.contains("\"viability\":0.0"));
    }

    #[test]
    fn test_perspective_max_scores() {
        let perspective = Perspective {
            thought: "Max scores".to_string(),
            novelty: 1.0,
            viability: 1.0,
            assumptions_challenged: None,
        };

        assert_eq!(perspective.novelty, 1.0);
        assert_eq!(perspective.viability, 1.0);
    }

    #[test]
    fn test_divergent_response_empty_perspectives() {
        let response = DivergentResponse {
            perspectives: vec![],
            synthesis: "No perspectives".to_string(),
            metadata: serde_json::json!({}),
        };

        assert_eq!(response.perspectives.len(), 0);
        assert_eq!(response.synthesis, "No perspectives");
    }

    #[test]
    fn test_divergent_response_multiple_perspectives() {
        let response = DivergentResponse {
            perspectives: vec![
                Perspective {
                    thought: "P1".to_string(),
                    novelty: 0.8,
                    viability: 0.7,
                    assumptions_challenged: None,
                },
                Perspective {
                    thought: "P2".to_string(),
                    novelty: 0.6,
                    viability: 0.9,
                    assumptions_challenged: None,
                },
                Perspective {
                    thought: "P3".to_string(),
                    novelty: 0.9,
                    viability: 0.5,
                    assumptions_challenged: None,
                },
            ],
            synthesis: "Three perspectives".to_string(),
            metadata: serde_json::json!({}),
        };

        assert_eq!(response.perspectives.len(), 3);
        assert_eq!(response.perspectives[0].novelty, 0.8);
        assert_eq!(response.perspectives[1].viability, 0.9);
        assert_eq!(response.perspectives[2].thought, "P3");
    }

    #[test]
    fn test_perspective_with_empty_assumptions_vec() {
        let perspective = Perspective {
            thought: "Test".to_string(),
            novelty: 0.5,
            viability: 0.5,
            assumptions_challenged: Some(vec![]),
        };

        let json = serde_json::to_string(&perspective).unwrap();
        let parsed: Perspective = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.assumptions_challenged.unwrap().len(), 0);
    }

    #[test]
    fn test_perspective_with_multiple_assumptions() {
        let assumptions = vec![
            "Assumption 1".to_string(),
            "Assumption 2".to_string(),
            "Assumption 3".to_string(),
        ];
        let perspective = Perspective {
            thought: "Multi-assumption".to_string(),
            novelty: 0.75,
            viability: 0.65,
            assumptions_challenged: Some(assumptions.clone()),
        };

        assert_eq!(perspective.assumptions_challenged.unwrap().len(), 3);
    }

    #[test]
    fn test_divergent_params_zero_confidence() {
        let params = DivergentParams::new("Test").with_confidence(0.0);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_divergent_params_max_confidence() {
        let params = DivergentParams::new("Test").with_confidence(1.0);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_divergent_params_min_perspectives() {
        let params = DivergentParams::new("Test").with_num_perspectives(2);
        assert_eq!(params.num_perspectives, 2);
    }

    #[test]
    fn test_divergent_params_max_perspectives() {
        let params = DivergentParams::new("Test").with_num_perspectives(5);
        assert_eq!(params.num_perspectives, 5);
    }

    #[test]
    fn test_divergent_result_with_branch() {
        let result = DivergentResult {
            session_id: "s-1".to_string(),
            thought_id: "t-1".to_string(),
            perspectives: vec![],
            synthesis: "With branch".to_string(),
            synthesis_thought_id: "t-s".to_string(),
            total_novelty_score: 0.5,
            most_viable_perspective: 0,
            most_novel_perspective: 0,
            branch_id: Some("branch-123".to_string()),
        };

        assert_eq!(result.branch_id, Some("branch-123".to_string()));
    }

    #[test]
    fn test_perspective_info_with_empty_assumptions() {
        let info = PerspectiveInfo {
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            novelty: 0.5,
            viability: 0.5,
            assumptions_challenged: Some(vec![]),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("assumptions_challenged"));
    }

    #[test]
    fn test_divergent_response_with_metadata() {
        let metadata = serde_json::json!({
            "model": "gpt-4",
            "temperature": 0.8,
            "custom_field": "value"
        });

        let response = DivergentResponse {
            perspectives: vec![],
            synthesis: "Test".to_string(),
            metadata: metadata.clone(),
        };

        assert_eq!(response.metadata["model"], "gpt-4");
        assert_eq!(response.metadata["temperature"], 0.8);
    }

    #[test]
    fn test_divergent_params_content_with_special_chars() {
        let content = "Test with special chars: !@#$%^&*()_+{}|:<>?";
        let params = DivergentParams::new(content);
        assert_eq!(params.content, content);
    }

    #[test]
    fn test_divergent_params_content_with_newlines() {
        let content = "Line 1\nLine 2\nLine 3";
        let params = DivergentParams::new(content);
        assert_eq!(params.content, content);
    }

    #[test]
    fn test_divergent_params_content_unicode() {
        let content = "Unicode: 你好世界 🌍 émojis";
        let params = DivergentParams::new(content);
        assert_eq!(params.content, content);
    }

    #[test]
    fn test_perspective_round_trip_serialization() {
        let original = Perspective {
            thought: "Round trip test".to_string(),
            novelty: 0.777,
            viability: 0.888,
            assumptions_challenged: Some(vec!["A1".to_string(), "A2".to_string()]),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Perspective = serde_json::from_str(&json).unwrap();

        assert_eq!(original.thought, deserialized.thought);
        assert_eq!(original.novelty, deserialized.novelty);
        assert_eq!(original.viability, deserialized.viability);
        assert_eq!(
            original.assumptions_challenged,
            deserialized.assumptions_challenged
        );
    }

    #[test]
    fn test_divergent_result_round_trip() {
        let original = DivergentResult {
            session_id: "s-123".to_string(),
            thought_id: "t-456".to_string(),
            perspectives: vec![PerspectiveInfo {
                thought_id: "p-1".to_string(),
                content: "Perspective".to_string(),
                novelty: 0.9,
                viability: 0.8,
                assumptions_challenged: Some(vec!["Challenge".to_string()]),
            }],
            synthesis: "Synth".to_string(),
            synthesis_thought_id: "t-synth".to_string(),
            total_novelty_score: 0.85,
            most_viable_perspective: 0,
            most_novel_perspective: 0,
            branch_id: Some("branch".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: DivergentResult = serde_json::from_str(&json).unwrap();

        assert_eq!(original.session_id, deserialized.session_id);
        assert_eq!(original.thought_id, deserialized.thought_id);
        assert_eq!(original.synthesis, deserialized.synthesis);
        assert_eq!(original.perspectives.len(), deserialized.perspectives.len());
        assert_eq!(original.branch_id, deserialized.branch_id);
    }

    // ============================================================================
    // DivergentMode::new() Tests
    // ============================================================================

    #[test]
    fn test_divergent_mode_new() {
        use crate::config::{
            DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LoggingConfig, PipeConfig,
            RequestConfig,
        };
        use std::path::PathBuf;

        let config = Config {
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            langbase: LangbaseConfig {
                api_key: "test_key".to_string(),
                base_url: "https://api.langbase.com".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: crate::config::LogFormat::Pretty,
            },
            request: RequestConfig::default(),
            pipes: PipeConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = DivergentMode::new(storage, langbase, &config);

        assert_eq!(mode.pipe_name, config.pipes.divergent);
    }

    #[test]
    fn test_divergent_mode_new_with_custom_pipe() {
        use crate::config::{
            DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LoggingConfig, PipeConfig,
            RequestConfig,
        };
        use std::path::PathBuf;

        let pipes = PipeConfig {
            divergent: "custom-divergent-pipe".to_string(),
            ..Default::default()
        };

        let config = Config {
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            langbase: LangbaseConfig {
                api_key: "api_key".to_string(),
                base_url: "https://api.langbase.com".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: crate::config::LogFormat::Pretty,
            },
            request: RequestConfig::default(),
            pipes,
            error_handling: ErrorHandlingConfig::default(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let mode = DivergentMode::new(storage, langbase, &config);

        assert_eq!(mode.pipe_name, "custom-divergent-pipe");
    }

    // ============================================================================
    // parse_response() Tests
    // ============================================================================

    #[test]
    fn test_parse_response_valid_json() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {
                    "thought": "First perspective",
                    "novelty": 0.8,
                    "viability": 0.7
                }
            ],
            "synthesis": "Combined insight"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.perspectives.len(), 1);
        assert_eq!(response.synthesis, "Combined insight");
    }

    #[test]
    fn test_parse_response_with_markdown_json_block() {
        let mode = create_test_mode();

        let markdown = r#"
Here's the response:
```json
{
    "perspectives": [
        {"thought": "P1", "novelty": 0.9, "viability": 0.8}
    ],
    "synthesis": "Result"
}
```
"#;

        let result = mode.parse_response(markdown);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.perspectives.len(), 1);
        assert_eq!(response.synthesis, "Result");
    }

    #[test]
    fn test_parse_response_with_code_block() {
        let mode = create_test_mode();

        let code_block = r#"
```
{
    "perspectives": [
        {"thought": "P1", "novelty": 0.5, "viability": 0.6}
    ],
    "synthesis": "Synth"
}
```
"#;

        let result = mode.parse_response(code_block);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.perspectives.len(), 1);
        assert_eq!(response.synthesis, "Synth");
    }

    #[test]
    fn test_parse_response_multiple_perspectives() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {"thought": "P1", "novelty": 0.8, "viability": 0.7},
                {"thought": "P2", "novelty": 0.6, "viability": 0.9},
                {"thought": "P3", "novelty": 0.9, "viability": 0.5}
            ],
            "synthesis": "Three perspectives combined"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.perspectives.len(), 3);
        assert_eq!(response.perspectives[0].thought, "P1");
        assert_eq!(response.perspectives[1].thought, "P2");
        assert_eq!(response.perspectives[2].thought, "P3");
    }

    #[test]
    fn test_parse_response_with_assumptions_challenged() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {
                    "thought": "Challenge assumptions",
                    "novelty": 0.7,
                    "viability": 0.8,
                    "assumptions_challenged": ["Assumption 1", "Assumption 2"]
                }
            ],
            "synthesis": "Synthesis text"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        let assumptions = response.perspectives[0]
            .assumptions_challenged
            .as_ref()
            .unwrap();
        assert_eq!(assumptions.len(), 2);
        assert_eq!(assumptions[0], "Assumption 1");
        assert_eq!(assumptions[1], "Assumption 2");
    }

    #[test]
    fn test_parse_response_with_metadata() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {"thought": "P1", "novelty": 0.8, "viability": 0.7}
            ],
            "synthesis": "Synth",
            "metadata": {
                "model": "gpt-4",
                "temperature": 0.9
            }
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.metadata["model"], "gpt-4");
        assert_eq!(response.metadata["temperature"], 0.9);
    }

    #[test]
    fn test_parse_response_invalid_json() {
        let mode = create_test_mode();

        let invalid = "This is not JSON";
        let result = mode.parse_response(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_missing_required_fields() {
        let mode = create_test_mode();

        let incomplete = r#"{
            "perspectives": []
        }"#;

        let result = mode.parse_response(incomplete);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_empty_perspectives() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [],
            "synthesis": "No perspectives"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.perspectives.len(), 0);
    }

    #[test]
    fn test_parse_response_unicode_content() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {
                    "thought": "Unicode: 你好世界 🌍 émojis",
                    "novelty": 0.85,
                    "viability": 0.75
                }
            ],
            "synthesis": "国际化 synthesis"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.perspectives[0].thought.contains("你好世界"));
        assert!(response.perspectives[0].thought.contains("🌍"));
        assert!(response.synthesis.contains("国际化"));
    }

    #[test]
    fn test_parse_response_escaped_characters() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {
                    "thought": "Special chars: \"quotes\" \n newlines \t tabs",
                    "novelty": 0.7,
                    "viability": 0.8
                }
            ],
            "synthesis": "Escaped: \\ backslash"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.perspectives[0].thought.contains("quotes"));
        assert!(response.synthesis.contains("backslash"));
    }

    #[test]
    fn test_parse_response_extreme_scores() {
        let mode = create_test_mode();

        let json = r#"{
            "perspectives": [
                {"thought": "Min", "novelty": 0.0, "viability": 0.0},
                {"thought": "Max", "novelty": 1.0, "viability": 1.0}
            ],
            "synthesis": "Extreme scores"
        }"#;

        let result = mode.parse_response(json);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.perspectives[0].novelty, 0.0);
        assert_eq!(response.perspectives[0].viability, 0.0);
        assert_eq!(response.perspectives[1].novelty, 1.0);
        assert_eq!(response.perspectives[1].viability, 1.0);
    }

    // ============================================================================
    // Additional build_messages() Edge Cases
    // ============================================================================

    #[test]
    fn test_build_messages_unicode_content() {
        let mode = create_test_mode();

        let content = "Unicode test: 你好 🚀 café";
        let messages = mode.build_messages(content, &[], 3, false, false);

        assert_eq!(messages[1].content, content);
    }

    #[test]
    fn test_build_messages_very_long_content() {
        let mode = create_test_mode();

        let long_content = "A".repeat(10000);
        let messages = mode.build_messages(&long_content, &[], 3, false, false);

        assert_eq!(messages[1].content, long_content);
    }

    #[test]
    fn test_build_messages_special_characters() {
        let mode = create_test_mode();

        let special = "Special: !@#$%^&*()_+{}|:<>?[]\\;',./`~";
        let messages = mode.build_messages(special, &[], 3, false, false);

        assert_eq!(messages[1].content, special);
    }

    #[test]
    fn test_build_messages_multiline_content() {
        let mode = create_test_mode();

        let multiline = "Line 1\nLine 2\nLine 3\nLine 4";
        let messages = mode.build_messages(multiline, &[], 3, false, false);

        assert_eq!(messages[1].content, multiline);
    }

    #[test]
    fn test_build_messages_with_tabs_and_spaces() {
        let mode = create_test_mode();

        let content = "Indented:\n\tTab line\n    Space line";
        let messages = mode.build_messages(content, &[], 3, false, false);

        assert_eq!(messages[1].content, content);
    }

    // ============================================================================
    // Boolean Flag Combinations
    // ============================================================================

    #[test]
    fn test_divergent_params_all_flags_false() {
        let params = DivergentParams::new("Test");
        assert!(!params.challenge_assumptions);
        assert!(!params.force_rebellion);
    }

    #[test]
    fn test_divergent_params_all_flags_true() {
        let params = DivergentParams::new("Test")
            .with_assumption_challenging()
            .with_rebellion();

        assert!(params.challenge_assumptions);
        assert!(params.force_rebellion);
    }

    #[test]
    fn test_divergent_params_serialize_false_booleans() {
        let params = DivergentParams::new("Test");
        let json = serde_json::to_string(&params).unwrap();

        // Default false booleans should still appear in JSON
        assert!(json.contains("\"challenge_assumptions\":false"));
        assert!(json.contains("\"force_rebellion\":false"));
    }

    #[test]
    fn test_divergent_params_serialize_true_booleans() {
        let params = DivergentParams::new("Test")
            .with_assumption_challenging()
            .with_rebellion();

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"challenge_assumptions\":true"));
        assert!(json.contains("\"force_rebellion\":true"));
    }

    #[test]
    fn test_divergent_params_deserialize_false_booleans() {
        let json = r#"{
            "content": "Test",
            "challenge_assumptions": false,
            "force_rebellion": false
        }"#;

        let params: DivergentParams = serde_json::from_str(json).unwrap();
        assert!(!params.challenge_assumptions);
        assert!(!params.force_rebellion);
    }

    #[test]
    fn test_divergent_params_deserialize_true_booleans() {
        let json = r#"{
            "content": "Test",
            "challenge_assumptions": true,
            "force_rebellion": true
        }"#;

        let params: DivergentParams = serde_json::from_str(json).unwrap();
        assert!(params.challenge_assumptions);
        assert!(params.force_rebellion);
    }

    // ============================================================================
    // Fractional Score Tests
    // ============================================================================

    #[test]
    fn test_perspective_fractional_scores() {
        let perspective = Perspective {
            thought: "Fractional".to_string(),
            novelty: 0.123456789,
            viability: 0.987654321,
            assumptions_challenged: None,
        };

        let json = serde_json::to_string(&perspective).unwrap();
        let parsed: Perspective = serde_json::from_str(&json).unwrap();

        // Should preserve precision to reasonable degree
        assert!((parsed.novelty - 0.123456789).abs() < 0.0001);
        assert!((parsed.viability - 0.987654321).abs() < 0.0001);
    }

    #[test]
    fn test_divergent_result_fractional_novelty_score() {
        let result = DivergentResult {
            session_id: "s-1".to_string(),
            thought_id: "t-1".to_string(),
            perspectives: vec![],
            synthesis: "Test".to_string(),
            synthesis_thought_id: "t-s".to_string(),
            total_novelty_score: 0.6666666666666666,
            most_viable_perspective: 0,
            most_novel_perspective: 0,
            branch_id: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: DivergentResult = serde_json::from_str(&json).unwrap();

        assert!((parsed.total_novelty_score - 0.6666666666666666).abs() < 0.0001);
    }

    // ============================================================================
    // Empty and Whitespace Tests
    // ============================================================================

    #[test]
    fn test_divergent_params_empty_content() {
        let params = DivergentParams::new("");
        assert_eq!(params.content, "");
    }

    #[test]
    fn test_divergent_params_whitespace_content() {
        let params = DivergentParams::new("   \t\n  ");
        assert_eq!(params.content, "   \t\n  ");
    }

    #[test]
    fn test_perspective_empty_thought() {
        let perspective = Perspective {
            thought: "".to_string(),
            novelty: 0.5,
            viability: 0.5,
            assumptions_challenged: None,
        };

        assert_eq!(perspective.thought, "");
    }

    #[test]
    fn test_divergent_response_empty_synthesis() {
        let response = DivergentResponse {
            perspectives: vec![],
            synthesis: "".to_string(),
            metadata: serde_json::json!({}),
        };

        assert_eq!(response.synthesis, "");
    }

    // ============================================================================
    // Large Array Tests
    // ============================================================================

    #[test]
    fn test_perspective_many_assumptions_challenged() {
        let many_assumptions: Vec<String> = (0..100).map(|i| format!("Assumption {}", i)).collect();

        let perspective = Perspective {
            thought: "Many assumptions".to_string(),
            novelty: 0.8,
            viability: 0.7,
            assumptions_challenged: Some(many_assumptions.clone()),
        };

        let json = serde_json::to_string(&perspective).unwrap();
        let parsed: Perspective = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.assumptions_challenged.unwrap().len(), 100);
    }

    #[test]
    fn test_divergent_response_five_perspectives() {
        let perspectives: Vec<Perspective> = (0..5)
            .map(|i| Perspective {
                thought: format!("Perspective {}", i),
                novelty: 0.5 + (i as f64 * 0.1),
                viability: 0.6 + (i as f64 * 0.05),
                assumptions_challenged: None,
            })
            .collect();

        let response = DivergentResponse {
            perspectives,
            synthesis: "Five perspectives".to_string(),
            metadata: serde_json::json!({}),
        };

        assert_eq!(response.perspectives.len(), 5);
        for i in 0..5 {
            assert_eq!(
                response.perspectives[i].thought,
                format!("Perspective {}", i)
            );
        }
    }

    // ============================================================================
    // Metadata Variations
    // ============================================================================

    #[test]
    fn test_divergent_response_null_metadata() {
        let json = r#"{
            "perspectives": [],
            "synthesis": "Test",
            "metadata": null
        }"#;

        let response: DivergentResponse = serde_json::from_str(json).unwrap();
        assert!(response.metadata.is_null());
    }

    #[test]
    fn test_divergent_response_complex_metadata() {
        let complex_metadata = serde_json::json!({
            "nested": {
                "deep": {
                    "value": 42
                }
            },
            "array": [1, 2, 3],
            "string": "test"
        });

        let response = DivergentResponse {
            perspectives: vec![],
            synthesis: "Test".to_string(),
            metadata: complex_metadata.clone(),
        };

        assert_eq!(response.metadata["nested"]["deep"]["value"], 42);
        assert_eq!(response.metadata["array"][0], 1);
    }

    // ============================================================================
    // Builder Pattern Overwrite Tests
    // ============================================================================

    #[test]
    fn test_divergent_params_overwrite_session() {
        let params = DivergentParams::new("Test")
            .with_session("first")
            .with_session("second");

        assert_eq!(params.session_id, Some("second".to_string()));
    }

    #[test]
    fn test_divergent_params_overwrite_branch() {
        let params = DivergentParams::new("Test")
            .with_branch("branch-1")
            .with_branch("branch-2");

        assert_eq!(params.branch_id, Some("branch-2".to_string()));
    }

    #[test]
    fn test_divergent_params_overwrite_num_perspectives() {
        let params = DivergentParams::new("Test")
            .with_num_perspectives(2)
            .with_num_perspectives(4);

        assert_eq!(params.num_perspectives, 4);
    }

    #[test]
    fn test_divergent_params_overwrite_confidence() {
        let params = DivergentParams::new("Test")
            .with_confidence(0.5)
            .with_confidence(0.9);

        assert_eq!(params.confidence, 0.9);
    }

    // ============================================================================
    // Index Boundary Tests
    // ============================================================================

    #[test]
    fn test_divergent_result_perspective_indices() {
        let result = DivergentResult {
            session_id: "s-1".to_string(),
            thought_id: "t-1".to_string(),
            perspectives: vec![
                PerspectiveInfo {
                    thought_id: "p-0".to_string(),
                    content: "P0".to_string(),
                    novelty: 0.5,
                    viability: 0.8,
                    assumptions_challenged: None,
                },
                PerspectiveInfo {
                    thought_id: "p-1".to_string(),
                    content: "P1".to_string(),
                    novelty: 0.9,
                    viability: 0.6,
                    assumptions_challenged: None,
                },
            ],
            synthesis: "Test".to_string(),
            synthesis_thought_id: "t-s".to_string(),
            total_novelty_score: 0.7,
            most_viable_perspective: 0,
            most_novel_perspective: 1,
            branch_id: None,
        };

        assert_eq!(result.most_viable_perspective, 0);
        assert_eq!(result.most_novel_perspective, 1);
        assert_eq!(result.perspectives[0].viability, 0.8);
        assert_eq!(result.perspectives[1].novelty, 0.9);
    }

    // ============================================================================
    // Skip Serializing Tests
    // ============================================================================

    #[test]
    fn test_divergent_params_skip_serializing_none_values() {
        let params = DivergentParams::new("Test");
        let json = serde_json::to_string(&params).unwrap();

        // session_id and branch_id should be omitted when None
        assert!(!json.contains("\"session_id\""));
        assert!(!json.contains("\"branch_id\""));
    }

    #[test]
    fn test_perspective_info_skip_serializing_none_assumptions() {
        let info = PerspectiveInfo {
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            novelty: 0.5,
            viability: 0.5,
            assumptions_challenged: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("assumptions_challenged"));
    }

    #[test]
    fn test_divergent_result_skip_serializing_none_branch() {
        let result = DivergentResult {
            session_id: "s-1".to_string(),
            thought_id: "t-1".to_string(),
            perspectives: vec![],
            synthesis: "Test".to_string(),
            synthesis_thought_id: "t-s".to_string(),
            total_novelty_score: 0.5,
            most_viable_perspective: 0,
            most_novel_perspective: 0,
            branch_id: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("\"branch_id\""));
    }
}
