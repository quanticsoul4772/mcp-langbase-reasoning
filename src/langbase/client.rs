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
            let error_body = response.text().await.unwrap_or_default();
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
            let error_body = response.text().await.unwrap_or_default();
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
            let error_body = response.text().await.unwrap_or_default();
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
}
