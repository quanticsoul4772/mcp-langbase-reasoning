use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::DIVERGENT_REASONING_PROMPT;
use crate::storage::{Invocation, Session, SqliteStorage, Storage, Thought};

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

/// Response from divergent reasoning Langbase pipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergentResponse {
    pub perspectives: Vec<Perspective>,
    pub synthesis: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Individual perspective in divergent response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perspective {
    pub thought: String,
    pub novelty: f64,
    pub viability: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assumptions_challenged: Option<Vec<String>>,
}

/// Result of divergent reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergentResult {
    pub session_id: String,
    pub thought_id: String,
    pub perspectives: Vec<PerspectiveInfo>,
    pub synthesis: String,
    pub synthesis_thought_id: String,
    pub total_novelty_score: f64,
    pub most_viable_perspective: usize,
    pub most_novel_perspective: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Perspective information in result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveInfo {
    pub thought_id: String,
    pub content: String,
    pub novelty: f64,
    pub viability: f64,
    pub assumptions_challenged: Vec<String>,
}

/// Divergent reasoning mode handler
pub struct DivergentMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}

impl DivergentMode {
    /// Create a new divergent mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
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
        let session = self.get_or_create_session(&params.session_id).await?;
        debug!(session_id = %session.id, "Processing divergent reasoning");

        // Get previous context
        let previous_thoughts = self.storage.get_session_thoughts(&session.id).await?;

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
            serde_json::to_value(&params).unwrap_or_default(),
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
        let divergent_response = self.parse_response(&response.completion)?;

        // Create main thought for the original input
        let main_thought = Thought::new(&session.id, &params.content, "divergent")
            .with_confidence(params.confidence);
        let main_thought = if let Some(ref branch_id) = params.branch_id {
            main_thought.with_branch(branch_id)
        } else {
            main_thought
        };
        self.storage.create_thought(&main_thought).await?;

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

            self.storage.create_thought(&perspective_thought).await?;

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
                assumptions_challenged: p.assumptions_challenged.clone().unwrap_or_default(),
            });
        }

        // Create synthesis thought
        let synthesis_thought = Thought::new(&session.id, &divergent_response.synthesis, "divergent")
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

        self.storage.create_thought(&synthesis_thought).await?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serde_json::to_value(&divergent_response).unwrap_or_default(),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

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

    async fn get_or_create_session(&self, session_id: &Option<String>) -> AppResult<Session> {
        match session_id {
            Some(id) => {
                match self.storage.get_session(id).await? {
                    Some(s) => Ok(s),
                    None => {
                        let mut new_session = Session::new("divergent");
                        new_session.id = id.clone();
                        self.storage.create_session(&new_session).await?;
                        Ok(new_session)
                    }
                }
            }
            None => {
                let session = Session::new("divergent");
                self.storage.create_session(&session).await?;
                Ok(session)
            }
        }
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
            &format!("Generate {} diverse, non-obvious perspectives", num_perspectives)
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
        // Try to parse as JSON first
        if let Ok(response) = serde_json::from_str::<DivergentResponse>(completion) {
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

        serde_json::from_str::<DivergentResponse>(json_str.trim()).map_err(|e| {
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
