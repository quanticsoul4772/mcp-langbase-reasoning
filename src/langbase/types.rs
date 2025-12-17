//! Langbase API types for pipe communication.
//!
//! This module provides request/response types for the Langbase Pipes API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;

/// Message in a Langbase conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender.
    pub role: MessageRole,
    /// Content of the message.
    pub content: String,
}

/// Message role in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System instruction message.
    System,
    /// User input message.
    User,
    /// Assistant response message.
    Assistant,
}

/// Request to run a Langbase pipe.
#[derive(Debug, Clone, Serialize)]
pub struct PipeRequest {
    /// Pipe name (required by Langbase API).
    pub name: String,
    /// Conversation messages to send.
    pub messages: Vec<Message>,
    /// Disable streaming (default: false for non-streaming response).
    #[serde(default)]
    pub stream: bool,
    /// Optional template variables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, String>>,
    /// Optional thread ID for conversation continuity.
    #[serde(rename = "threadId", skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// Response from a Langbase pipe.
#[derive(Debug, Clone, Deserialize)]
pub struct PipeResponse {
    /// Whether the request succeeded.
    pub success: bool,
    /// Completion text from the model.
    pub completion: String,
    /// Thread ID for conversation continuity.
    #[serde(rename = "threadId")]
    pub thread_id: Option<String>,
    /// Raw model response details.
    pub raw: Option<RawResponse>,
}

/// Raw model response details.
#[derive(Debug, Clone, Deserialize)]
pub struct RawResponse {
    /// Model name used for completion.
    pub model: Option<String>,
    /// Token usage information.
    pub usage: Option<Usage>,
}

/// Token usage information.
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    /// Number of prompt tokens.
    pub prompt_tokens: Option<u32>,
    /// Number of completion tokens.
    pub completion_tokens: Option<u32>,
    /// Total tokens used.
    pub total_tokens: Option<u32>,
}

impl Message {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }
}

impl PipeRequest {
    /// Create a new pipe request with name and messages
    pub fn new(name: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            name: name.into(),
            messages,
            stream: false, // Disable streaming for synchronous responses
            variables: None,
            thread_id: None,
        }
    }

    /// Add variables to the request
    pub fn with_variables(mut self, variables: HashMap<String, String>) -> Self {
        self.variables = Some(variables);
        self
    }

    /// Add a single variable
    pub fn with_variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// Set the thread ID for conversation continuity
    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }
}

/// Structured reasoning response from a pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningResponse {
    /// The reasoning thought/content.
    pub thought: String,
    /// Confidence score (0.0-1.0).
    pub confidence: f64,
    /// Optional metadata.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Request to create a new Langbase pipe.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePipeRequest {
    /// Pipe name (unique identifier).
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Visibility status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PipeStatus>,
    /// Model to use (e.g., "openai:gpt-4o-mini").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Whether to update if exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upsert: Option<bool>,
    /// Whether to enable streaming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Whether to output JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json: Option<bool>,
    /// Whether to store conversations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// Model temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Initial messages/prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
}

/// Pipe visibility status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipeStatus {
    /// Publicly accessible.
    Public,
    /// Private access only.
    Private,
}

/// Response from creating a pipe.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePipeResponse {
    /// Pipe name.
    pub name: String,
    /// Pipe description.
    pub description: Option<String>,
    /// Visibility status.
    pub status: String,
    /// Owner's login name.
    pub owner_login: String,
    /// Pipe URL.
    pub url: String,
    /// Pipe type.
    #[serde(rename = "type")]
    pub pipe_type: String,
    /// API key for the pipe.
    pub api_key: String,
}

impl CreatePipeRequest {
    /// Create a new pipe request with just a name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            status: None,
            model: None,
            upsert: None,
            stream: None,
            json: None,
            store: None,
            temperature: None,
            max_tokens: None,
            messages: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set status (public/private)
    pub fn with_status(mut self, status: PipeStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Set model (e.g., "openai:gpt-4o-mini")
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Enable upsert (update if exists)
    pub fn with_upsert(mut self, upsert: bool) -> Self {
        self.upsert = Some(upsert);
        self
    }

    /// Enable JSON output mode
    pub fn with_json_output(mut self, json: bool) -> Self {
        self.json = Some(json);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set system/user messages
    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = Some(messages);
        self
    }
}

impl ReasoningResponse {
    /// Parse a reasoning response from pipe completion text
    pub fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<ReasoningResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse reasoning response as JSON, using raw completion as thought"
                );
                // Fall back to treating the entire completion as the thought
                Self {
                    thought: completion.to_string(),
                    confidence: 0.8,
                    metadata: None,
                }
            }
        }
    }
}

// ============================================================================
// Phase 4: Bias & Fallacy Detection Response Types
// ============================================================================

/// A single detected bias from the analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedBias {
    /// Name of the cognitive bias (e.g., "confirmation_bias", "anchoring_bias")
    pub bias_type: String,
    /// Severity level from 1 (minor) to 5 (critical)
    pub severity: i32,
    /// Confidence in this detection (0.0-1.0)
    pub confidence: f64,
    /// Explanation of why this is a bias
    pub explanation: String,
    /// Suggested remediation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Text excerpt showing the bias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

/// Response from bias detection pipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasDetectionResponse {
    /// List of detected biases
    pub detections: Vec<DetectedBias>,
    /// Overall reasoning quality score (0.0-1.0, higher = better)
    pub reasoning_quality: f64,
    /// Overall assessment summary
    pub overall_assessment: String,
    /// Additional metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl BiasDetectionResponse {
    /// Parse a bias detection response from pipe completion text
    pub fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<BiasDetectionResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse bias detection response as JSON, returning empty result"
                );
                Self {
                    detections: vec![],
                    reasoning_quality: 0.5,
                    overall_assessment: completion.to_string(),
                    metadata: None,
                }
            }
        }
    }
}

/// A single detected logical fallacy from the analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFallacy {
    /// Name of the fallacy (e.g., "ad_hominem", "straw_man", "false_dichotomy")
    pub fallacy_type: String,
    /// Category: "formal" or "informal"
    pub category: String,
    /// Severity level from 1 (minor) to 5 (critical)
    pub severity: i32,
    /// Confidence in this detection (0.0-1.0)
    pub confidence: f64,
    /// Explanation of why this is a fallacy
    pub explanation: String,
    /// Suggested remediation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Text excerpt showing the fallacy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

/// Response from fallacy detection pipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallacyDetectionResponse {
    /// List of detected fallacies
    pub detections: Vec<DetectedFallacy>,
    /// Overall argument validity score (0.0-1.0, higher = more valid)
    pub argument_validity: f64,
    /// Overall assessment summary
    pub overall_assessment: String,
    /// Additional metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl FallacyDetectionResponse {
    /// Parse a fallacy detection response from pipe completion text
    pub fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<FallacyDetectionResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse fallacy detection response as JSON, returning empty result"
                );
                Self {
                    detections: vec![],
                    argument_validity: 0.5,
                    overall_assessment: completion.to_string(),
                    metadata: None,
                }
            }
        }
    }
}
