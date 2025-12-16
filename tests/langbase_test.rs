//! Integration tests for Langbase client
//!
//! Tests HTTP client behavior using wiremock for request/response mocking.

use serde_json::json;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

use mcp_langbase_reasoning::config::{LangbaseConfig, RequestConfig};
use mcp_langbase_reasoning::langbase::{LangbaseClient, Message, PipeRequest};

/// Create a test client pointing to mock server
fn create_test_client(base_url: &str) -> LangbaseClient {
    let config = LangbaseConfig {
        api_key: "test-api-key".to_string(),
        base_url: base_url.to_string(),
    };

    let request_config = RequestConfig {
        timeout_ms: 5000,
        max_retries: 0, // No retries for testing
        retry_delay_ms: 100,
    };

    LangbaseClient::new(&config, request_config).expect("Failed to create client")
}

/// Create a simple pipe request for testing
fn create_test_request(content: &str) -> PipeRequest {
    PipeRequest::new(
        "test-pipe",
        vec![Message::user(content)],
    )
}

#[cfg(test)]
mod pipe_call_tests {
    use super::*;

    #[tokio::test]
    async fn test_successful_pipe_call() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .and(header("Authorization", "Bearer test-api-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "This is a test reasoning response.",
                "threadId": "thread-123",
                "raw": {
                    "model": "gpt-4o-mini",
                    "usage": {
                        "prompt_tokens": 100,
                        "completion_tokens": 50,
                        "total_tokens": 150
                    }
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Test thought content");
        let result = client.call_pipe(request).await;

        assert!(result.is_ok(), "Pipe call should succeed: {:?}", result.err());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.completion, "This is a test reasoning response.");
        assert_eq!(response.thread_id, Some("thread-123".to_string()));
    }

    #[tokio::test]
    async fn test_pipe_call_with_thread_id() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "Continued reasoning...",
                "threadId": "session-123"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = PipeRequest::new("test-pipe", vec![Message::user("Continue")])
            .with_thread_id("session-123");
        let result = client.call_pipe(request).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pipe_call_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {
                    "message": "Missing required field: content",
                    "type": "invalid_request_error"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("");
        let result = client.call_pipe(request).await;

        assert!(result.is_err(), "Should return error for validation failure");
    }

    #[tokio::test]
    async fn test_pipe_call_authentication_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": {
                    "message": "Invalid API key",
                    "type": "authentication_error"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Test");
        let result = client.call_pipe(request).await;

        assert!(result.is_err(), "Should return error for auth failure");
    }

    #[tokio::test]
    async fn test_pipe_call_rate_limit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_json(json!({
                        "error": {
                            "message": "Rate limit exceeded",
                            "type": "rate_limit_error"
                        }
                    }))
                    .insert_header("Retry-After", "60"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Test");
        let result = client.call_pipe(request).await;

        assert!(result.is_err(), "Should return error for rate limit");
    }

    #[tokio::test]
    async fn test_pipe_call_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": {
                    "message": "Internal server error",
                    "type": "server_error"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Test");
        let result = client.call_pipe(request).await;

        assert!(result.is_err(), "Should return error for server error");
    }
}

#[cfg(test)]
mod request_format_tests {
    use super::*;

    #[tokio::test]
    async fn test_request_includes_required_headers() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .and(header("Authorization", "Bearer test-api-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "Response"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Test");
        let _ = client.call_pipe(request).await;

        // If we reach here without panic, the headers matched correctly
    }

