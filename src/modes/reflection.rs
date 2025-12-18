//! Reflection reasoning mode - meta-cognitive analysis and quality improvement.
//!
//! This module provides reflection capabilities for analyzing and improving reasoning:
//! - Iterative refinement with quality thresholds
//! - Strength and weakness identification
//! - Improved thought generation
//! - Session evaluation for overall reasoning quality

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::REFLECTION_PROMPT;
use crate::storage::{Invocation, SqliteStorage, Storage, Thought};

/// Input parameters for reflection reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionParams {
    /// The thought ID to reflect upon (optional - if not provided, reflects on content)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_id: Option<String>,
    /// Content to reflect upon (used if thought_id not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional session ID (creates new if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional branch ID for tree mode integration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    /// Maximum iterations for iterative refinement
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    /// Quality threshold to stop iterating (0.0-1.0)
    #[serde(default = "default_quality_threshold")]
    pub quality_threshold: f64,
    /// Whether to include full reasoning chain in context
    #[serde(default)]
    pub include_chain: bool,
}

fn default_max_iterations() -> usize {
    3
}

fn default_quality_threshold() -> f64 {
    0.8
}

/// Response from reflection reasoning Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResponse {
    /// The meta-cognitive analysis of the thought.
    pub analysis: String,
    /// Identified strengths in the reasoning.
    pub strengths: Vec<String>,
    /// Identified weaknesses in the reasoning.
    pub weaknesses: Vec<String>,
    /// Recommendations for improvement.
    pub recommendations: Vec<String>,
    /// Confidence in the reflection analysis (0.0-1.0).
    pub confidence: f64,
    /// Optional quality score for the original thought (0.0-1.0).
    #[serde(default)]
    pub quality_score: Option<f64>,
    /// Optional improved version of the thought.
    #[serde(default)]
    pub improved_thought: Option<String>,
    /// Additional metadata from the response.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Result of reflection reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResult {
    /// The session ID.
    pub session_id: String,
    /// The ID of the reflection thought that was created.
    pub reflection_thought_id: String,
    /// The ID of the original thought that was reflected upon, if any.
    pub original_thought_id: Option<String>,
    /// The meta-cognitive analysis of the thought.
    pub analysis: String,
    /// Identified strengths in the reasoning.
    pub strengths: Vec<String>,
    /// Identified weaknesses in the reasoning.
    pub weaknesses: Vec<String>,
    /// Recommendations for improvement.
    pub recommendations: Vec<String>,
    /// Quality score of the thought (0.0-1.0).
    pub quality_score: f64,
    /// Optional improved version of the thought.
    pub improved_thought: Option<ImprovedThought>,
    /// Number of reflection iterations performed.
    pub iterations_performed: usize,
    /// Whether quality improved from the original.
    pub quality_improved: bool,
    /// Optional branch ID for tree mode integration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Improved thought generated from reflection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovedThought {
    /// The ID of the improved thought.
    pub thought_id: String,
    /// The improved content.
    pub content: String,
    /// Confidence in the improved thought (0.0-1.0).
    pub confidence: f64,
}

/// Reflection reasoning mode handler for meta-cognitive analysis.
#[derive(Clone)]
pub struct ReflectionMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for reflection.
    pipe_name: String,
}

