//! Unit tests for Langbase API types.
//!
//! Tests request/response types, serialization, deserialization,
//! and builder patterns for Langbase pipe communication.

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
    assert_eq!(
        req.variables.as_ref().unwrap().get("key1"),
        Some(&"value1".to_string())
    );
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

// Serialization tests
#[test]
fn test_message_serialize() {
    let msg = Message::system("Test system message");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("system"));
    assert!(json.contains("Test system message"));
}

#[test]
fn test_message_deserialize() {
    let json = r#"{"role": "user", "content": "Hello"}"#;
    let msg: Message = serde_json::from_str(json).unwrap();
    assert!(matches!(msg.role, MessageRole::User));
    assert_eq!(msg.content, "Hello");
}

#[test]
fn test_pipe_request_serialize() {
    let req = PipeRequest::new("test-pipe", vec![Message::user("Test")])
        .with_variable("key", "value")
        .with_thread_id("thread-1");
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("test-pipe"));
    assert!(json.contains("threadId"));
    assert!(json.contains("thread-1"));
}

#[test]
fn test_pipe_response_deserialize() {
    let json = r#"{
        "success": true,
        "completion": "Response text",
        "threadId": "t-123",
        "raw": {
            "model": "gpt-4",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        }
    }"#;
    let resp: PipeResponse = serde_json::from_str(json).unwrap();
    assert!(resp.success);
    assert_eq!(resp.completion, "Response text");
    assert_eq!(resp.thread_id, Some("t-123".to_string()));
    assert!(resp.raw.is_some());
    let raw = resp.raw.unwrap();
    assert_eq!(raw.model, Some("gpt-4".to_string()));
    let usage = raw.usage.unwrap();
    assert_eq!(usage.prompt_tokens, Some(10));
    assert_eq!(usage.completion_tokens, Some(20));
    assert_eq!(usage.total_tokens, Some(30));
}

#[test]
fn test_pipe_response_deserialize_minimal() {
    let json = r#"{"success": false, "completion": ""}"#;
    let resp: PipeResponse = serde_json::from_str(json).unwrap();
    assert!(!resp.success);
    assert!(resp.thread_id.is_none());
    assert!(resp.raw.is_none());
}

#[test]
fn test_create_pipe_request_serialize() {
    let req = CreatePipeRequest::new("new-pipe")
        .with_description("Test pipe")
        .with_status(PipeStatus::Public)
        .with_model("openai:gpt-4")
        .with_temperature(0.7)
        .with_max_tokens(1000);
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("new-pipe"));
    assert!(json.contains("Test pipe"));
    assert!(json.contains("public"));
    assert!(json.contains("0.7"));
    assert!(json.contains("1000"));
}

#[test]
fn test_create_pipe_response_deserialize() {
    let json = r#"{
        "name": "my-pipe",
        "description": "A test pipe",
        "status": "public",
        "owner_login": "testuser",
        "url": "https://api.langbase.com/pipe/my-pipe",
        "type": "chat",
        "api_key": "secret-key"
    }"#;
    let resp: CreatePipeResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.name, "my-pipe");
    assert_eq!(resp.description, Some("A test pipe".to_string()));
    assert_eq!(resp.status, "public");
    assert_eq!(resp.owner_login, "testuser");
    assert_eq!(resp.pipe_type, "chat");
    assert_eq!(resp.api_key, "secret-key");
}

#[test]
fn test_reasoning_response_serialize() {
    let resp = ReasoningResponse {
        thought: "A reasoned thought".to_string(),
        confidence: 0.85,
        metadata: Some(serde_json::json!({"analysis": "complete"})),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("A reasoned thought"));
    assert!(json.contains("0.85"));
    assert!(json.contains("analysis"));
}

#[test]
fn test_pipe_status_serialize() {
    let public = PipeStatus::Public;
    let private = PipeStatus::Private;
    assert_eq!(serde_json::to_string(&public).unwrap(), "\"public\"");
    assert_eq!(serde_json::to_string(&private).unwrap(), "\"private\"");
}

