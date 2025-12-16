use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;

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

#[cfg(test)]
mod tests {
    use super::*;

    // Message tests
    #[test]
    fn test_message_system() {
        let msg = Message::system("You are a helpful assistant");
        assert!(matches!(msg.role, MessageRole::System));
        assert_eq!(msg.content, "You are a helpful assistant");
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello, world!");
        assert!(matches!(msg.role, MessageRole::User));
        assert_eq!(msg.content, "Hello, world!");
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there!");
        assert!(matches!(msg.role, MessageRole::Assistant));
        assert_eq!(msg.content, "Hi there!");
    }

    // PipeRequest tests
    #[test]
    fn test_pipe_request_new() {
        let req = PipeRequest::new("test-pipe", vec![Message::user("test")]);
        assert_eq!(req.name, "test-pipe");
        assert_eq!(req.messages.len(), 1);
        assert!(!req.stream);
        assert!(req.variables.is_none());
        assert!(req.thread_id.is_none());
    }

    #[test]
    fn test_pipe_request_with_variables() {
        let mut vars = HashMap::new();
        vars.insert("key1".to_string(), "value1".to_string());

        let req = PipeRequest::new("test", vec![]).with_variables(vars);
        assert!(req.variables.is_some());
        assert_eq!(req.variables.as_ref().unwrap().get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_pipe_request_with_variable() {
        let req = PipeRequest::new("test", vec![])
            .with_variable("key1", "value1")
            .with_variable("key2", "value2");

        let vars = req.variables.unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("key1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_pipe_request_with_thread_id() {
        let req = PipeRequest::new("test", vec![]).with_thread_id("thread-123");
        assert_eq!(req.thread_id, Some("thread-123".to_string()));
    }

    // CreatePipeRequest tests
    #[test]
    fn test_create_pipe_request_new() {
        let req = CreatePipeRequest::new("my-pipe");
        assert_eq!(req.name, "my-pipe");
        assert!(req.description.is_none());
        assert!(req.status.is_none());
        assert!(req.model.is_none());
    }

    #[test]
    fn test_create_pipe_request_with_description() {
        let req = CreatePipeRequest::new("pipe").with_description("A test pipe");
        assert_eq!(req.description, Some("A test pipe".to_string()));
    }

    #[test]
    fn test_create_pipe_request_with_status() {
        let req = CreatePipeRequest::new("pipe").with_status(PipeStatus::Private);
        assert!(matches!(req.status, Some(PipeStatus::Private)));
    }

    #[test]
    fn test_create_pipe_request_with_model() {
        let req = CreatePipeRequest::new("pipe").with_model("openai:gpt-4o-mini");
        assert_eq!(req.model, Some("openai:gpt-4o-mini".to_string()));
    }

    #[test]
    fn test_create_pipe_request_with_upsert() {
        let req = CreatePipeRequest::new("pipe").with_upsert(true);
        assert_eq!(req.upsert, Some(true));
    }

    #[test]
    fn test_create_pipe_request_with_json_output() {
        let req = CreatePipeRequest::new("pipe").with_json_output(true);
        assert_eq!(req.json, Some(true));
    }

    #[test]
    fn test_create_pipe_request_with_temperature() {
        let req = CreatePipeRequest::new("pipe").with_temperature(0.7);
        assert_eq!(req.temperature, Some(0.7));
    }

    #[test]
    fn test_create_pipe_request_with_max_tokens() {
        let req = CreatePipeRequest::new("pipe").with_max_tokens(1000);
        assert_eq!(req.max_tokens, Some(1000));
    }

    #[test]
    fn test_create_pipe_request_with_messages() {
        let messages = vec![Message::system("Be helpful")];
        let req = CreatePipeRequest::new("pipe").with_messages(messages);
        assert!(req.messages.is_some());
        assert_eq!(req.messages.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_create_pipe_request_builder_chain() {
        let req = CreatePipeRequest::new("test-pipe")
            .with_description("Test description")
            .with_status(PipeStatus::Public)
            .with_model("openai:gpt-4")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.5)
            .with_max_tokens(2000);

        assert_eq!(req.name, "test-pipe");
        assert_eq!(req.description, Some("Test description".to_string()));
        assert!(matches!(req.status, Some(PipeStatus::Public)));
        assert_eq!(req.model, Some("openai:gpt-4".to_string()));
        assert_eq!(req.upsert, Some(true));
        assert_eq!(req.json, Some(true));
        assert_eq!(req.temperature, Some(0.5));
        assert_eq!(req.max_tokens, Some(2000));
    }

    // ReasoningResponse tests
    #[test]
    fn test_reasoning_response_from_json() {
        let json = r#"{"thought": "This is a thought", "confidence": 0.9}"#;
        let resp = ReasoningResponse::from_completion(json);
        assert_eq!(resp.thought, "This is a thought");
        assert_eq!(resp.confidence, 0.9);
    }

    #[test]
    fn test_reasoning_response_from_plain_text() {
        let text = "Just a plain text response";
        let resp = ReasoningResponse::from_completion(text);
        assert_eq!(resp.thought, "Just a plain text response");
        assert_eq!(resp.confidence, 0.8); // default
        assert!(resp.metadata.is_none());
    }

    #[test]
    fn test_reasoning_response_with_metadata() {
        let json = r#"{"thought": "test", "confidence": 0.75, "metadata": {"key": "value"}}"#;
        let resp = ReasoningResponse::from_completion(json);
        assert_eq!(resp.thought, "test");
        assert_eq!(resp.confidence, 0.75);
        assert!(resp.metadata.is_some());
    }
}
