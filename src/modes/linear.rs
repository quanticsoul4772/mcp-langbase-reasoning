use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest, ReasoningResponse};
use crate::prompts::LINEAR_REASONING_PROMPT;
use crate::storage::{Invocation, Session, SqliteStorage, Storage, Thought};

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

/// Result of linear reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearResult {
    pub thought_id: String,
    pub session_id: String,
    pub content: String,
    pub confidence: f64,
    pub previous_thought: Option<String>,
}

/// Linear reasoning mode handler
pub struct LinearMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
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
        let session = match &params.session_id {
            Some(id) => {
                match self.storage.get_session(id).await? {
                    Some(s) => s,
                    None => {
                        // Create new session with provided ID
                        let mut new_session = Session::new("linear");
                        new_session.id = id.clone();
                        self.storage.create_session(&new_session).await?;
                        new_session
                    }
                }
            }
            None => {
                let session = Session::new("linear");
                self.storage.create_session(&session).await?;
                session
            }
        };

        debug!(session_id = %session.id, "Processing linear reasoning");

        // Get previous thoughts for context
        let previous_thoughts = self.storage.get_session_thoughts(&session.id).await?;
        let previous_thought = previous_thoughts.last().cloned();

        // Build context for Langbase
        let messages = self.build_messages(&params.content, &previous_thoughts);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.linear",
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
        let reasoning = ReasoningResponse::from_completion(&response.completion);

        // Create and store thought
        let thought = Thought::new(&session.id, &reasoning.thought, "linear")
            .with_confidence(reasoning.confidence.max(params.confidence));

        self.storage.create_thought(&thought).await?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serde_json::to_value(&reasoning).unwrap_or_default(),
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