#[test]
fn test_message_role_serialize() {
    assert_eq!(
        serde_json::to_string(&MessageRole::System).unwrap(),
        "\"system\""
    );
    assert_eq!(
        serde_json::to_string(&MessageRole::User).unwrap(),
        "\"user\""
    );
    assert_eq!(
        serde_json::to_string(&MessageRole::Assistant).unwrap(),
        "\"assistant\""
    );
}

#[test]
fn test_message_role_deserialize() {
    let system: MessageRole = serde_json::from_str("\"system\"").unwrap();
    let user: MessageRole = serde_json::from_str("\"user\"").unwrap();
    let assistant: MessageRole = serde_json::from_str("\"assistant\"").unwrap();
    assert!(matches!(system, MessageRole::System));
    assert!(matches!(user, MessageRole::User));
    assert!(matches!(assistant, MessageRole::Assistant));
}

#[test]
fn test_pipe_status_deserialize() {
    let public: PipeStatus = serde_json::from_str("\"public\"").unwrap();
    let private: PipeStatus = serde_json::from_str("\"private\"").unwrap();
    assert!(matches!(public, PipeStatus::Public));
    assert!(matches!(private, PipeStatus::Private));
}

#[test]
fn test_raw_response_deserialize() {
    let json = r#"{"model": "gpt-4", "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}}"#;
    let raw: RawResponse = serde_json::from_str(json).unwrap();
    assert_eq!(raw.model, Some("gpt-4".to_string()));
    let usage = raw.usage.unwrap();
    assert_eq!(usage.prompt_tokens, Some(10));
    assert_eq!(usage.completion_tokens, Some(20));
    assert_eq!(usage.total_tokens, Some(30));
}

#[test]
fn test_usage_deserialize() {
    let json = r#"{"prompt_tokens": 100, "completion_tokens": 200, "total_tokens": 300}"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.prompt_tokens, Some(100));
    assert_eq!(usage.completion_tokens, Some(200));
    assert_eq!(usage.total_tokens, Some(300));
}

#[test]
fn test_usage_partial_deserialize() {
    let json = r#"{"prompt_tokens": 50}"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.prompt_tokens, Some(50));
    assert!(usage.completion_tokens.is_none());
    assert!(usage.total_tokens.is_none());
}

#[test]
fn test_raw_response_minimal() {
    let json = r#"{}"#;
    let raw: RawResponse = serde_json::from_str(json).unwrap();
    assert!(raw.model.is_none());
    assert!(raw.usage.is_none());
}

// ========================================================================
// Phase 4: Bias & Fallacy Detection Response Tests
// ========================================================================

#[test]
fn test_detected_bias_deserialize() {
    let json = r#"{
        "bias_type": "confirmation_bias",
        "severity": 4,
        "confidence": 0.85,
        "explanation": "The argument only considers evidence that supports the conclusion",
        "remediation": "Consider evidence that might contradict your conclusion",
        "excerpt": "This proves our hypothesis is correct"
    }"#;
    let bias: DetectedBias = serde_json::from_str(json).unwrap();
    assert_eq!(bias.bias_type, "confirmation_bias");
    assert_eq!(bias.severity, 4);
    assert_eq!(bias.confidence, 0.85);
    assert!(bias.explanation.contains("evidence"));
    assert!(bias.remediation.is_some());
    assert!(bias.excerpt.is_some());
}

#[test]
fn test_detected_bias_serialize() {
    let bias = DetectedBias {
        bias_type: "anchoring_bias".to_string(),
        severity: 3,
        confidence: 0.75,
        explanation: "Over-reliance on initial information".to_string(),
        remediation: Some("Consider multiple reference points".to_string()),
        excerpt: None,
    };
    let json = serde_json::to_string(&bias).unwrap();
    assert!(json.contains("anchoring_bias"));
    assert!(json.contains("\"severity\":3"));
    assert!(json.contains("0.75"));
    assert!(!json.contains("excerpt")); // Should be skipped when None
}

#[test]
fn test_bias_detection_response_from_json() {
    let json = r#"{
        "detections": [
            {
                "bias_type": "availability_heuristic",
                "severity": 2,
                "confidence": 0.6,
                "explanation": "Recent events given too much weight"
            }
        ],
        "reasoning_quality": 0.7,
        "overall_assessment": "Minor bias detected in reasoning"
    }"#;
    let resp = BiasDetectionResponse::from_completion(json);
    assert_eq!(resp.detections.len(), 1);
    assert_eq!(resp.detections[0].bias_type, "availability_heuristic");
    assert_eq!(resp.reasoning_quality, 0.7);
    assert!(resp.overall_assessment.contains("Minor bias"));
}

