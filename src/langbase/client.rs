use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use super::types::{CreatePipeRequest, CreatePipeResponse, Message, PipeRequest, PipeResponse};
use crate::config::{LangbaseConfig, RequestConfig};
use crate::error::{LangbaseError, LangbaseResult};
use crate::prompts::{
    BIAS_DETECTION_PROMPT, DIVERGENT_REASONING_PROMPT, FALLACY_DETECTION_PROMPT,
    LINEAR_REASONING_PROMPT, REFLECTION_PROMPT, TREE_REASONING_PROMPT,
};

/// Client for interacting with Langbase Pipes API
#[derive(Clone)]
pub struct LangbaseClient {
    client: Client,
    base_url: String,
    api_key: String,
    request_config: RequestConfig,
}

impl LangbaseClient {
    /// Create a new Langbase client
    pub fn new(config: &LangbaseConfig, request_config: RequestConfig) -> LangbaseResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(request_config.timeout_ms))
            .build()
            .map_err(LangbaseError::Http)?;

        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key.clone(),
            request_config,
        })
    }

    /// Call a Langbase pipe with the given request
    pub async fn call_pipe(&self, request: PipeRequest) -> LangbaseResult<PipeResponse> {
        let url = format!("{}/v1/pipes/run", self.base_url);
        let pipe_name = request.name.clone();

        let mut last_error = None;
        let mut retries = 0;

        while retries <= self.request_config.max_retries {
            if retries > 0 {
                let delay = Duration::from_millis(
                    self.request_config.retry_delay_ms * (2_u64.pow(retries - 1)),
                );
                warn!(
                    pipe = %pipe_name,
                    retry = retries,
                    delay_ms = delay.as_millis(),
                    "Retrying Langbase request"
                );
                tokio::time::sleep(delay).await;
            }

            let start = Instant::now();

            match self.execute_request(&url, &request).await {
                Ok(response) => {
                    let latency = start.elapsed();
                    info!(
                        pipe = %pipe_name,
                        latency_ms = latency.as_millis(),
                        "Langbase pipe call succeeded"
                    );
                    return Ok(response);
                }
                Err(e) => {
                    let latency = start.elapsed();
                    error!(
                        pipe = %pipe_name,
                        error = %e,
                        latency_ms = latency.as_millis(),
                        retry = retries,
                        "Langbase pipe call failed"
                    );
                    last_error = Some(e);
                    retries += 1;
                }
            }
        }

        Err(LangbaseError::Unavailable {
            message: last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string()),
            retries,
        })
    }

    /// Execute a single request (internal)
    async fn execute_request(
        &self,
        url: &str,
        request: &PipeRequest,
    ) -> LangbaseResult<PipeResponse> {
        debug!(
            pipe = %request.name,
            messages = request.messages.len(),
            "Calling Langbase pipe"
        );

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LangbaseError::Timeout {
                        timeout_ms: self.request_config.timeout_ms,
                    }
                } else {
                    LangbaseError::Http(e)
                }
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|e| {
                warn!(error = %e, status = %status, "Failed to read pipe run error response body");
                "Unable to read error response".to_string()
            });
            return Err(LangbaseError::Api {
                status: status.as_u16(),
                message: error_body,
            });
        }

        let pipe_response: PipeResponse =
            response
                .json()
                .await
                .map_err(|e| LangbaseError::InvalidResponse {
                    message: format!("Failed to parse response: {}", e),
                })?;

        Ok(pipe_response)
    }

    /// Get the base URL (for testing)
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Create a new pipe
    pub async fn create_pipe(
        &self,
        request: CreatePipeRequest,
    ) -> LangbaseResult<CreatePipeResponse> {
        let url = format!("{}/v1/pipes", self.base_url);

        info!(pipe = %request.name, "Creating Langbase pipe");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(LangbaseError::Http)?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|e| {
                warn!(error = %e, status = %status, "Failed to read pipe creation error response body");
                "Unable to read error response".to_string()
            });
            return Err(LangbaseError::Api {
                status: status.as_u16(),
                message: error_body,
            });
        }

        let pipe_response: CreatePipeResponse =
            response
                .json()
                .await
                .map_err(|e| LangbaseError::InvalidResponse {
                    message: format!("Failed to parse create pipe response: {}", e),
                })?;

        info!(
            pipe = %pipe_response.name,
            url = %pipe_response.url,
            "Pipe created successfully"
        );

        Ok(pipe_response)
    }

    /// Delete a pipe by name (uses beta endpoint)
    pub async fn delete_pipe(&self, owner_login: &str, pipe_name: &str) -> LangbaseResult<()> {
        let url = format!("{}/beta/pipes/{}/{}", self.base_url, owner_login, pipe_name);

        info!(pipe = %pipe_name, owner = %owner_login, "Deleting Langbase pipe");

        let response = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(LangbaseError::Http)?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|e| {
                warn!(error = %e, status = %status, "Failed to read pipe deletion error response body");
                "Unable to read error response".to_string()
            });
            return Err(LangbaseError::Api {
                status: status.as_u16(),
                message: error_body,
            });
        }

        info!(pipe = %pipe_name, "Pipe deleted successfully");

        Ok(())
    }

    /// Ensure the linear reasoning pipe exists, creating it if needed
    pub async fn ensure_linear_pipe(&self, pipe_name: &str) -> LangbaseResult<()> {
        let request = CreatePipeRequest::new(pipe_name)
            .with_description("Linear reasoning mode for MCP server")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.7)
            .with_max_tokens(2000)
            .with_messages(vec![Message::system(LINEAR_REASONING_PROMPT)]);

        self.ensure_pipe_internal(request, "Linear reasoning").await
    }

    /// Ensure the tree reasoning pipe exists, creating it if needed
    pub async fn ensure_tree_pipe(&self, pipe_name: &str) -> LangbaseResult<()> {
        let request = CreatePipeRequest::new(pipe_name)
            .with_description("Tree-based reasoning mode for exploring multiple paths")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.8) // Slightly higher for exploration
            .with_max_tokens(3000) // More tokens for multiple branches
            .with_messages(vec![Message::system(TREE_REASONING_PROMPT)]);

        self.ensure_pipe_internal(request, "Tree reasoning").await
    }

    /// Ensure the divergent reasoning pipe exists, creating it if needed
    pub async fn ensure_divergent_pipe(&self, pipe_name: &str) -> LangbaseResult<()> {
        let request = CreatePipeRequest::new(pipe_name)
            .with_description("Divergent reasoning mode for creative perspectives")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.9) // Higher for maximum creativity
            .with_max_tokens(3000) // More tokens for multiple perspectives
            .with_messages(vec![Message::system(DIVERGENT_REASONING_PROMPT)]);

        self.ensure_pipe_internal(request, "Divergent reasoning")
            .await
    }

    /// Ensure the reflection reasoning pipe exists, creating it if needed
    pub async fn ensure_reflection_pipe(&self, pipe_name: &str) -> LangbaseResult<()> {
        let request = CreatePipeRequest::new(pipe_name)
            .with_description("Reflection mode for meta-cognitive analysis")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.6) // Lower for precise analysis
            .with_max_tokens(2500)
            .with_messages(vec![Message::system(REFLECTION_PROMPT)]);

        self.ensure_pipe_internal(request, "Reflection").await
    }

    /// Ensure all reasoning pipes exist, creating them if needed
    pub async fn ensure_all_pipes(&self) -> LangbaseResult<()> {
        self.ensure_linear_pipe("linear-reasoning-v1").await?;
        self.ensure_tree_pipe("tree-reasoning-v1").await?;
        self.ensure_divergent_pipe("divergent-reasoning-v1").await?;
        self.ensure_reflection_pipe("reflection-v1").await?;
        info!("All reasoning pipes ready");
        Ok(())
    }

    /// Ensure the bias detection pipe exists, creating it if needed
    pub async fn ensure_bias_detection_pipe(&self, pipe_name: &str) -> LangbaseResult<()> {
        let request = CreatePipeRequest::new(pipe_name)
            .with_description("Bias detection mode for identifying cognitive biases")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.5) // Lower for precise analysis
            .with_max_tokens(3000) // More tokens for detailed analysis
            .with_messages(vec![Message::system(BIAS_DETECTION_PROMPT)]);

        self.ensure_pipe_internal(request, "Bias detection").await
    }

    /// Ensure the fallacy detection pipe exists, creating it if needed
    pub async fn ensure_fallacy_detection_pipe(&self, pipe_name: &str) -> LangbaseResult<()> {
        let request = CreatePipeRequest::new(pipe_name)
            .with_description("Fallacy detection mode for identifying logical fallacies")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.5) // Lower for precise analysis
            .with_max_tokens(3000) // More tokens for detailed analysis
            .with_messages(vec![Message::system(FALLACY_DETECTION_PROMPT)]);

        self.ensure_pipe_internal(request, "Fallacy detection")
            .await
    }

    /// Ensure all detection pipes exist, creating them if needed
    pub async fn ensure_detection_pipes(&self) -> LangbaseResult<()> {
        self.ensure_bias_detection_pipe("detect-biases-v1").await?;
        self.ensure_fallacy_detection_pipe("detect-fallacies-v1")
            .await?;
        info!("All detection pipes ready");
        Ok(())
    }

    /// Internal helper to ensure a pipe exists
    async fn ensure_pipe_internal(
        &self,
        request: CreatePipeRequest,
        mode_name: &str,
    ) -> LangbaseResult<()> {
        let pipe_name = request.name.clone();

        match self.create_pipe(request).await {
            Ok(_) => {
                info!(pipe = %pipe_name, mode = %mode_name, "pipe ready");
                Ok(())
            }
            Err(LangbaseError::Api { status: 409, .. }) => {
                // Pipe already exists, that's fine
                info!(pipe = %pipe_name, mode = %mode_name, "pipe already exists");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };

        let request_config = RequestConfig::default();

        let client = LangbaseClient::new(&config, request_config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_base_url() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };

        let request_config = RequestConfig::default();
        let client = LangbaseClient::new(&config, request_config).unwrap();

        assert_eq!(client.base_url(), "https://api.langbase.com");
    }

    #[test]
    fn test_client_base_url_trailing_slash_trimmed() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://api.langbase.com/".to_string(),
        };

        let request_config = RequestConfig::default();
        let client = LangbaseClient::new(&config, request_config).unwrap();

        // Trailing slash should be trimmed
        assert_eq!(client.base_url(), "https://api.langbase.com");
    }

    #[test]
    fn test_client_with_custom_request_config() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://custom.api.com".to_string(),
        };

        let request_config = RequestConfig {
            timeout_ms: 60000,
            max_retries: 5,
            retry_delay_ms: 2000,
        };

        let client = LangbaseClient::new(&config, request_config);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.base_url(), "https://custom.api.com");
    }

    #[test]
    fn test_pipe_request_creation() {
        let messages = vec![
            Message::system("System prompt"),
            Message::user("User message"),
        ];
        let request = PipeRequest::new("test-pipe", messages);
        assert_eq!(request.name, "test-pipe");
        assert_eq!(request.messages.len(), 2);
    }

    #[test]
    fn test_pipe_request_with_thread_id() {
        let messages = vec![Message::user("Test")];
        let request = PipeRequest::new("pipe-1", messages).with_thread_id("thread-123");
        assert_eq!(request.thread_id, Some("thread-123".to_string()));
    }

    #[test]
    fn test_pipe_request_with_variables() {
        let messages = vec![Message::user("Test")];
        let mut vars = std::collections::HashMap::new();
        vars.insert("key1".to_string(), "value1".to_string());
        vars.insert("key2".to_string(), "value2".to_string());

        let request = PipeRequest::new("pipe-1", messages).with_variables(vars);
        assert!(request.variables.is_some());
        let variables = request.variables.unwrap();
        assert_eq!(variables.get("key1"), Some(&"value1".to_string()));
        assert_eq!(variables.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_create_pipe_request_basic() {
        let request = CreatePipeRequest::new("my-pipe");
        assert_eq!(request.name, "my-pipe");
        assert!(request.description.is_none());
        assert!(request.model.is_none());
    }

    #[test]
    fn test_create_pipe_request_builder() {
        let request = CreatePipeRequest::new("test-pipe")
            .with_description("Test description")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.7)
            .with_max_tokens(2000)
            .with_messages(vec![Message::system("System prompt")]);

        assert_eq!(request.name, "test-pipe");
        assert_eq!(request.description, Some("Test description".to_string()));
        assert_eq!(request.model, Some("openai:gpt-4o-mini".to_string()));
        assert_eq!(request.upsert, Some(true));
        assert_eq!(request.json, Some(true));
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(2000));
        assert!(request.messages.is_some());
    }

    #[test]
    fn test_message_system() {
        use super::super::types::MessageRole;
        let msg = Message::system("System instructions");
        assert!(matches!(msg.role, MessageRole::System));
        assert_eq!(msg.content, "System instructions");
    }

    #[test]
    fn test_message_user() {
        use super::super::types::MessageRole;
        let msg = Message::user("User query");
        assert!(matches!(msg.role, MessageRole::User));
        assert_eq!(msg.content, "User query");
    }

    #[test]
    fn test_message_assistant() {
        use super::super::types::MessageRole;
        let msg = Message::assistant("Assistant response");
        assert!(matches!(msg.role, MessageRole::Assistant));
        assert_eq!(msg.content, "Assistant response");
    }

    // URL Building Tests
    #[test]
    fn test_client_base_url_multiple_trailing_slashes() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://api.langbase.com///".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        assert_eq!(client.base_url(), "https://api.langbase.com");
    }

    #[test]
    fn test_client_base_url_with_path() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://api.langbase.com/v2/".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        assert_eq!(client.base_url(), "https://api.langbase.com/v2");
    }

    #[test]
    fn test_client_base_url_no_protocol() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "api.langbase.com".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        assert_eq!(client.base_url(), "api.langbase.com");
    }

    #[test]
    fn test_client_base_url_localhost() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "http://localhost:8080/".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_client_base_url_with_port() {
        let config = LangbaseConfig {
            api_key: "test_key".to_string(),
            base_url: "https://api.langbase.com:443/".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        assert_eq!(client.base_url(), "https://api.langbase.com:443");
    }

    // PipeRequest Construction Tests
    #[test]
    fn test_pipe_request_empty_messages() {
        let messages = vec![];
        let request = PipeRequest::new("test-pipe", messages);
        assert_eq!(request.name, "test-pipe");
        assert_eq!(request.messages.len(), 0);
        assert!(request.thread_id.is_none());
        assert!(request.variables.is_none());
    }

    #[test]
    fn test_pipe_request_single_message() {
        let messages = vec![Message::user("Single message")];
        let request = PipeRequest::new("pipe-1", messages);
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].content, "Single message");
    }

    #[test]
    fn test_pipe_request_multiple_messages_all_roles() {
        let messages = vec![
            Message::system("System prompt"),
            Message::user("User query 1"),
            Message::assistant("Assistant response 1"),
            Message::user("User query 2"),
        ];
        let request = PipeRequest::new("pipe-1", messages);
        assert_eq!(request.messages.len(), 4);
    }

    #[test]
    fn test_pipe_request_with_empty_thread_id() {
        let messages = vec![Message::user("Test")];
        let request = PipeRequest::new("pipe-1", messages).with_thread_id("");
        assert_eq!(request.thread_id, Some("".to_string()));
    }

    #[test]
    fn test_pipe_request_with_special_chars_in_thread_id() {
        let messages = vec![Message::user("Test")];
        let request =
            PipeRequest::new("pipe-1", messages).with_thread_id("thread-123-abc_!@#$%^&*()");
        assert_eq!(
            request.thread_id,
            Some("thread-123-abc_!@#$%^&*()".to_string())
        );
    }

    #[test]
    fn test_pipe_request_with_empty_variables() {
        let messages = vec![Message::user("Test")];
        let vars = std::collections::HashMap::new();
        let request = PipeRequest::new("pipe-1", messages).with_variables(vars);
        assert!(request.variables.is_some());
        assert_eq!(request.variables.unwrap().len(), 0);
    }

    #[test]
    fn test_pipe_request_with_many_variables() {
        let messages = vec![Message::user("Test")];
        let mut vars = std::collections::HashMap::new();
        for i in 0..100 {
            vars.insert(format!("key{}", i), format!("value{}", i));
        }
        let request = PipeRequest::new("pipe-1", messages).with_variables(vars);
        assert_eq!(request.variables.unwrap().len(), 100);
    }

    #[test]
    fn test_pipe_request_chaining_all_options() {
        let messages = vec![Message::user("Test")];
        let mut vars = std::collections::HashMap::new();
        vars.insert("key".to_string(), "value".to_string());
        let request = PipeRequest::new("pipe-1", messages)
            .with_thread_id("thread-123")
            .with_variables(vars);
        assert!(request.thread_id.is_some());
        assert!(request.variables.is_some());
    }

    // CreatePipeRequest Builder Tests
    #[test]
    fn test_create_pipe_request_empty_name() {
        let request = CreatePipeRequest::new("");
        assert_eq!(request.name, "");
    }

    #[test]
    fn test_create_pipe_request_name_with_special_chars() {
        let request = CreatePipeRequest::new("my-pipe_v1.0-test");
        assert_eq!(request.name, "my-pipe_v1.0-test");
    }

    #[test]
    fn test_create_pipe_request_with_empty_description() {
        let request = CreatePipeRequest::new("pipe-1").with_description("");
        assert_eq!(request.description, Some("".to_string()));
    }

    #[test]
    fn test_create_pipe_request_with_long_description() {
        let long_desc = "a".repeat(1000);
        let request = CreatePipeRequest::new("pipe-1").with_description(&long_desc);
        assert_eq!(request.description, Some(long_desc));
    }

    #[test]
    fn test_create_pipe_request_with_various_models() {
        let models = vec![
            "openai:gpt-4o-mini",
            "openai:gpt-4o",
            "anthropic:claude-3-opus",
            "custom-model",
        ];
        for model in models {
            let request = CreatePipeRequest::new("pipe-1").with_model(model);
            assert_eq!(request.model, Some(model.to_string()));
        }
    }

    #[test]
    fn test_create_pipe_request_upsert_false() {
        let request = CreatePipeRequest::new("pipe-1").with_upsert(false);
        assert_eq!(request.upsert, Some(false));
    }

    #[test]
    fn test_create_pipe_request_json_output_false() {
        let request = CreatePipeRequest::new("pipe-1").with_json_output(false);
        assert_eq!(request.json, Some(false));
    }

    #[test]
    fn test_create_pipe_request_temperature_bounds() {
        let temps = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        for temp in temps {
            let request = CreatePipeRequest::new("pipe-1").with_temperature(temp);
            assert_eq!(request.temperature, Some(temp));
        }
    }

    #[test]
    fn test_create_pipe_request_max_tokens_various() {
        let token_counts = vec![1, 100, 1000, 4096, 8192];
        for tokens in token_counts {
            let request = CreatePipeRequest::new("pipe-1").with_max_tokens(tokens);
            assert_eq!(request.max_tokens, Some(tokens));
        }
    }

    #[test]
    fn test_create_pipe_request_with_empty_messages() {
        let request = CreatePipeRequest::new("pipe-1").with_messages(vec![]);
        assert!(request.messages.is_some());
        assert_eq!(request.messages.unwrap().len(), 0);
    }

    #[test]
    fn test_create_pipe_request_with_multiple_system_messages() {
        let messages = vec![
            Message::system("System 1"),
            Message::system("System 2"),
            Message::system("System 3"),
        ];
        let request = CreatePipeRequest::new("pipe-1").with_messages(messages);
        assert_eq!(request.messages.unwrap().len(), 3);
    }

    #[test]
    fn test_create_pipe_request_full_builder_chain() {
        let messages = vec![Message::system("Test")];
        let request = CreatePipeRequest::new("full-pipe")
            .with_description("Full test")
            .with_model("openai:gpt-4o-mini")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(0.8)
            .with_max_tokens(3000)
            .with_messages(messages);

        assert_eq!(request.name, "full-pipe");
        assert_eq!(request.description, Some("Full test".to_string()));
        assert_eq!(request.model, Some("openai:gpt-4o-mini".to_string()));
        assert_eq!(request.upsert, Some(true));
        assert_eq!(request.json, Some(true));
        assert_eq!(request.temperature, Some(0.8));
        assert_eq!(request.max_tokens, Some(3000));
        assert!(request.messages.is_some());
    }

    // Message Builder Tests
    #[test]
    fn test_message_system_empty_content() {
        let msg = Message::system("");
        assert_eq!(msg.content, "");
    }

    #[test]
    fn test_message_user_empty_content() {
        let msg = Message::user("");
        assert_eq!(msg.content, "");
    }

    #[test]
    fn test_message_assistant_empty_content() {
        let msg = Message::assistant("");
        assert_eq!(msg.content, "");
    }

    #[test]
    fn test_message_system_long_content() {
        let long_content = "x".repeat(10000);
        let msg = Message::system(&long_content);
        assert_eq!(msg.content, long_content);
    }

    #[test]
    fn test_message_user_with_special_chars() {
        let content = "Test\n\r\t!@#$%^&*(){}[]<>?/|\\\"'`~";
        let msg = Message::user(content);
        assert_eq!(msg.content, content);
    }

    #[test]
    fn test_message_assistant_with_unicode() {
        let content = "Hello 世界 🌍 émoji ñ ü";
        let msg = Message::assistant(content);
        assert_eq!(msg.content, content);
    }

    #[test]
    fn test_message_user_with_json() {
        let json_content = r#"{"key": "value", "nested": {"data": 123}}"#;
        let msg = Message::user(json_content);
        assert_eq!(msg.content, json_content);
    }

    // RequestConfig Tests
    #[test]
    fn test_request_config_default_values() {
        let config = RequestConfig::default();
        assert!(config.timeout_ms > 0);
        assert!(config.max_retries >= 0);
        assert!(config.retry_delay_ms > 0);
    }

    #[test]
    fn test_request_config_zero_retries() {
        let config = RequestConfig {
            timeout_ms: 30000,
            max_retries: 0,
            retry_delay_ms: 1000,
        };
        let langbase_config = LangbaseConfig {
            api_key: "test".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let client = LangbaseClient::new(&langbase_config, config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_request_config_high_timeout() {
        let config = RequestConfig {
            timeout_ms: 300000,
            max_retries: 3,
            retry_delay_ms: 1000,
        };
        let langbase_config = LangbaseConfig {
            api_key: "test".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let client = LangbaseClient::new(&langbase_config, config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_request_config_many_retries() {
        let config = RequestConfig {
            timeout_ms: 30000,
            max_retries: 10,
            retry_delay_ms: 500,
        };
        let langbase_config = LangbaseConfig {
            api_key: "test".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let client = LangbaseClient::new(&langbase_config, config);
        assert!(client.is_ok());
    }

    // Client Configuration Tests
    #[test]
    fn test_client_with_empty_api_key() {
        let config = LangbaseConfig {
            api_key: "".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default());
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_with_long_api_key() {
        let long_key = "k".repeat(500);
        let config = LangbaseConfig {
            api_key: long_key.clone(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default());
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_base_url_immutability() {
        let config = LangbaseConfig {
            api_key: "test".to_string(),
            base_url: "https://api.langbase.com/".to_string(),
        };
        let client = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        let url1 = client.base_url();
        let url2 = client.base_url();
        assert_eq!(url1, url2);
        assert_eq!(url1, "https://api.langbase.com");
    }

    #[test]
    fn test_client_clone() {
        let config = LangbaseConfig {
            api_key: "test".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let client1 = LangbaseClient::new(&config, RequestConfig::default()).unwrap();
        let client2 = client1.clone();
        assert_eq!(client1.base_url(), client2.base_url());
    }

    // Pipe Name Tests
    #[test]
    fn test_pipe_request_with_hyphenated_name() {
        let messages = vec![Message::user("Test")];
        let request = PipeRequest::new("my-custom-pipe-v1", messages);
        assert_eq!(request.name, "my-custom-pipe-v1");
    }

    #[test]
    fn test_pipe_request_with_underscored_name() {
        let messages = vec![Message::user("Test")];
        let request = PipeRequest::new("my_custom_pipe_v1", messages);
        assert_eq!(request.name, "my_custom_pipe_v1");
    }

    #[test]
    fn test_pipe_request_with_numeric_name() {
        let messages = vec![Message::user("Test")];
        let request = PipeRequest::new("pipe123", messages);
        assert_eq!(request.name, "pipe123");
    }

    #[test]
    fn test_create_pipe_request_with_version_suffix() {
        let request = CreatePipeRequest::new("reasoning-v2.1.0");
        assert_eq!(request.name, "reasoning-v2.1.0");
    }

    // Edge Cases
    #[test]
    fn test_pipe_request_with_very_long_name() {
        let long_name = "pipe-".to_string() + &"a".repeat(200);
        let messages = vec![Message::user("Test")];
        let request = PipeRequest::new(&long_name, messages);
        assert_eq!(request.name, long_name);
    }

    #[test]
    fn test_message_content_with_null_bytes() {
        let content = "Hello\0World";
        let msg = Message::user(content);
        assert_eq!(msg.content, content);
    }

    #[test]
    fn test_create_pipe_request_temperature_negative() {
        let request = CreatePipeRequest::new("pipe-1").with_temperature(-0.5);
        assert_eq!(request.temperature, Some(-0.5));
    }

    #[test]
    fn test_create_pipe_request_max_tokens_zero() {
        let request = CreatePipeRequest::new("pipe-1").with_max_tokens(0);
        assert_eq!(request.max_tokens, Some(0));
    }

    #[test]
    fn test_variables_with_special_key_names() {
        let messages = vec![Message::user("Test")];
        let mut vars = std::collections::HashMap::new();
        vars.insert("key-with-dashes".to_string(), "value1".to_string());
        vars.insert("key_with_underscores".to_string(), "value2".to_string());
        vars.insert("keyWithCamelCase".to_string(), "value3".to_string());
        vars.insert("key.with.dots".to_string(), "value4".to_string());

        let request = PipeRequest::new("pipe-1", messages).with_variables(vars.clone());
        let req_vars = request.variables.unwrap();
        assert_eq!(req_vars.len(), 4);
        assert_eq!(req_vars.get("key-with-dashes"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_variables_with_empty_values() {
        let messages = vec![Message::user("Test")];
        let mut vars = std::collections::HashMap::new();
        vars.insert("key1".to_string(), "".to_string());
        vars.insert("key2".to_string(), "value".to_string());

        let request = PipeRequest::new("pipe-1", messages).with_variables(vars);
        let req_vars = request.variables.unwrap();
        assert_eq!(req_vars.get("key1"), Some(&"".to_string()));
    }

    #[test]
    fn test_variables_with_json_values() {
        let messages = vec![Message::user("Test")];
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "json_data".to_string(),
            r#"{"nested": "value"}"#.to_string(),
        );

        let request = PipeRequest::new("pipe-1", messages).with_variables(vars);
        let req_vars = request.variables.unwrap();
        assert_eq!(
            req_vars.get("json_data"),
            Some(&r#"{"nested": "value"}"#.to_string())
        );
    }
}
