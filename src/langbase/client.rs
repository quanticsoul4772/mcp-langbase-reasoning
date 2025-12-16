use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use super::types::{CreatePipeRequest, CreatePipeResponse, Message, PipeRequest, PipeResponse};
use crate::config::{LangbaseConfig, RequestConfig};
use crate::error::{LangbaseError, LangbaseResult};
use crate::prompts::LINEAR_REASONING_PROMPT;

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

        match self.create_pipe(request).await {
            Ok(_) => {
                info!(pipe = %pipe_name, "Linear reasoning pipe ready");
                Ok(())
            }
            Err(LangbaseError::Api { status: 409, .. }) => {
                // Pipe already exists, that's fine
                info!(pipe = %pipe_name, "Pipe already exists");
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
}