#[test]
fn test_bias_detection_response_fallback() {
    let text = "Unable to parse as JSON, this is plain text";
    let resp = BiasDetectionResponse::from_completion(text);
    assert!(resp.detections.is_empty());
    assert_eq!(resp.reasoning_quality, 0.5);
    assert_eq!(resp.overall_assessment, text);
}

#[test]
fn test_bias_detection_response_with_metadata() {
    let json = r#"{
        "detections": [],
        "reasoning_quality": 0.95,
        "overall_assessment": "No biases detected",
        "metadata": {"analysis_time_ms": 150}
    }"#;
    let resp = BiasDetectionResponse::from_completion(json);
    assert!(resp.detections.is_empty());
    assert_eq!(resp.reasoning_quality, 0.95);
    assert!(resp.metadata.is_some());
}

#[test]
fn test_detected_fallacy_deserialize() {
    let json = r#"{
        "fallacy_type": "ad_hominem",
        "category": "informal",
        "severity": 4,
        "confidence": 0.9,
        "explanation": "Attacks the person rather than their argument",
        "remediation": "Focus on the argument itself, not the person making it",
        "excerpt": "You can't trust his argument because he's not an expert"
    }"#;
    let fallacy: DetectedFallacy = serde_json::from_str(json).unwrap();
    assert_eq!(fallacy.fallacy_type, "ad_hominem");
    assert_eq!(fallacy.category, "informal");
    assert_eq!(fallacy.severity, 4);
    assert_eq!(fallacy.confidence, 0.9);
    assert!(fallacy.remediation.is_some());
    assert!(fallacy.excerpt.is_some());
}

#[test]
fn test_detected_fallacy_serialize() {
    let fallacy = DetectedFallacy {
        fallacy_type: "false_dichotomy".to_string(),
        category: "informal".to_string(),
        severity: 3,
        confidence: 0.8,
        explanation: "Presents only two options when more exist".to_string(),
        remediation: None,
        excerpt: None,
    };
    let json = serde_json::to_string(&fallacy).unwrap();
    assert!(json.contains("false_dichotomy"));
    assert!(json.contains("informal"));
    assert!(!json.contains("remediation")); // Should be skipped when None
    assert!(!json.contains("excerpt")); // Should be skipped when None
}

#[test]
fn test_fallacy_detection_response_from_json() {
    let json = r#"{
        "detections": [
            {
                "fallacy_type": "straw_man",
                "category": "informal",
                "severity": 4,
                "confidence": 0.85,
                "explanation": "Misrepresents opponent's position"
            },
            {
                "fallacy_type": "affirming_consequent",
                "category": "formal",
                "severity": 3,
                "confidence": 0.7,
                "explanation": "Invalid logical form: If P then Q, Q, therefore P"
            }
        ],
        "argument_validity": 0.4,
        "overall_assessment": "Multiple fallacies detected affecting argument validity"
    }"#;
    let resp = FallacyDetectionResponse::from_completion(json);
    assert_eq!(resp.detections.len(), 2);
    assert_eq!(resp.detections[0].fallacy_type, "straw_man");
    assert_eq!(resp.detections[1].category, "formal");
    assert_eq!(resp.argument_validity, 0.4);
}

#[test]
fn test_fallacy_detection_response_fallback() {
    let text = "Plain text response without JSON";
    let resp = FallacyDetectionResponse::from_completion(text);
    assert!(resp.detections.is_empty());
    assert_eq!(resp.argument_validity, 0.5);
    assert_eq!(resp.overall_assessment, text);
}

#[test]
fn test_fallacy_detection_response_empty_detections() {
    let json = r#"{
        "detections": [],
        "argument_validity": 0.95,
        "overall_assessment": "No logical fallacies detected"
    }"#;
    let resp = FallacyDetectionResponse::from_completion(json);
    assert!(resp.detections.is_empty());
    assert_eq!(resp.argument_validity, 0.95);
}