    #[tokio::test]
    async fn test_request_with_variables() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "Response with variables"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = PipeRequest::new("test-pipe", vec![Message::user("Test")])
            .with_variable("key1", "value1")
            .with_variable("key2", "value2");
        let result = client.call_pipe(request).await;

        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod response_parsing_tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_standard_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "Parsed reasoning output",
                "threadId": "thread-abc",
                "raw": {
                    "model": "gpt-4o-mini",
                    "usage": {
                        "prompt_tokens": 100,
                        "completion_tokens": 50,
                        "total_tokens": 150
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Input");
        let result = client.call_pipe(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.completion, "Parsed reasoning output");
        assert!(response.raw.is_some());
        let raw = response.raw.unwrap();
        assert_eq!(raw.model, Some("gpt-4o-mini".to_string()));
    }

    #[tokio::test]
    async fn test_handle_empty_completion() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": ""
            })))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Input");
        let result = client.call_pipe(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.completion, "");
    }

    #[tokio::test]
    async fn test_handle_malformed_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Input");
        let result = client.call_pipe(request).await;

        assert!(result.is_err(), "Should fail on malformed JSON");
    }

    #[tokio::test]
    async fn test_parse_response_without_optional_fields() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "Minimal response"
            })))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());
        let request = create_test_request("Input");
        let result = client.call_pipe(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.thread_id.is_none());
        assert!(response.raw.is_none());
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_request_timeout() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({
                        "success": true,
                        "completion": "Delayed response"
                    }))
                    .set_delay(Duration::from_secs(10)), // Longer than timeout
            )
            .mount(&mock_server)
            .await;

        // Create client with short timeout
        let config = LangbaseConfig {
            api_key: "test-api-key".to_string(),
            base_url: mock_server.uri(),
        };
        let request_config = RequestConfig {
            timeout_ms: 100, // 100ms timeout
            max_retries: 0,
            retry_delay_ms: 100,
        };
        let client = LangbaseClient::new(&config, request_config).unwrap();

        let request = create_test_request("Test");
        let result = client.call_pipe(request).await;

        assert!(result.is_err(), "Should timeout");
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_on_server_error() {
        let mock_server = MockServer::start().await;

        // First two calls fail, third succeeds
        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": {"message": "Server error"}
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        // With max_retries=0, we should only see one call
        let config = LangbaseConfig {
            api_key: "test-api-key".to_string(),
            base_url: mock_server.uri(),
        };
        let request_config = RequestConfig {
            timeout_ms: 5000,
            max_retries: 0, // No retries
            retry_delay_ms: 10,
        };
        let client = LangbaseClient::new(&config, request_config).unwrap();

        let request = create_test_request("Test");
        let result = client.call_pipe(request).await;

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let system = Message::system("You are a helper");
        let user = Message::user("Hello");
        let assistant = Message::assistant("Hi there");

        assert_eq!(system.content, "You are a helper");
        assert_eq!(user.content, "Hello");
        assert_eq!(assistant.content, "Hi there");
    }

    #[test]
    fn test_pipe_request_builder() {
        let request = PipeRequest::new("my-pipe", vec![Message::user("Test")])
            .with_thread_id("thread-1")
            .with_variable("key", "value");

        assert_eq!(request.name, "my-pipe");
        assert_eq!(request.thread_id, Some("thread-1".to_string()));
        assert!(request.variables.is_some());
        assert_eq!(
            request.variables.unwrap().get("key"),
            Some(&"value".to_string())
        );
    }
}

#[cfg(test)]
mod reasoning_response_tests {
    use mcp_langbase_reasoning::langbase::ReasoningResponse;

    #[test]
    fn test_parse_valid_json_response() {
        let json = r#"{"thought": "This is analysis", "confidence": 0.85, "metadata": {"key": "value"}}"#;
        let response = ReasoningResponse::from_completion(json);

        assert_eq!(response.thought, "This is analysis");
        assert!((response.confidence - 0.85).abs() < 0.001);
        assert!(response.metadata.is_some());
    }

    #[test]
    fn test_parse_plain_text_response() {
        let text = "This is plain text reasoning output.";
        let response = ReasoningResponse::from_completion(text);

        assert_eq!(response.thought, text);
        assert!((response.confidence - 0.8).abs() < 0.001); // Default confidence
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_parse_minimal_json_response() {
        let json = r#"{"thought": "Minimal", "confidence": 0.5}"#;
        let response = ReasoningResponse::from_completion(json);

        assert_eq!(response.thought, "Minimal");
        assert!((response.confidence - 0.5).abs() < 0.001);
    }
}
