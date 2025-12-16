use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Message in a Langbase conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

/// Message role
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// Request to run a Langbase pipe
#[derive(Debug, Clone, Serialize)]
pub struct PipeRequest {
    /// Pipe name (required by Langbase API)
    pub name: String,
    pub messages: Vec<Message>,
    /// Disable streaming (default: false for non-streaming response)
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, String>>,
    #[serde(rename = "threadId", skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// Response from a Langbase pipe
#[derive(Debug, Clone, Deserialize)]
pub struct PipeResponse {
    pub success: bool,
    pub completion: String,
    #[serde(rename = "threadId")]
    pub thread_id: Option<String>,
    pub raw: Option<RawResponse>,
}

/// Raw model response details
#[derive(Debug, Clone, Deserialize)]
pub struct RawResponse {
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

/// Token usage information
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
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

/// Structured reasoning response from a pipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningResponse {
    pub thought: String,
    pub confidence: f64,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Request to create a new Langbase pipe
#[derive(Debug, Clone, Serialize)]
pub struct CreatePipeRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PipeStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upsert: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
}

/// Pipe visibility status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipeStatus {
    Public,
    Private,
}

/// Response from creating a pipe
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePipeResponse {
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub owner_login: String,
    pub url: String,
    #[serde(rename = "type")]
    pub pipe_type: String,
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
        // Try to parse as JSON first
        if let Ok(parsed) = serde_json::from_str::<ReasoningResponse>(completion) {
            return parsed;
        }

        // Fall back to treating the entire completion as the thought
        Self {
            thought: completion.to_string(),
            confidence: 0.8,
            metadata: None,
        }
    }
}
