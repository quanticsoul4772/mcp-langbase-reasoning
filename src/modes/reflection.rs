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

/// Response from reflection reasoning Langbase pipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResponse {
    pub analysis: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub recommendations: Vec<String>,
    pub confidence: f64,
    #[serde(default)]
    pub quality_score: Option<f64>,
    #[serde(default)]
    pub improved_thought: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Result of reflection reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResult {
    pub session_id: String,
    pub reflection_thought_id: String,
    pub original_thought_id: Option<String>,
    pub analysis: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub recommendations: Vec<String>,
    pub quality_score: f64,
    pub improved_thought: Option<ImprovedThought>,
    pub iterations_performed: usize,
    pub quality_improved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Improved thought from reflection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovedThought {
    pub thought_id: String,
    pub content: String,
    pub confidence: f64,
}

/// Reflection reasoning mode handler
pub struct ReflectionMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
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
            let thought = self.storage.get_thought(thought_id).await?
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

        let quality_improved = best_quality > original_thought
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
            return Err(ToolError::Session("Session has no thoughts to evaluate".to_string()).into());
        }

        let total_confidence: f64 = thoughts.iter().map(|t| t.confidence).sum();
        let avg_confidence = total_confidence / thoughts.len() as f64;

        let mode_counts: std::collections::HashMap<String, usize> = thoughts
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
            Some(id) => {
                match self.storage.get_session(id).await? {
                    Some(s) => Ok(s),
                    None => {
                        let mut new_session = Session::new("reflection");
                        new_session.id = id.clone();
                        self.storage.create_session(&new_session).await?;
                        Ok(new_session)
                    }
                }
            }
            None => {
                let session = Session::new("reflection");
                self.storage.create_session(&session).await?;
                Ok(session)
            }
        }
    }

    async fn get_reasoning_chain(&self, session_id: &str, thought: &Thought) -> AppResult<Vec<Thought>> {
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
                .map(|t| format!("- [{}] (confidence: {:.2}) {}", t.mode, t.confidence, t.content))
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
            completion
                .split("```")
                .nth(1)
                .unwrap_or(completion)
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

/// Session evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvaluation {
    pub session_id: String,
    pub total_thoughts: usize,
    pub average_confidence: f64,
    pub mode_distribution: std::collections::HashMap<String, usize>,
    pub coherence_score: f64,
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