impl ReflectionMode {
    /// Create a new reflection mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            pipe_name: config.pipes.reflection.clone(),
        }
    }

    /// Process a reflection reasoning request
    pub async fn process(&self, params: ReflectionParams) -> AppResult<ReflectionResult> {
        let start = Instant::now();

        // Validate input - need either thought_id or content
        if params.thought_id.is_none() && params.content.is_none() {
            return Err(ToolError::Validation {
                field: "thought_id or content".to_string(),
                reason: "Either thought_id or content must be provided".to_string(),
            }
            .into());
        }

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "reflection")
            .await?;
        debug!(session_id = %session.id, "Processing reflection reasoning");

        // Get the content to reflect upon
        let (original_content, original_thought) = if let Some(thought_id) = &params.thought_id {
            let thought = self
                .core
                .storage()
                .get_thought(thought_id)
                .await?
                .ok_or_else(|| ToolError::Session(format!("Thought not found: {}", thought_id)))?;
            (thought.content.clone(), Some(thought))
        } else {
            (params.content.clone().unwrap_or_default(), None)
        };

        // Get reasoning chain context if requested
        let context_chain = if params.include_chain {
            if let Some(ref thought) = original_thought {
                self.get_reasoning_chain(&session.id, thought).await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Perform iterative reflection
        let max_iterations = params.max_iterations.min(5);
        let mut current_content = original_content.clone();
        let mut iterations_performed = 0;
        let mut best_quality = 0.0;
        let mut final_response: Option<ReflectionResponse> = None;

        for iteration in 0..max_iterations {
            iterations_performed = iteration + 1;

            // Build messages for Langbase
            let messages = self.build_messages(&current_content, &context_chain, iteration);

            // Create invocation log
            let mut invocation = Invocation::new(
                "reasoning.reflection",
                serde_json::json!({
                    "iteration": iteration,
                    "content": &current_content
                }),
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
            let reflection = self.parse_response(&response.completion)?;
            let quality = reflection.quality_score.unwrap_or(reflection.confidence);

            // Log invocation
            let latency = start.elapsed().as_millis() as i64;
            invocation = invocation.success(
                serialize_for_log(&reflection, "reasoning.reflection output"),
                latency,
            );
            self.core.storage().log_invocation(&invocation).await?;

            // Check if quality threshold met
            if quality >= params.quality_threshold {
                debug!(
                    iteration = iteration,
                    quality = quality,
                    threshold = params.quality_threshold,
                    "Quality threshold met, stopping iterations"
                );
                best_quality = quality;
                final_response = Some(reflection);
                break;
            }

            // Update for next iteration if improved thought available
            if let Some(ref improved) = reflection.improved_thought {
                if quality > best_quality {
                    best_quality = quality;
                    current_content = improved.clone();
                }
            }

            final_response = Some(reflection);
        }

        let reflection = final_response.ok_or_else(|| ToolError::Reasoning {
            message: "No reflection response generated".to_string(),
        })?;

        // Create reflection thought
        let reflection_thought = Thought::new(&session.id, &reflection.analysis, "reflection")
            .with_confidence(reflection.confidence)
            .with_metadata(serde_json::json!({
                "strengths": reflection.strengths,
                "weaknesses": reflection.weaknesses,
                "recommendations": reflection.recommendations,
                "quality_score": best_quality,
                "iterations": iterations_performed
            }));

        let reflection_thought = if let Some(ref thought) = original_thought {
            reflection_thought.with_parent(&thought.id)
        } else {
            reflection_thought
        };

        let reflection_thought = if let Some(ref branch_id) = params.branch_id {
            reflection_thought.with_branch(branch_id)
        } else {
            reflection_thought
        };

        self.core
            .storage()
            .create_thought(&reflection_thought)
            .await?;

        // Create improved thought if available and different from original
        let improved_thought = if let Some(ref improved_content) = reflection.improved_thought {
            if improved_content != &original_content {
                let improved = Thought::new(&session.id, improved_content, "reflection")
                    .with_confidence(best_quality)
                    .with_parent(&reflection_thought.id)
                    .with_metadata(serde_json::json!({
                        "is_improved_version": true,
                        "original_thought_id": original_thought.as_ref().map(|t| &t.id)
                    }));

                let improved = if let Some(ref branch_id) = params.branch_id {
                    improved.with_branch(branch_id)
                } else {
                    improved
                };

                self.core.storage().create_thought(&improved).await?;

                Some(ImprovedThought {
                    thought_id: improved.id,
                    content: improved_content.clone(),
                    confidence: best_quality,
                })
            } else {
                None
            }
        } else {
            None
        };

        let quality_improved = best_quality
            > original_thought
                .as_ref()
                .map(|t| t.confidence)
                .unwrap_or(0.5);

        info!(
            session_id = %session.id,
            reflection_id = %reflection_thought.id,
            iterations = iterations_performed,
            quality_score = best_quality,
            quality_improved = quality_improved,
            latency_ms = start.elapsed().as_millis(),
            "Reflection reasoning completed"
        );

        Ok(ReflectionResult {
            session_id: session.id,
            reflection_thought_id: reflection_thought.id,
            original_thought_id: original_thought.map(|t| t.id),
            analysis: reflection.analysis,
            strengths: reflection.strengths,
            weaknesses: reflection.weaknesses,
            recommendations: reflection.recommendations,
            quality_score: best_quality,
            improved_thought,
            iterations_performed,
            quality_improved,
            branch_id: params.branch_id,
        })
    }

    /// Self-evaluate a session's reasoning quality
    pub async fn evaluate_session(&self, session_id: &str) -> AppResult<SessionEvaluation> {
        let thoughts = self.core.storage().get_session_thoughts(session_id).await?;

        if thoughts.is_empty() {
            return Err(
                ToolError::Session("Session has no thoughts to evaluate".to_string()).into(),
            );
        }

        let total_confidence: f64 = thoughts.iter().map(|t| t.confidence).sum();
        let avg_confidence = total_confidence / thoughts.len() as f64;

        let mode_counts: std::collections::HashMap<String, usize> =
            thoughts
                .iter()
                .fold(std::collections::HashMap::new(), |mut acc, t| {
                    *acc.entry(t.mode.clone()).or_insert(0) += 1;
                    acc
                });

        let coherence_score = self.calculate_coherence(&thoughts);

        Ok(SessionEvaluation {
            session_id: session_id.to_string(),
            total_thoughts: thoughts.len(),
            average_confidence: avg_confidence,
            mode_distribution: mode_counts,
            coherence_score,
            recommendation: if avg_confidence < 0.6 {
                "Consider reviewing and refining low-confidence thoughts".to_string()
            } else if coherence_score < 0.5 {
                "Reasoning chain may have logical gaps - consider reflection mode".to_string()
            } else {
                "Reasoning quality is acceptable".to_string()
            },
        })
    }

    async fn get_reasoning_chain(
        &self,
        session_id: &str,
        thought: &Thought,
    ) -> AppResult<Vec<Thought>> {
        let all_thoughts = self.core.storage().get_session_thoughts(session_id).await?;

        // Build chain by following parent_id references
        let mut chain = Vec::new();
        let mut current_id = thought.parent_id.clone();

        while let Some(parent_id) = current_id {
            if let Some(parent) = all_thoughts.iter().find(|t| t.id == parent_id) {
                chain.push(parent.clone());
                current_id = parent.parent_id.clone();
            } else {
                break;
            }

            // Limit chain length
            if chain.len() >= 10 {
                break;
            }
        }

        chain.reverse(); // Oldest first
        chain.push(thought.clone()); // Add the target thought

        Ok(chain)
    }

    fn calculate_coherence(&self, thoughts: &[Thought]) -> f64 {
        if thoughts.len() < 2 {
            return 1.0;
        }

        // Simple coherence metric based on confidence progression and parent chains
        let mut linked_count = 0;
        for thought in thoughts.iter().skip(1) {
            if thought.parent_id.is_some() {
                linked_count += 1;
            }
        }

        let link_ratio = linked_count as f64 / (thoughts.len() - 1) as f64;

        // Penalize large confidence swings
        let confidence_stability: f64 = thoughts
            .windows(2)
            .map(|w| 1.0 - (w[0].confidence - w[1].confidence).abs())
            .sum::<f64>()
            / (thoughts.len() - 1) as f64;

        (link_ratio + confidence_stability) / 2.0
    }

    fn build_messages(&self, content: &str, chain: &[Thought], iteration: usize) -> Vec<Message> {
        let mut messages = Vec::new();

        // Enhanced system prompt for iteration
        let mut system_prompt = REFLECTION_PROMPT.to_string();
        if iteration > 0 {
            system_prompt.push_str(&format!(
                "\n\nThis is iteration {} of reflection. Focus on addressing previously identified weaknesses and improving the thought quality.",
                iteration + 1
            ));
        }

        messages.push(Message::system(system_prompt));

        // Add reasoning chain context if available
        if !chain.is_empty() {
            let chain_text: Vec<String> = chain
                .iter()
                .map(|t| {
                    format!(
                        "- [{}] (confidence: {:.2}) {}",
                        t.mode, t.confidence, t.content
                    )
                })
                .collect();

            messages.push(Message::user(format!(
                "Reasoning chain leading to this thought:\n{}\n\nNow reflect on the final thought:",
                chain_text.join("\n")
            )));
        }

        // Add content to reflect upon
        messages.push(Message::user(format!(
            "Thought to reflect upon:\n\n{}",
            content
        )));

        messages
    }

    fn parse_response(&self, completion: &str) -> AppResult<ReflectionResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from reflection response"
            );
            ToolError::Reasoning {
                message: format!("Reflection response extraction failed: {}", e),
            }
        })?;

        serde_json::from_str::<ReflectionResponse>(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse reflection response: {}", e),
            }
            .into()
        })
    }
}

/// Session evaluation result showing overall reasoning quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvaluation {
    /// The session ID that was evaluated.
    pub session_id: String,
    /// Total number of thoughts in the session.
    pub total_thoughts: usize,
    /// Average confidence across all thoughts (0.0-1.0).
    pub average_confidence: f64,
    /// Distribution of thoughts by reasoning mode.
    pub mode_distribution: std::collections::HashMap<String, usize>,
    /// Coherence score measuring logical consistency (0.0-1.0).
    pub coherence_score: f64,
    /// Recommendation for improving reasoning quality.
    pub recommendation: String,
}

impl ReflectionParams {
    /// Create new params with thought ID
    pub fn for_thought(thought_id: impl Into<String>) -> Self {
        Self {
            thought_id: Some(thought_id.into()),
            content: None,
            session_id: None,
            branch_id: None,
            max_iterations: default_max_iterations(),
            quality_threshold: default_quality_threshold(),
            include_chain: false,
        }
    }

