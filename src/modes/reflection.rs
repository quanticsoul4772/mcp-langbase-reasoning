//! Reflection reasoning mode - meta-cognitive analysis and quality improvement.
//!
//! This module provides reflection capabilities for analyzing and improving reasoning:
//! - Iterative refinement with quality thresholds
//! - Strength and weakness identification
//! - Improved thought generation
//! - Session evaluation for overall reasoning quality

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::REFLECTION_PROMPT;
use crate::storage::{Invocation, Session, SqliteStorage, Storage, Thought};

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
pub struct ReflectionMode {
    /// Storage backend for persisting data.
    storage: SqliteStorage,
    /// Langbase client for LLM-powered reflection.
    langbase: LangbaseClient,
    /// The Langbase pipe name for reflection.
    pipe_name: String,
}

impl ReflectionMode {
    /// Create a new reflection mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
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
        let session = self.get_or_create_session(&params.session_id).await?;
        debug!(session_id = %session.id, "Processing reflection reasoning");

        // Get the content to reflect upon
        let (original_content, original_thought) = if let Some(thought_id) = &params.thought_id {
            let thought =
                self.storage.get_thought(thought_id).await?.ok_or_else(|| {
                    ToolError::Session(format!("Thought not found: {}", thought_id))
                })?;
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
            let reflection = self.parse_response(&response.completion)?;
            let quality = reflection.quality_score.unwrap_or(reflection.confidence);

            // Log invocation
            let latency = start.elapsed().as_millis() as i64;
            invocation = invocation.success(
                serde_json::to_value(&reflection).unwrap_or_default(),
                latency,
            );
            self.storage.log_invocation(&invocation).await?;

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

        self.storage.create_thought(&reflection_thought).await?;

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

                self.storage.create_thought(&improved).await?;

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
        let thoughts = self.storage.get_session_thoughts(session_id).await?;

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

    async fn get_or_create_session(&self, session_id: &Option<String>) -> AppResult<Session> {
        match session_id {
            Some(id) => match self.storage.get_session(id).await? {
                Some(s) => Ok(s),
                None => {
                    let mut new_session = Session::new("reflection");
                    new_session.id = id.clone();
                    self.storage.create_session(&new_session).await?;
                    Ok(new_session)
                }
            },
            None => {
                let session = Session::new("reflection");
                self.storage.create_session(&session).await?;
                Ok(session)
            }
        }
    }

    async fn get_reasoning_chain(
        &self,
        session_id: &str,
        thought: &Thought,
    ) -> AppResult<Vec<Thought>> {
        let all_thoughts = self.storage.get_session_thoughts(session_id).await?;

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
        // Try to parse as JSON first
        if let Ok(response) = serde_json::from_str::<ReflectionResponse>(completion) {
            return Ok(response);
        }

        // Try to extract JSON from markdown code blocks
        let json_str = if completion.contains("```json") {
            completion
                .split("```json")
                .nth(1)
                .and_then(|s| s.split("```").next())
                .unwrap_or(completion)
        } else if completion.contains("```") {
            completion.split("```").nth(1).unwrap_or(completion)
        } else {
            completion
        };

        serde_json::from_str::<ReflectionResponse>(json_str.trim()).map_err(|e| {
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
}