    /// Create new params with content
    pub fn for_content(content: impl Into<String>) -> Self {
        Self {
            thought_id: None,
            content: Some(content.into()),
            session_id: None,
            branch_id: None,
            max_iterations: default_max_iterations(),
            quality_threshold: default_quality_threshold(),
            include_chain: false,
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

    /// Set the maximum iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max.clamp(1, 5);
        self
    }

    /// Set the quality threshold
    pub fn with_quality_threshold(mut self, threshold: f64) -> Self {
        self.quality_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Include reasoning chain context
    pub fn with_chain(mut self) -> Self {
        self.include_chain = true;
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
    fn test_default_max_iterations() {
        assert_eq!(default_max_iterations(), 3);
    }

    #[test]
    fn test_default_quality_threshold() {
        assert_eq!(default_quality_threshold(), 0.8);
    }

    // ============================================================================
    // ReflectionParams Tests
    // ============================================================================

    #[test]
    fn test_reflection_params_for_thought() {
        let params = ReflectionParams::for_thought("thought-123");
        assert_eq!(params.thought_id, Some("thought-123".to_string()));
        assert!(params.content.is_none());
        assert!(params.session_id.is_none());
        assert!(params.branch_id.is_none());
        assert_eq!(params.max_iterations, 3);
        assert_eq!(params.quality_threshold, 0.8);
        assert!(!params.include_chain);
    }

    #[test]
    fn test_reflection_params_for_content() {
        let params = ReflectionParams::for_content("Some content to reflect upon");
        assert!(params.thought_id.is_none());
        assert_eq!(
            params.content,
            Some("Some content to reflect upon".to_string())
        );
        assert!(params.session_id.is_none());
        assert_eq!(params.max_iterations, 3);
    }

    #[test]
    fn test_reflection_params_with_session() {
        let params = ReflectionParams::for_thought("t-1").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_reflection_params_with_branch() {
        let params = ReflectionParams::for_thought("t-1").with_branch("branch-456");
        assert_eq!(params.branch_id, Some("branch-456".to_string()));
    }

    #[test]
    fn test_reflection_params_with_max_iterations() {
        let params = ReflectionParams::for_thought("t-1").with_max_iterations(4);
        assert_eq!(params.max_iterations, 4);
    }

    #[test]
    fn test_reflection_params_max_iterations_clamped_high() {
        let params = ReflectionParams::for_thought("t-1").with_max_iterations(10);
        assert_eq!(params.max_iterations, 5); // max is 5
    }

    #[test]
    fn test_reflection_params_max_iterations_clamped_low() {
        let params = ReflectionParams::for_thought("t-1").with_max_iterations(0);
        assert_eq!(params.max_iterations, 1); // min is 1
    }

    #[test]
    fn test_reflection_params_with_quality_threshold() {
        let params = ReflectionParams::for_thought("t-1").with_quality_threshold(0.9);
        assert_eq!(params.quality_threshold, 0.9);
    }

    #[test]
    fn test_reflection_params_quality_threshold_clamped_high() {
        let params = ReflectionParams::for_thought("t-1").with_quality_threshold(1.5);
        assert_eq!(params.quality_threshold, 1.0);
    }

    #[test]
    fn test_reflection_params_quality_threshold_clamped_low() {
        let params = ReflectionParams::for_thought("t-1").with_quality_threshold(-0.5);
        assert_eq!(params.quality_threshold, 0.0);
    }

    #[test]
    fn test_reflection_params_with_chain() {
        let params = ReflectionParams::for_thought("t-1").with_chain();
        assert!(params.include_chain);
    }

    #[test]
    fn test_reflection_params_builder_chain() {
        let params = ReflectionParams::for_thought("t-1")
            .with_session("my-session")
            .with_branch("my-branch")
            .with_max_iterations(4)
            .with_quality_threshold(0.85)
            .with_chain();

        assert_eq!(params.thought_id, Some("t-1".to_string()));
        assert_eq!(params.session_id, Some("my-session".to_string()));
        assert_eq!(params.branch_id, Some("my-branch".to_string()));
        assert_eq!(params.max_iterations, 4);
        assert_eq!(params.quality_threshold, 0.85);
        assert!(params.include_chain);
    }

    #[test]
    fn test_reflection_params_serialize() {
        let params = ReflectionParams::for_thought("t-1")
            .with_session("sess-1")
            .with_max_iterations(3);

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("t-1"));
        assert!(json.contains("sess-1"));
        assert!(json.contains("\"max_iterations\":3"));
    }

    #[test]
    fn test_reflection_params_deserialize() {
        let json = r#"{
            "thought_id": "t-1",
            "content": "Some content",
            "session_id": "s-1",
            "branch_id": "b-1",
            "max_iterations": 4,
            "quality_threshold": 0.9,
            "include_chain": true
        }"#;
        let params: ReflectionParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.thought_id, Some("t-1".to_string()));
        assert_eq!(params.content, Some("Some content".to_string()));
        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.branch_id, Some("b-1".to_string()));
        assert_eq!(params.max_iterations, 4);
        assert_eq!(params.quality_threshold, 0.9);
        assert!(params.include_chain);
    }

    #[test]
    fn test_reflection_params_deserialize_minimal() {
        let json = r#"{"thought_id": "t-1"}"#;
        let params: ReflectionParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.thought_id, Some("t-1".to_string()));
        assert!(params.content.is_none());
        assert!(params.session_id.is_none());
        assert_eq!(params.max_iterations, 3); // default
        assert_eq!(params.quality_threshold, 0.8); // default
        assert!(!params.include_chain); // default
    }

    // ============================================================================
    // ReflectionResponse Tests
    // ============================================================================

    #[test]
    fn test_reflection_response_serialize() {
        let response = ReflectionResponse {
            analysis: "This is the analysis".to_string(),
            strengths: vec!["Strength 1".to_string(), "Strength 2".to_string()],
            weaknesses: vec!["Weakness 1".to_string()],
            recommendations: vec!["Recommendation 1".to_string()],
            confidence: 0.85,
            quality_score: Some(0.9),
            improved_thought: Some("Improved version".to_string()),
            metadata: serde_json::json!({"key": "value"}),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("This is the analysis"));
        assert!(json.contains("Strength 1"));
        assert!(json.contains("Weakness 1"));
        assert!(json.contains("0.85"));
        assert!(json.contains("0.9"));
        assert!(json.contains("Improved version"));
    }

    #[test]
    fn test_reflection_response_deserialize() {
        let json = r#"{
            "analysis": "Analysis text",
            "strengths": ["S1", "S2"],
            "weaknesses": ["W1"],
            "recommendations": ["R1", "R2"],
            "confidence": 0.75,
            "quality_score": 0.8,
            "improved_thought": "Better thought"
        }"#;
        let response: ReflectionResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.analysis, "Analysis text");
        assert_eq!(response.strengths.len(), 2);
        assert_eq!(response.weaknesses.len(), 1);
        assert_eq!(response.recommendations.len(), 2);
        assert_eq!(response.confidence, 0.75);
        assert_eq!(response.quality_score, Some(0.8));
        assert_eq!(
            response.improved_thought,
            Some("Better thought".to_string())
        );
    }

    #[test]
    fn test_reflection_response_deserialize_minimal() {
        let json = r#"{
            "analysis": "Basic analysis",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.5
        }"#;
        let response: ReflectionResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.analysis, "Basic analysis");
        assert!(response.strengths.is_empty());
        assert!(response.quality_score.is_none());
        assert!(response.improved_thought.is_none());
    }

    // ============================================================================
    // ImprovedThought Tests
    // ============================================================================

    #[test]
    fn test_improved_thought_serialize() {
        let improved = ImprovedThought {
            thought_id: "t-improved".to_string(),
            content: "Improved content".to_string(),
            confidence: 0.92,
        };

        let json = serde_json::to_string(&improved).unwrap();
        assert!(json.contains("t-improved"));
        assert!(json.contains("Improved content"));
        assert!(json.contains("0.92"));
    }

    #[test]
    fn test_improved_thought_deserialize() {
        let json = r#"{
            "thought_id": "t-1",
            "content": "Better version",
            "confidence": 0.88
        }"#;
        let improved: ImprovedThought = serde_json::from_str(json).unwrap();

        assert_eq!(improved.thought_id, "t-1");
        assert_eq!(improved.content, "Better version");
        assert_eq!(improved.confidence, 0.88);
    }

    // ============================================================================
    // ReflectionResult Tests
    // ============================================================================

    #[test]
    fn test_reflection_result_serialize() {
        let result = ReflectionResult {
            session_id: "sess-1".to_string(),
            reflection_thought_id: "t-refl".to_string(),
            original_thought_id: Some("t-orig".to_string()),
            analysis: "Analysis of thought".to_string(),
            strengths: vec!["Strong point".to_string()],
            weaknesses: vec!["Weak point".to_string()],
            recommendations: vec!["Suggestion".to_string()],
            quality_score: 0.85,
            improved_thought: Some(ImprovedThought {
                thought_id: "t-improved".to_string(),
                content: "Better".to_string(),
                confidence: 0.9,
            }),
            iterations_performed: 2,
            quality_improved: true,
            branch_id: Some("branch-1".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("sess-1"));
        assert!(json.contains("t-refl"));
        assert!(json.contains("t-orig"));
        assert!(json.contains("Strong point"));
        assert!(json.contains("branch-1"));
    }

    #[test]
    fn test_reflection_result_deserialize() {
        let json = r#"{
            "session_id": "s-1",
            "reflection_thought_id": "t-r",
            "original_thought_id": "t-o",
            "analysis": "Analysis",
            "strengths": ["S"],
            "weaknesses": ["W"],
            "recommendations": ["R"],
            "quality_score": 0.8,
            "improved_thought": null,
            "iterations_performed": 3,
            "quality_improved": false
        }"#;
        let result: ReflectionResult = serde_json::from_str(json).unwrap();

        assert_eq!(result.session_id, "s-1");
        assert_eq!(result.reflection_thought_id, "t-r");
        assert_eq!(result.original_thought_id, Some("t-o".to_string()));
        assert_eq!(result.quality_score, 0.8);
        assert!(result.improved_thought.is_none());
        assert_eq!(result.iterations_performed, 3);
        assert!(!result.quality_improved);
        assert!(result.branch_id.is_none());
    }

    #[test]
    fn test_reflection_result_without_branch() {
        let result = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-1".to_string(),
            original_thought_id: None,
            analysis: "No branch".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            quality_score: 0.5,
            improved_thought: None,
            iterations_performed: 1,
            quality_improved: false,
            branch_id: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        // branch_id should be omitted due to skip_serializing_if
        assert!(!json.contains("branch_id"));
    }

    // ============================================================================
    // SessionEvaluation Tests
    // ============================================================================

    #[test]
    fn test_session_evaluation_serialize() {
        let mut mode_dist = std::collections::HashMap::new();
        mode_dist.insert("linear".to_string(), 5);
        mode_dist.insert("tree".to_string(), 3);

        let eval = SessionEvaluation {
            session_id: "sess-eval".to_string(),
            total_thoughts: 8,
            average_confidence: 0.75,
            mode_distribution: mode_dist,
            coherence_score: 0.82,
            recommendation: "Reasoning quality is acceptable".to_string(),
        };

        let json = serde_json::to_string(&eval).unwrap();
        assert!(json.contains("sess-eval"));
        assert!(json.contains("8"));
        assert!(json.contains("0.75"));
        assert!(json.contains("linear"));
        assert!(json.contains("0.82"));
    }

    #[test]
    fn test_session_evaluation_deserialize() {
        let json = r#"{
            "session_id": "s-1",
            "total_thoughts": 10,
            "average_confidence": 0.7,
            "mode_distribution": {"linear": 6, "divergent": 4},
            "coherence_score": 0.9,
            "recommendation": "Good quality"
        }"#;
        let eval: SessionEvaluation = serde_json::from_str(json).unwrap();

        assert_eq!(eval.session_id, "s-1");
        assert_eq!(eval.total_thoughts, 10);
        assert_eq!(eval.average_confidence, 0.7);
        assert_eq!(eval.mode_distribution.get("linear"), Some(&6));
        assert_eq!(eval.coherence_score, 0.9);
        assert_eq!(eval.recommendation, "Good quality");
    }

    // ============================================================================
    // Serialization Round-Trip Tests
    // ============================================================================

    #[test]
    fn test_reflection_params_roundtrip() {
        let original = ReflectionParams::for_thought("t-1")
            .with_session("sess-1")
            .with_branch("branch-1")
            .with_max_iterations(4)
            .with_quality_threshold(0.85)
            .with_chain();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ReflectionParams = serde_json::from_str(&json).unwrap();

        assert_eq!(original.thought_id, deserialized.thought_id);
        assert_eq!(original.session_id, deserialized.session_id);
        assert_eq!(original.branch_id, deserialized.branch_id);
        assert_eq!(original.max_iterations, deserialized.max_iterations);
        assert_eq!(original.quality_threshold, deserialized.quality_threshold);
        assert_eq!(original.include_chain, deserialized.include_chain);
    }

    #[test]
    fn test_reflection_response_roundtrip() {
        let original = ReflectionResponse {
            analysis: "Deep analysis".to_string(),
            strengths: vec!["S1".to_string(), "S2".to_string()],
            weaknesses: vec!["W1".to_string()],
            recommendations: vec!["R1".to_string(), "R2".to_string()],
            confidence: 0.87,
            quality_score: Some(0.92),
            improved_thought: Some("Improved version".to_string()),
            metadata: serde_json::json!({"extra": "data"}),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ReflectionResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(original.analysis, deserialized.analysis);
        assert_eq!(original.strengths, deserialized.strengths);
        assert_eq!(original.weaknesses, deserialized.weaknesses);
        assert_eq!(original.recommendations, deserialized.recommendations);
        assert_eq!(original.confidence, deserialized.confidence);
        assert_eq!(original.quality_score, deserialized.quality_score);
        assert_eq!(original.improved_thought, deserialized.improved_thought);
    }

    #[test]
    fn test_reflection_result_roundtrip() {
        let original = ReflectionResult {
            session_id: "s-123".to_string(),
            reflection_thought_id: "t-refl".to_string(),
            original_thought_id: Some("t-orig".to_string()),
            analysis: "Complete analysis".to_string(),
            strengths: vec!["Strength".to_string()],
            weaknesses: vec!["Weakness".to_string()],
            recommendations: vec!["Recommendation".to_string()],
            quality_score: 0.88,
            improved_thought: Some(ImprovedThought {
                thought_id: "t-imp".to_string(),
                content: "Better".to_string(),
                confidence: 0.91,
            }),
            iterations_performed: 3,
            quality_improved: true,
            branch_id: Some("br-1".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ReflectionResult = serde_json::from_str(&json).unwrap();

        assert_eq!(original.session_id, deserialized.session_id);
        assert_eq!(
            original.reflection_thought_id,
            deserialized.reflection_thought_id
        );
        assert_eq!(
            original.original_thought_id,
            deserialized.original_thought_id
        );
        assert_eq!(original.quality_score, deserialized.quality_score);
        assert_eq!(
            original.iterations_performed,
            deserialized.iterations_performed
        );
        assert_eq!(original.quality_improved, deserialized.quality_improved);
        assert_eq!(original.branch_id, deserialized.branch_id);
    }

    #[test]
    fn test_session_evaluation_roundtrip() {
        let mut mode_dist = std::collections::HashMap::new();
        mode_dist.insert("linear".to_string(), 7);
        mode_dist.insert("tree".to_string(), 3);
        mode_dist.insert("reflection".to_string(), 2);

        let original = SessionEvaluation {
            session_id: "eval-session".to_string(),
            total_thoughts: 12,
            average_confidence: 0.73,
            mode_distribution: mode_dist,
            coherence_score: 0.85,
            recommendation: "Continue with current approach".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SessionEvaluation = serde_json::from_str(&json).unwrap();

        assert_eq!(original.session_id, deserialized.session_id);
        assert_eq!(original.total_thoughts, deserialized.total_thoughts);
        assert_eq!(original.average_confidence, deserialized.average_confidence);
        assert_eq!(original.coherence_score, deserialized.coherence_score);
        assert_eq!(original.recommendation, deserialized.recommendation);
        assert_eq!(
            original.mode_distribution.len(),
            deserialized.mode_distribution.len()
        );
    }

    // ============================================================================
    // Edge Cases Tests
    // ============================================================================

    #[test]
    fn test_reflection_params_empty_strings() {
        let params = ReflectionParams::for_content("");
        assert_eq!(params.content, Some("".to_string()));
    }

    #[test]
    fn test_reflection_params_builder_empty_session() {
        let params = ReflectionParams::for_thought("t-1").with_session("");
        assert_eq!(params.session_id, Some("".to_string()));
    }

    #[test]
    fn test_reflection_params_builder_empty_branch() {
        let params = ReflectionParams::for_thought("t-1").with_branch("");
        assert_eq!(params.branch_id, Some("".to_string()));
    }

    #[test]
    fn test_reflection_params_max_iterations_boundary() {
        // Test boundary values
        let params1 = ReflectionParams::for_thought("t-1").with_max_iterations(1);
        assert_eq!(params1.max_iterations, 1);

        let params5 = ReflectionParams::for_thought("t-1").with_max_iterations(5);
        assert_eq!(params5.max_iterations, 5);
    }

    #[test]
    fn test_reflection_params_quality_threshold_boundary() {
        // Test boundary values
        let params0 = ReflectionParams::for_thought("t-1").with_quality_threshold(0.0);
        assert_eq!(params0.quality_threshold, 0.0);

        let params1 = ReflectionParams::for_thought("t-1").with_quality_threshold(1.0);
        assert_eq!(params1.quality_threshold, 1.0);
    }

    #[test]
    fn test_reflection_response_empty_arrays() {
        let response = ReflectionResponse {
            analysis: "Analysis".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            confidence: 0.5,
            quality_score: None,
            improved_thought: None,
            metadata: serde_json::Value::Null,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ReflectionResponse = serde_json::from_str(&json).unwrap();

        assert!(deserialized.strengths.is_empty());
        assert!(deserialized.weaknesses.is_empty());
        assert!(deserialized.recommendations.is_empty());
    }

    #[test]
    fn test_improved_thought_roundtrip() {
        let original = ImprovedThought {
            thought_id: "imp-123".to_string(),
            content: "Significantly improved reasoning".to_string(),
            confidence: 0.95,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ImprovedThought = serde_json::from_str(&json).unwrap();

        assert_eq!(original.thought_id, deserialized.thought_id);
        assert_eq!(original.content, deserialized.content);
        assert_eq!(original.confidence, deserialized.confidence);
    }

    #[test]
    fn test_reflection_params_deserialize_with_defaults() {
        // Test that missing fields get default values
        let json = r#"{"content": "Test content"}"#;
        let params: ReflectionParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.max_iterations, 3);
        assert_eq!(params.quality_threshold, 0.8);
        assert!(!params.include_chain);
    }

    #[test]
    fn test_reflection_response_quality_score_none() {
        let json = r#"{
            "analysis": "Test",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.6
        }"#;
        let response: ReflectionResponse = serde_json::from_str(json).unwrap();

        assert!(response.quality_score.is_none());
        assert!(response.improved_thought.is_none());
    }

    #[test]
    fn test_reflection_params_very_long_content() {
        let long_content = "x".repeat(10000);
        let params = ReflectionParams::for_content(&long_content);
        assert_eq!(params.content.as_ref().unwrap().len(), 10000);
    }

    #[test]
    fn test_reflection_params_special_characters() {
        let special = "Test with \n newlines \t tabs and \"quotes\" and 'apostrophes'";
        let params = ReflectionParams::for_content(special);
        assert_eq!(params.content, Some(special.to_string()));
    }

    #[test]
    fn test_reflection_params_unicode() {
        let unicode = "Unicode test: 你好世界 🌍 مرحبا";
        let params = ReflectionParams::for_content(unicode);
        assert_eq!(params.content, Some(unicode.to_string()));
    }

    #[test]
    fn test_reflection_result_none_values() {
        let result = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-r".to_string(),
            original_thought_id: None,
            analysis: "Analysis".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            quality_score: 0.5,
            improved_thought: None,
            iterations_performed: 1,
            quality_improved: false,
            branch_id: None,
        };

        assert!(result.original_thought_id.is_none());
        assert!(result.improved_thought.is_none());
        assert!(result.branch_id.is_none());
    }

    #[test]
    fn test_session_evaluation_empty_mode_distribution() {
        let eval = SessionEvaluation {
            session_id: "s-1".to_string(),
            total_thoughts: 0,
            average_confidence: 0.0,
            mode_distribution: std::collections::HashMap::new(),
            coherence_score: 0.0,
            recommendation: "No data".to_string(),
        };

        assert!(eval.mode_distribution.is_empty());
        assert_eq!(eval.total_thoughts, 0);
    }

    // ============================================================================
    // Additional Integration Tests (without build_messages - tested via integration tests)
    // ============================================================================

    #[test]
    fn test_default_values_consistency() {
        // Ensure default functions match struct defaults
        let params_thought = ReflectionParams::for_thought("t-1");
        let params_content = ReflectionParams::for_content("content");

        assert_eq!(params_thought.max_iterations, default_max_iterations());
        assert_eq!(
            params_thought.quality_threshold,
            default_quality_threshold()
        );
        assert_eq!(params_content.max_iterations, default_max_iterations());
        assert_eq!(
            params_content.quality_threshold,
            default_quality_threshold()
        );
    }

    #[test]
    fn test_clamp_values_negative() {
        // Test that negative values are properly clamped
        let params = ReflectionParams::for_thought("t-1")
            .with_max_iterations(0)
            .with_quality_threshold(-1.0);

        assert_eq!(params.max_iterations, 1);
        assert_eq!(params.quality_threshold, 0.0);
    }

    #[test]
    fn test_clamp_values_very_high() {
        // Test that very high values are properly clamped
        let params = ReflectionParams::for_thought("t-1")
            .with_max_iterations(100)
            .with_quality_threshold(5.0);

        assert_eq!(params.max_iterations, 5);
        assert_eq!(params.quality_threshold, 1.0);
    }

    #[test]
    fn test_reflection_result_quality_improved_logic() {
        // Test different quality_improved scenarios
        let result_improved = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-1".to_string(),
            original_thought_id: Some("t-orig".to_string()),
            analysis: "A".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            quality_score: 0.9,
            improved_thought: None,
            iterations_performed: 1,
            quality_improved: true,
            branch_id: None,
        };

        assert!(result_improved.quality_improved);
        assert!(result_improved.quality_score > 0.5);
    }

    // ============================================================================
    // Additional Builder Pattern Tests
    // ============================================================================

    #[test]
    fn test_reflection_params_multiple_with_calls() {
        let params = ReflectionParams::for_thought("t-1")
            .with_session("s1")
            .with_session("s2") // Should override
            .with_max_iterations(2)
            .with_max_iterations(4); // Should override

        assert_eq!(params.session_id, Some("s2".to_string()));
        assert_eq!(params.max_iterations, 4);
    }

    #[test]
    fn test_reflection_params_content_builder_full_chain() {
        let params = ReflectionParams::for_content("My content")
            .with_session("session-abc")
            .with_branch("branch-xyz")
            .with_max_iterations(2)
            .with_quality_threshold(0.75)
            .with_chain();

        assert!(params.thought_id.is_none());
        assert_eq!(params.content, Some("My content".to_string()));
        assert_eq!(params.session_id, Some("session-abc".to_string()));
        assert_eq!(params.branch_id, Some("branch-xyz".to_string()));
        assert_eq!(params.max_iterations, 2);
        assert_eq!(params.quality_threshold, 0.75);
        assert!(params.include_chain);
    }

    #[test]
    fn test_reflection_params_default_include_chain_false() {
        let params1 = ReflectionParams::for_thought("t-1");
        let params2 = ReflectionParams::for_content("content");

        assert!(!params1.include_chain);
        assert!(!params2.include_chain);
    }

    #[test]
    fn test_reflection_response_metadata_default() {
        let json = r#"{
            "analysis": "Test",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.5
        }"#;
        let response: ReflectionResponse = serde_json::from_str(json).unwrap();

        // metadata should default to null
        assert_eq!(response.metadata, serde_json::Value::Null);
    }

    #[test]
    fn test_reflection_params_skip_serializing_none() {
        let params = ReflectionParams::for_content("content");
        let json = serde_json::to_string(&params).unwrap();

        // Fields that are None should be omitted
        assert!(!json.contains("thought_id"));
        assert!(!json.contains("session_id"));
        assert!(!json.contains("branch_id"));
    }

    #[test]
    fn test_reflection_result_skip_serializing_branch_none() {
        let result = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-1".to_string(),
            original_thought_id: None,
            analysis: "A".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            quality_score: 0.5,
            improved_thought: None,
            iterations_performed: 1,
            quality_improved: false,
            branch_id: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("branch_id"));
    }

    #[test]
    fn test_reflection_result_with_all_fields() {
        let result = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-refl".to_string(),
            original_thought_id: Some("t-orig".to_string()),
            analysis: "Full analysis".to_string(),
            strengths: vec!["S1".to_string(), "S2".to_string()],
            weaknesses: vec!["W1".to_string()],
            recommendations: vec!["R1".to_string(), "R2".to_string(), "R3".to_string()],
            quality_score: 0.88,
            improved_thought: Some(ImprovedThought {
                thought_id: "t-imp".to_string(),
                content: "Improved".to_string(),
                confidence: 0.92,
            }),
            iterations_performed: 4,
            quality_improved: true,
            branch_id: Some("b-1".to_string()),
        };

        assert_eq!(result.strengths.len(), 2);
        assert_eq!(result.weaknesses.len(), 1);
        assert_eq!(result.recommendations.len(), 3);
        assert!(result.improved_thought.is_some());
        assert!(result.branch_id.is_some());
    }

    #[test]
    fn test_session_evaluation_single_mode() {
        let mut mode_dist = std::collections::HashMap::new();
        mode_dist.insert("reflection".to_string(), 1);

        let eval = SessionEvaluation {
            session_id: "s-1".to_string(),
            total_thoughts: 1,
            average_confidence: 0.9,
            mode_distribution: mode_dist,
            coherence_score: 1.0,
            recommendation: "Single thought".to_string(),
        };

        assert_eq!(eval.mode_distribution.len(), 1);
        assert_eq!(eval.total_thoughts, 1);
    }

    #[test]
    fn test_improved_thought_zero_confidence() {
        let improved = ImprovedThought {
            thought_id: "t-1".to_string(),
            content: "Low confidence improvement".to_string(),
            confidence: 0.0,
        };

        assert_eq!(improved.confidence, 0.0);
    }

    #[test]
    fn test_improved_thought_full_confidence() {
        let improved = ImprovedThought {
            thought_id: "t-1".to_string(),
            content: "Perfect improvement".to_string(),
            confidence: 1.0,
        };

        assert_eq!(improved.confidence, 1.0);
    }

    #[test]
    fn test_reflection_response_with_complex_metadata() {
        let response = ReflectionResponse {
            analysis: "Complex".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            confidence: 0.5,
            quality_score: None,
            improved_thought: None,
            metadata: serde_json::json!({
                "nested": {
                    "field": "value",
                    "array": [1, 2, 3]
                },
                "boolean": true,
                "number": 42
            }),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ReflectionResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(response.metadata["nested"]["field"], "value");
        assert_eq!(deserialized.metadata["boolean"], true);
    }

    // ============================================================================
    // ReflectionMode Constructor Tests
    // ============================================================================

    fn create_test_config() -> Config {
        use crate::config::{DatabaseConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig};
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
        }
    }

    #[test]
    fn test_reflection_mode_new() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, config.pipes.reflection);
    }

    #[test]
    fn test_reflection_mode_new_with_custom_pipe_name() {
        let mut config = create_test_config();
        config.pipes.reflection = "custom-reflection-pipe".to_string();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        assert_eq!(mode.pipe_name, "custom-reflection-pipe");
    }

    // ============================================================================
    // build_messages() Tests
    // ============================================================================

    #[test]
    fn test_build_messages_simple_content() {
        use crate::langbase::MessageRole;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let messages = mode.build_messages("Test content", &[], 0);

        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(messages[0].content.contains("meta-cognitive"));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert!(messages[1].content.contains("Test content"));
    }

    #[test]
    fn test_build_messages_with_iteration() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let messages = mode.build_messages("Test content", &[], 2);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("iteration 3"));
        assert!(messages[0]
            .content
            .contains("previously identified weaknesses"));
    }

    #[test]
    fn test_build_messages_with_empty_chain() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let chain: Vec<Thought> = vec![];
        let messages = mode.build_messages("Content", &chain, 0);

        assert_eq!(messages.len(), 2);
        assert!(!messages
            .iter()
            .any(|m| m.content.contains("Reasoning chain")));
    }

    #[test]
    fn test_build_messages_with_reasoning_chain() {
        use crate::langbase::MessageRole;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First thought", "linear").with_confidence(0.7);
        let thought2 = Thought::new("sess-1", "Second thought", "tree").with_confidence(0.8);
        let chain = vec![thought1, thought2];

        let messages = mode.build_messages("Final thought", &chain, 0);

        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0].role, MessageRole::System));
        assert!(matches!(messages[1].role, MessageRole::User));
        assert!(messages[1].content.contains("Reasoning chain"));
        assert!(messages[1].content.contains("First thought"));
        assert!(messages[1].content.contains("Second thought"));
        assert!(messages[1].content.contains("0.70"));
        assert!(messages[1].content.contains("0.80"));
        assert!(matches!(messages[2].role, MessageRole::User));
        assert!(messages[2].content.contains("Final thought"));
    }

    #[test]
    fn test_build_messages_chain_formatting() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought = Thought::new("sess-1", "Content", "divergent").with_confidence(0.65);
        let chain = vec![thought];

        let messages = mode.build_messages("Test", &chain, 0);

        let chain_message = &messages[1];
        assert!(chain_message.content.contains("[divergent]"));
        assert!(chain_message.content.contains("confidence: 0.65"));
        assert!(chain_message.content.contains("Content"));
    }

    #[test]
    fn test_build_messages_empty_content() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let messages = mode.build_messages("", &[], 0);

        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("Thought to reflect upon"));
    }

    #[test]
    fn test_build_messages_unicode_content() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let unicode_content = "Test with unicode: 你好 🌍 مرحبا";
        let messages = mode.build_messages(unicode_content, &[], 0);

        assert!(messages[1].content.contains(unicode_content));
    }

    #[test]
    fn test_build_messages_special_characters() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let special = "Content with\nnewlines\tand\ttabs and \"quotes\"";
        let messages = mode.build_messages(special, &[], 0);

        assert!(messages[1].content.contains(special));
    }

    #[test]
    fn test_build_messages_large_chain() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let mut chain = Vec::new();
        for i in 0..5 {
            chain.push(Thought::new("sess-1", &format!("Thought {}", i), "linear"));
        }

        let messages = mode.build_messages("Final", &chain, 0);

        assert_eq!(messages.len(), 3);
        let chain_msg = &messages[1];
        assert!(chain_msg.content.contains("Thought 0"));
        assert!(chain_msg.content.contains("Thought 4"));
    }

    // ============================================================================
    // parse_response() Tests
    // ============================================================================

    #[test]
    fn test_parse_response_valid_json() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "This is the analysis",
            "strengths": ["Strength 1", "Strength 2"],
            "weaknesses": ["Weakness 1"],
            "recommendations": ["Recommendation 1"],
            "confidence": 0.85
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert_eq!(response.analysis, "This is the analysis");
        assert_eq!(response.strengths.len(), 2);
        assert_eq!(response.weaknesses.len(), 1);
        assert_eq!(response.recommendations.len(), 1);
        assert_eq!(response.confidence, 0.85);
    }

    #[test]
    fn test_parse_response_with_quality_score() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "Analysis",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.7,
            "quality_score": 0.9
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert_eq!(response.quality_score, Some(0.9));
    }

    #[test]
    fn test_parse_response_with_improved_thought() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "Analysis",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.8,
            "improved_thought": "This is the improved version"
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert_eq!(
            response.improved_thought,
            Some("This is the improved version".to_string())
        );
    }

    #[test]
    fn test_parse_response_with_metadata() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "Analysis",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.6,
            "metadata": {"key": "value", "number": 42}
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert_eq!(response.metadata["key"], "value");
        assert_eq!(response.metadata["number"], 42);
    }

    #[test]
    fn test_parse_response_json_with_markdown() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"Here's my analysis:

```json
{
    "analysis": "Extracted from markdown",
    "strengths": ["S1"],
    "weaknesses": ["W1"],
    "recommendations": ["R1"],
    "confidence": 0.75
}
```"#;

        let response = mode.parse_response(completion).unwrap();
        assert_eq!(response.analysis, "Extracted from markdown");
        assert_eq!(response.confidence, 0.75);
    }

    #[test]
    fn test_parse_response_invalid_json() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = "This is not JSON at all";
        let result = mode.parse_response(completion);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_missing_required_fields() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{"analysis": "Missing other fields"}"#;
        let result = mode.parse_response(completion);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_empty_arrays() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "Test",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.5
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert!(response.strengths.is_empty());
        assert!(response.weaknesses.is_empty());
        assert!(response.recommendations.is_empty());
    }

    #[test]
    fn test_parse_response_unicode_in_analysis() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "Unicode: 你好世界 🌍",
            "strengths": [],
            "weaknesses": [],
            "recommendations": [],
            "confidence": 0.5
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert!(response.analysis.contains("你好世界"));
        assert!(response.analysis.contains("🌍"));
    }

    // ============================================================================
    // calculate_coherence() Tests
    // ============================================================================

    #[test]
    fn test_calculate_coherence_empty_thoughts() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let thoughts: Vec<Thought> = vec![];
        let coherence = mode.calculate_coherence(&thoughts);

        // Empty array should have perfect coherence (no inconsistency)
        assert_eq!(coherence, 1.0);
    }

    #[test]
    fn test_calculate_coherence_single_thought() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let thought = Thought::new("sess-1", "Single thought", "linear");
        let coherence = mode.calculate_coherence(&[thought]);

        assert_eq!(coherence, 1.0);
    }

    #[test]
    fn test_calculate_coherence_all_linked() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.8);
        let thought2 = Thought::new("sess-1", "Second", "linear")
            .with_confidence(0.8)
            .with_parent(&thought1.id);
        let thought3 = Thought::new("sess-1", "Third", "linear")
            .with_confidence(0.8)
            .with_parent(&thought2.id);

        let coherence = mode.calculate_coherence(&[thought1, thought2, thought3]);

        // All linked + stable confidence = high coherence
        assert!(coherence > 0.9);
    }

    #[test]
    fn test_calculate_coherence_no_links() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.8);
        let thought2 = Thought::new("sess-1", "Second", "linear").with_confidence(0.8);
        let thought3 = Thought::new("sess-1", "Third", "linear").with_confidence(0.8);

        let coherence = mode.calculate_coherence(&[thought1, thought2, thought3]);

        // No links = poor link ratio, but stable confidence helps
        assert!(coherence < 0.7);
    }

    #[test]
    fn test_calculate_coherence_unstable_confidence() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.9);
        let thought2 = Thought::new("sess-1", "Second", "linear").with_confidence(0.1);
        let thought3 = Thought::new("sess-1", "Third", "linear").with_confidence(0.9);

        let coherence = mode.calculate_coherence(&[thought1, thought2, thought3]);

        // Large confidence swings reduce coherence
        assert!(coherence < 0.7);
    }

    #[test]
    fn test_calculate_coherence_partial_links() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.7);
        let thought2 = Thought::new("sess-1", "Second", "linear")
            .with_confidence(0.75)
            .with_parent(&thought1.id);
        let thought3 = Thought::new("sess-1", "Third", "linear").with_confidence(0.7);

        let coherence = mode.calculate_coherence(&[thought1, thought2, thought3]);

        // 50% linked, stable confidence = medium coherence
        assert!(coherence > 0.5 && coherence < 0.9);
    }

    #[test]
    fn test_calculate_coherence_two_thoughts_linked() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.8);
        let thought2 = Thought::new("sess-1", "Second", "linear")
            .with_confidence(0.8)
            .with_parent(&thought1.id);

        let coherence = mode.calculate_coherence(&[thought1, thought2]);

        // Perfect link ratio, stable confidence
        assert!(coherence > 0.9);
    }

    #[test]
    fn test_calculate_coherence_two_thoughts_unlinked() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.8);
        let thought2 = Thought::new("sess-1", "Second", "linear").with_confidence(0.8);

        let coherence = mode.calculate_coherence(&[thought1, thought2]);

        // No links but stable confidence
        assert!(coherence < 0.7);
    }

    #[test]
    fn test_calculate_coherence_varying_confidence() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let thought1 = Thought::new("sess-1", "First", "linear").with_confidence(0.5);
        let thought2 = Thought::new("sess-1", "Second", "linear")
            .with_confidence(0.6)
            .with_parent(&thought1.id);
        let thought3 = Thought::new("sess-1", "Third", "linear")
            .with_confidence(0.7)
            .with_parent(&thought2.id);

        let coherence = mode.calculate_coherence(&[thought1, thought2, thought3]);

        // Good links, gradually improving confidence
        assert!(coherence > 0.8);
    }

    // ============================================================================
    // Additional Edge Case Tests
    // ============================================================================

    #[test]
    fn test_reflection_params_for_thought_with_empty_id() {
        let params = ReflectionParams::for_thought("");
        assert_eq!(params.thought_id, Some("".to_string()));
    }

    #[test]
    fn test_reflection_params_for_content_with_whitespace() {
        let params = ReflectionParams::for_content("   \n\t  ");
        assert_eq!(params.content, Some("   \n\t  ".to_string()));
    }

    #[test]
    fn test_reflection_response_zero_confidence() {
        let response = ReflectionResponse {
            analysis: "Low confidence".to_string(),
            strengths: vec![],
            weaknesses: vec!["Many issues".to_string()],
            recommendations: vec![],
            confidence: 0.0,
            quality_score: Some(0.0),
            improved_thought: None,
            metadata: serde_json::Value::Null,
        };

        assert_eq!(response.confidence, 0.0);
        assert_eq!(response.quality_score, Some(0.0));
    }

    #[test]
    fn test_reflection_response_perfect_confidence() {
        let response = ReflectionResponse {
            analysis: "Perfect".to_string(),
            strengths: vec!["Everything".to_string()],
            weaknesses: vec![],
            recommendations: vec![],
            confidence: 1.0,
            quality_score: Some(1.0),
            improved_thought: None,
            metadata: serde_json::Value::Null,
        };

        assert_eq!(response.confidence, 1.0);
        assert_eq!(response.quality_score, Some(1.0));
    }

    #[test]
    fn test_reflection_result_iterations_zero() {
        let result = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-1".to_string(),
            original_thought_id: None,
            analysis: "Quick".to_string(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            quality_score: 0.95,
            improved_thought: None,
            iterations_performed: 0,
            quality_improved: true,
            branch_id: None,
        };

        assert_eq!(result.iterations_performed, 0);
    }

    #[test]
    fn test_session_evaluation_high_confidence_low_coherence() {
        let mut mode_dist = std::collections::HashMap::new();
        mode_dist.insert("linear".to_string(), 5);

        let eval = SessionEvaluation {
            session_id: "s-1".to_string(),
            total_thoughts: 5,
            average_confidence: 0.9,
            mode_distribution: mode_dist,
            coherence_score: 0.3,
            recommendation: "Reasoning chain may have logical gaps - consider reflection mode"
                .to_string(),
        };

        assert!(eval.average_confidence > 0.6);
        assert!(eval.coherence_score < 0.5);
        assert!(eval.recommendation.contains("logical gaps"));
    }

    #[test]
    fn test_session_evaluation_low_confidence_high_coherence() {
        let mut mode_dist = std::collections::HashMap::new();
        mode_dist.insert("linear".to_string(), 3);

        let eval = SessionEvaluation {
            session_id: "s-1".to_string(),
            total_thoughts: 3,
            average_confidence: 0.4,
            mode_distribution: mode_dist,
            coherence_score: 0.9,
            recommendation: "Consider reviewing and refining low-confidence thoughts".to_string(),
        };

        assert!(eval.average_confidence < 0.6);
        assert!(eval.coherence_score > 0.5);
        assert!(eval.recommendation.contains("low-confidence"));
    }

    #[test]
    fn test_reflection_params_with_all_options_none() {
        let params = ReflectionParams {
            thought_id: None,
            content: None,
            session_id: None,
            branch_id: None,
            max_iterations: 1,
            quality_threshold: 0.5,
            include_chain: false,
        };

        assert!(params.thought_id.is_none());
        assert!(params.content.is_none());
        assert!(params.session_id.is_none());
        assert!(params.branch_id.is_none());
    }

    #[test]
    fn test_improved_thought_empty_content() {
        let improved = ImprovedThought {
            thought_id: "t-1".to_string(),
            content: "".to_string(),
            confidence: 0.5,
        };

        assert_eq!(improved.content, "");
    }

    #[test]
    fn test_reflection_response_large_arrays() {
        let strengths: Vec<String> = (0..100).map(|i| format!("Strength {}", i)).collect();
        let weaknesses: Vec<String> = (0..50).map(|i| format!("Weakness {}", i)).collect();
        let recommendations: Vec<String> = (0..75).map(|i| format!("Rec {}", i)).collect();

        let response = ReflectionResponse {
            analysis: "Large arrays".to_string(),
            strengths: strengths.clone(),
            weaknesses: weaknesses.clone(),
            recommendations: recommendations.clone(),
            confidence: 0.5,
            quality_score: None,
            improved_thought: None,
            metadata: serde_json::Value::Null,
        };

        assert_eq!(response.strengths.len(), 100);
        assert_eq!(response.weaknesses.len(), 50);
        assert_eq!(response.recommendations.len(), 75);
    }

    #[test]
    fn test_reflection_params_extreme_quality_threshold() {
        let params1 = ReflectionParams::for_thought("t-1").with_quality_threshold(-100.0);
        assert_eq!(params1.quality_threshold, 0.0);

        let params2 = ReflectionParams::for_thought("t-1").with_quality_threshold(100.0);
        assert_eq!(params2.quality_threshold, 1.0);
    }

    #[test]
    fn test_reflection_params_extreme_max_iterations() {
        let params1 = ReflectionParams::for_thought("t-1").with_max_iterations(usize::MAX);
        assert_eq!(params1.max_iterations, 5);

        let params2 = ReflectionParams::for_thought("t-1").with_max_iterations(0);
        assert_eq!(params2.max_iterations, 1);
    }

    #[test]
    fn test_session_evaluation_many_modes() {
        let mut mode_dist = std::collections::HashMap::new();
        mode_dist.insert("linear".to_string(), 10);
        mode_dist.insert("tree".to_string(), 8);
        mode_dist.insert("divergent".to_string(), 5);
        mode_dist.insert("reflection".to_string(), 3);
        mode_dist.insert("backtracking".to_string(), 2);

        let eval = SessionEvaluation {
            session_id: "s-complex".to_string(),
            total_thoughts: 28,
            average_confidence: 0.72,
            mode_distribution: mode_dist.clone(),
            coherence_score: 0.68,
            recommendation: "Reasoning quality is acceptable".to_string(),
        };

        assert_eq!(eval.mode_distribution.len(), 5);
        assert_eq!(eval.total_thoughts, 28);
        assert_eq!(eval.mode_distribution.get("linear"), Some(&10));
    }

    #[test]
    fn test_reflection_result_with_very_long_analysis() {
        let long_analysis = "a".repeat(10000);
        let result = ReflectionResult {
            session_id: "s-1".to_string(),
            reflection_thought_id: "t-1".to_string(),
            original_thought_id: None,
            analysis: long_analysis.clone(),
            strengths: vec![],
            weaknesses: vec![],
            recommendations: vec![],
            quality_score: 0.5,
            improved_thought: None,
            iterations_performed: 1,
            quality_improved: false,
            branch_id: None,
        };

        assert_eq!(result.analysis.len(), 10000);
    }

    #[test]
    fn test_parse_response_with_nested_json_arrays() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);

        let completion = r#"{
            "analysis": "Complex",
            "strengths": ["S1", "S2", "S3"],
            "weaknesses": ["W1", "W2"],
            "recommendations": ["R1", "R2", "R3", "R4"],
            "confidence": 0.65,
            "metadata": {
                "nested": {
                    "deep": {
                        "array": [1, 2, 3]
                    }
                }
            }
        }"#;

        let response = mode.parse_response(completion).unwrap();
        assert_eq!(response.strengths.len(), 3);
        assert_eq!(response.weaknesses.len(), 2);
        assert_eq!(response.recommendations.len(), 4);
        assert_eq!(response.metadata["nested"]["deep"]["array"][0], 1);
    }

    #[test]
    fn test_reflection_params_quality_threshold_precision() {
        let params = ReflectionParams::for_thought("t-1").with_quality_threshold(0.123456789);
        assert!((params.quality_threshold - 0.123456789).abs() < 1e-9);
    }

    #[test]
    fn test_build_messages_iteration_zero() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let messages = mode.build_messages("Content", &[], 0);

        assert!(!messages[0].content.contains("iteration"));
    }

    #[test]
    fn test_build_messages_iteration_one() {
        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let mode = ReflectionMode::new(storage, langbase, &config);
        let messages = mode.build_messages("Content", &[], 1);

        assert!(messages[0].content.contains("iteration 2"));
    }
}
