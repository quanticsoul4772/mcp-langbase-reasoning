//! Integration tests for full MCP → Mode → Langbase → Storage flow
//!
//! These tests verify the end-to-end behavior of the reasoning system,
//! ensuring all components work together correctly.

use serde_json::json;
use tempfile::tempdir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use mcp_langbase_reasoning::config::{
    Config, DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig, RequestConfig,
};
use mcp_langbase_reasoning::langbase::LangbaseClient;
use mcp_langbase_reasoning::modes::{
    DivergentMode, DivergentParams, LinearMode, LinearParams, ReflectionMode, ReflectionParams,
    TreeMode, TreeParams,
};
use mcp_langbase_reasoning::storage::{SqliteStorage, Storage};

/// Create test configuration with mock server URL
fn create_test_config(mock_url: &str, db_path: std::path::PathBuf) -> Config {
    Config {
        langbase: LangbaseConfig {
            api_key: "test-api-key".to_string(),
            base_url: mock_url.to_string(),
        },
        database: DatabaseConfig {
            path: db_path,
            max_connections: 1,
        },
        logging: LoggingConfig {
            level: "debug".to_string(),
            format: LogFormat::Pretty,
        },
        request: RequestConfig {
            timeout_ms: 5000,
            max_retries: 0,
            retry_delay_ms: 100,
        },
        pipes: PipeConfig {
            linear: "linear-reasoning-v1".to_string(),
            tree: "tree-reasoning-v1".to_string(),
            divergent: "divergent-reasoning-v1".to_string(),
            reflection: "reflection-v1".to_string(),
            auto_router: "mode-router-v1".to_string(),
            auto: None,
            backtracking: None,
            got: None,
            detection: None,
            decision: None,
            evidence: None,
        },
        error_handling: ErrorHandlingConfig::default(),
    }
}

/// Create test storage with temporary database
async fn create_test_storage(db_path: std::path::PathBuf) -> SqliteStorage {
    let config = DatabaseConfig {
        path: db_path,
        max_connections: 1,
    };
    SqliteStorage::new(&config)
        .await
        .expect("Failed to create storage")
}

/// Mock response helpers
fn mock_linear_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": r#"{"thought": "Linear reasoning result", "confidence": 0.85, "metadata": {}}"#,
        "threadId": "thread-123"
    }))
}

fn mock_tree_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": json!({
            "branches": [
                {"thought": "Branch 1", "confidence": 0.8, "rationale": "First approach"},
                {"thought": "Branch 2", "confidence": 0.75, "rationale": "Second approach"}
            ],
            "recommended_branch": 0,
            "metadata": {}
        }).to_string(),
        "threadId": "thread-456"
    }))
}

fn mock_divergent_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": json!({
            "perspectives": [
                {"thought": "Creative view 1", "novelty": 0.9, "viability": 0.7},
                {"thought": "Creative view 2", "novelty": 0.85, "viability": 0.8}
            ],
            "synthesis": "Combined insight",
            "metadata": {}
        }).to_string(),
        "threadId": "thread-789"
    }))
}

fn mock_reflection_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": json!({
            "analysis": "Meta-cognitive assessment",
            "strengths": ["Clear logic"],
            "weaknesses": ["Limited scope"],
            "recommendations": ["Broaden analysis"],
            "confidence": 0.8,
            "metadata": {}
        }).to_string(),
        "threadId": "thread-abc"
    }))
}

fn mock_bias_detection_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": json!({
            "detections": [
                {
                    "bias_type": "confirmation_bias",
                    "severity": 3,
                    "confidence": 0.85,
                    "explanation": "The argument selectively cites evidence that supports the conclusion",
                    "remediation": "Consider evidence that contradicts the conclusion",
                    "excerpt": "All studies show that..."
                },
                {
                    "bias_type": "anchoring_bias",
                    "severity": 2,
                    "confidence": 0.7,
                    "explanation": "The initial estimate heavily influences the final conclusion",
                    "remediation": "Re-evaluate from multiple starting points"
                }
            ],
            "reasoning_quality": 0.6,
            "overall_assessment": "The reasoning shows moderate bias with potential for improvement",
            "metadata": {}
        }).to_string(),
        "threadId": "thread-bias-123"
    }))
}

fn mock_fallacy_detection_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": json!({
            "detections": [
                {
                    "fallacy_type": "ad_hominem",
                    "category": "informal",
                    "severity": 4,
                    "confidence": 0.9,
                    "explanation": "The argument attacks the person rather than addressing their claims",
                    "remediation": "Focus on the argument's content, not the speaker",
                    "excerpt": "You can't trust him because..."
                },
                {
                    "fallacy_type": "affirming_consequent",
                    "category": "formal",
                    "severity": 3,
                    "confidence": 0.75,
                    "explanation": "Invalid logical structure: If P then Q, Q, therefore P",
                    "remediation": "Restructure the argument to avoid this logical error"
                }
            ],
            "argument_validity": 0.4,
            "overall_assessment": "The argument contains significant logical errors",
            "metadata": {}
        }).to_string(),
        "threadId": "thread-fallacy-456"
    }))
}

fn mock_empty_detection_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "completion": json!({
            "detections": [],
            "reasoning_quality": 0.95,
            "overall_assessment": "No significant biases detected",
            "metadata": {}
        }).to_string(),
        "threadId": "thread-empty-789"
    }))
}

#[cfg(test)]
mod linear_mode_integration {
    use super::*;

    #[tokio::test]
    async fn test_linear_mode_full_flow() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_linear_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = LinearMode::new(storage.clone(), langbase, &config);

        let params = LinearParams::new("Test thought for linear reasoning").with_confidence(0.8);

        let result = mode.process(params).await;
        assert!(
            result.is_ok(),
            "Linear mode should succeed: {:?}",
            result.err()
        );

        let linear_result = result.unwrap();
        assert!(!linear_result.thought_id.is_empty());
        assert_eq!(linear_result.content, "Linear reasoning result");
        assert!((linear_result.confidence - 0.85).abs() < 0.01);

        // Verify thought was stored
        let stored = storage.get_thought(&linear_result.thought_id).await;
        assert!(stored.is_ok());
        let stored_thought = stored.unwrap();
        assert!(stored_thought.is_some());
        assert_eq!(stored_thought.unwrap().content, linear_result.content);
    }

    #[tokio::test]
    async fn test_linear_mode_with_session_continuation() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_linear_response())
            .expect(2)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = LinearMode::new(storage.clone(), langbase, &config);

        // First thought
        let params1 = LinearParams::new("First thought");
        let result1 = mode.process(params1).await;
        assert!(result1.is_ok());
        let thought1 = result1.unwrap();

        // Second thought in same session
        let params2 = LinearParams::new("Second thought").with_session(&thought1.session_id);

        let result2 = mode.process(params2).await;
        assert!(result2.is_ok());
        let thought2 = result2.unwrap();

        // Both should be in same session
        assert_eq!(thought1.session_id, thought2.session_id);

        // Verify session has both thoughts
        let history = storage.get_session_thoughts(&thought1.session_id).await;
        assert!(history.is_ok());
        assert_eq!(history.unwrap().len(), 2);
    }
}

#[cfg(test)]
mod tree_mode_integration {
    use super::*;

    #[tokio::test]
    async fn test_tree_mode_creates_branches() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_tree_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = TreeMode::new(storage.clone(), langbase, &config);

        let params = TreeParams::new("Explore multiple approaches").with_num_branches(2);

        let result = mode.process(params).await;
        assert!(
            result.is_ok(),
            "Tree mode should succeed: {:?}",
            result.err()
        );

        let tree_result = result.unwrap();
        assert!(!tree_result.thought_id.is_empty());

        // Verify branches were created in storage
        let branches = storage.get_session_branches(&tree_result.session_id).await;
        assert!(branches.is_ok());
        // Should have branches
        assert!(!branches.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_tree_mode_branch_focus() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_tree_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = TreeMode::new(storage.clone(), langbase, &config);

        // Create initial thought
        let params = TreeParams::new("Initial exploration");

        let result = mode.process(params).await;
        assert!(result.is_ok());

        let tree_result = result.unwrap();

        // List and focus on a branch
        let branches = storage.get_session_branches(&tree_result.session_id).await;
        assert!(branches.is_ok());

        let branch_list = branches.unwrap();
        if !branch_list.is_empty() {
            let focused = mode
                .focus_branch(&tree_result.session_id, &branch_list[0].id)
                .await;
            assert!(focused.is_ok());
        }
    }
}

#[cfg(test)]
mod divergent_mode_integration {
    use super::*;

    #[tokio::test]
    async fn test_divergent_mode_generates_perspectives() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_divergent_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = DivergentMode::new(storage.clone(), langbase, &config);

        let params = DivergentParams::new("Generate creative solutions").with_num_perspectives(3);

        let result = mode.process(params).await;
        assert!(
            result.is_ok(),
            "Divergent mode should succeed: {:?}",
            result.err()
        );

        let divergent_result = result.unwrap();
        assert!(!divergent_result.synthesis_thought_id.is_empty());
    }

    #[tokio::test]
    async fn test_divergent_mode_with_constraints() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_divergent_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = DivergentMode::new(storage.clone(), langbase, &config);

        let params = DivergentParams::new("Creative problem solving with constraints")
            .with_num_perspectives(2);

        let result = mode.process(params).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod reflection_mode_integration {
    use super::*;

    #[tokio::test]
    async fn test_reflection_mode_analyzes_content() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_reflection_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = ReflectionMode::new(storage.clone(), langbase, &config);

        let params = ReflectionParams::for_content("Analyze this reasoning process");

        let result = mode.process(params).await;
        assert!(
            result.is_ok(),
            "Reflection mode should succeed: {:?}",
            result.err()
        );

        let analysis = result.unwrap();
        assert!(!analysis.reflection_thought_id.is_empty());
    }

    #[tokio::test]
    async fn test_reflection_mode_evaluates_session() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        // First create some content to reflect on
        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_linear_response())
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let linear_mode = LinearMode::new(storage.clone(), langbase.clone(), &config);
        let linear_params = LinearParams::new("Initial reasoning");

        let linear_result = linear_mode.process(linear_params).await;
        assert!(linear_result.is_ok());
        let thought = linear_result.unwrap();

        // Now evaluate the session - this is a local operation, no HTTP call needed
        let reflection_mode = ReflectionMode::new(storage.clone(), langbase, &config);
        let eval_result = reflection_mode.evaluate_session(&thought.session_id).await;
        assert!(
            eval_result.is_ok(),
            "Session evaluation should succeed: {:?}",
            eval_result.err()
        );

        let eval = eval_result.unwrap();
        assert_eq!(eval.total_thoughts, 1);
        assert!(eval.average_confidence > 0.0);
    }
}

#[cfg(test)]
mod multi_mode_integration {
    use super::*;

    #[tokio::test]
    async fn test_linear_then_reflection_flow() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        // Setup mock for linear mode
        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_linear_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        // Step 1: Linear reasoning
        let linear_mode = LinearMode::new(storage.clone(), langbase.clone(), &config);
        let linear_params = LinearParams::new("Step-by-step analysis of problem X");

        let linear_result = linear_mode.process(linear_params).await;
        assert!(linear_result.is_ok());
        let linear_thought = linear_result.unwrap();

        // Reset and setup reflection mock
        mock_server.reset().await;
        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_reflection_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        // Step 2: Reflect on the linear reasoning
        let reflection_mode = ReflectionMode::new(storage.clone(), langbase, &config);
        let reflection_params = ReflectionParams::for_thought(&linear_thought.thought_id);

        let reflection_result = reflection_mode.process(reflection_params).await;
        assert!(reflection_result.is_ok());

        // Verify both thoughts exist in storage
        let linear_stored = storage.get_thought(&linear_thought.thought_id).await;
        assert!(linear_stored.is_ok());
        assert!(linear_stored.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_tree_exploration_then_focus() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_tree_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = TreeMode::new(storage.clone(), langbase, &config);

        // Step 1: Create tree with branches
        let params = TreeParams::new("Explore solution space").with_num_branches(3);

        let result = mode.process(params).await;
        assert!(result.is_ok());
        let tree_result = result.unwrap();

        // Step 2: List branches
        let branches = storage.get_session_branches(&tree_result.session_id).await;
        assert!(branches.is_ok());
        let branch_list = branches.unwrap();

        // Step 3: Focus on first branch (if available)
        if !branch_list.is_empty() {
            let focus_result = mode
                .focus_branch(&tree_result.session_id, &branch_list[0].id)
                .await;
            assert!(focus_result.is_ok());

            // Verify branch state
            let branch = storage.get_branch(&branch_list[0].id).await;
            assert!(branch.is_ok());
        }
    }
}

#[cfg(test)]
mod error_handling_integration {
    use super::*;

    #[tokio::test]
    async fn test_handles_langbase_error_gracefully() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": {"message": "Internal server error"}
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let params = LinearParams::new("This should fail");

        let result = mode.process(params).await;
        assert!(result.is_err(), "Should return error on Langbase failure");
    }

    #[tokio::test]
    async fn test_handles_invalid_response_gracefully() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "not valid json for reasoning"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = LinearMode::new(storage, langbase, &config);

        let params = LinearParams::new("Test with invalid response");

        // Should handle gracefully - the mode should parse plain text as fallback
        let result = mode.process(params).await;
        // Just verify it doesn't panic - behavior depends on implementation
        let _ = result;
    }
}

#[cfg(test)]
mod storage_persistence_integration {
    use super::*;

    #[tokio::test]
    async fn test_thoughts_persist_across_operations() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_linear_response())
            .expect(3)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = LinearMode::new(storage.clone(), langbase, &config);

        // Create multiple thoughts
        let mut thought_ids = Vec::new();
        for i in 1..=3 {
            let params = LinearParams::new(format!("Thought number {}", i));

            let result = mode.process(params).await;
            assert!(result.is_ok());
            thought_ids.push(result.unwrap().thought_id);
        }

        // Verify all thoughts are retrievable
        for id in &thought_ids {
            let thought = storage.get_thought(id).await;
            assert!(thought.is_ok());
            assert!(thought.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn test_session_history_preserved() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_linear_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let mode = LinearMode::new(storage.clone(), langbase, &config);

        let params = LinearParams::new("Test thought");

        let result = mode.process(params).await;
        assert!(result.is_ok());
        let linear_result = result.unwrap();

        // Verify session exists
        let session = storage.get_session(&linear_result.session_id).await;
        assert!(session.is_ok());
        assert!(session.unwrap().is_some());

        // Verify history is retrievable
        let history = storage
            .get_session_thoughts(&linear_result.session_id)
            .await;
        assert!(history.is_ok());
        assert!(!history.unwrap().is_empty());
    }
}

// ============================================================================
// Bias & Fallacy Detection Integration Tests
// ============================================================================

#[cfg(test)]
mod bias_detection_integration {
    use super::*;
    use mcp_langbase_reasoning::langbase::{BiasDetectionResponse, Message, PipeRequest};
    use mcp_langbase_reasoning::storage::{Detection, DetectionType, Session};

    #[tokio::test]
    async fn test_bias_detection_parses_response_correctly() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_bias_detection_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        // Call the pipe directly to test response parsing
        let messages = vec![
            Message::system("Detect biases"),
            Message::user("Test content with potential biases"),
        ];
        let request = PipeRequest::new("detect-biases-v1", messages);
        let response = langbase.call_pipe(request).await;

        assert!(response.is_ok(), "Langbase call should succeed");
        let pipe_response = response.unwrap();

        // Parse the response
        let bias_response = BiasDetectionResponse::from_completion(&pipe_response.completion);

        assert_eq!(bias_response.detections.len(), 2);
        assert_eq!(bias_response.detections[0].bias_type, "confirmation_bias");
        assert_eq!(bias_response.detections[0].severity, 3);
        assert!((bias_response.detections[0].confidence - 0.85).abs() < 0.01);
        assert_eq!(bias_response.detections[1].bias_type, "anchoring_bias");
        assert!((bias_response.reasoning_quality - 0.6).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_bias_detection_handles_empty_response() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_empty_detection_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let messages = vec![
            Message::system("Detect biases"),
            Message::user("Clean content with no biases"),
        ];
        let request = PipeRequest::new("detect-biases-v1", messages);
        let response = langbase.call_pipe(request).await;

        assert!(response.is_ok());
        let pipe_response = response.unwrap();
        let bias_response = BiasDetectionResponse::from_completion(&pipe_response.completion);

        assert!(bias_response.detections.is_empty());
        assert!((bias_response.reasoning_quality - 0.95).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_bias_detection_stores_results() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let storage = create_test_storage(db_path).await;

        // First create a session (FK constraint)
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        // Create and store a detection with valid session FK
        let detection = Detection::new(
            DetectionType::Bias,
            "confirmation_bias",
            3,
            0.85,
            "Selectively citing evidence",
        )
        .with_session(&session.id)
        .with_remediation("Consider contradictory evidence");

        let result = storage.create_detection(&detection).await;
        assert!(
            result.is_ok(),
            "Detection creation failed: {:?}",
            result.err()
        );

        // Retrieve and verify
        let stored = storage.get_detection(&detection.id).await;
        assert!(stored.is_ok());
        let stored_detection = stored.unwrap();
        assert!(stored_detection.is_some());

        let d = stored_detection.unwrap();
        assert_eq!(d.detected_issue, "confirmation_bias");
        assert_eq!(d.severity, 3);
        assert!((d.confidence - 0.85).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_bias_detection_retrieves_by_session() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let storage = create_test_storage(db_path).await;

        // First create a session (FK constraint)
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();
        let session_id = &session.id;

        // Create multiple detections for the same session
        let detection1 = Detection::new(
            DetectionType::Bias,
            "confirmation_bias",
            3,
            0.85,
            "First bias",
        )
        .with_session(session_id);

        let detection2 =
            Detection::new(DetectionType::Bias, "anchoring_bias", 2, 0.7, "Second bias")
                .with_session(session_id);

        storage.create_detection(&detection1).await.unwrap();
        storage.create_detection(&detection2).await.unwrap();

        // Retrieve by session
        let session_detections = storage.get_session_detections(session_id).await;
        assert!(session_detections.is_ok());
        assert_eq!(session_detections.unwrap().len(), 2);
    }
}

#[cfg(test)]
mod fallacy_detection_integration {
    use super::*;
    use mcp_langbase_reasoning::langbase::{FallacyDetectionResponse, Message, PipeRequest};
    use mcp_langbase_reasoning::storage::{Detection, DetectionType, Session, Thought};

    #[tokio::test]
    async fn test_fallacy_detection_parses_response_correctly() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(mock_fallacy_detection_response())
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let messages = vec![
            Message::system("Detect fallacies"),
            Message::user("Test argument with fallacies"),
        ];
        let request = PipeRequest::new("detect-fallacies-v1", messages);
        let response = langbase.call_pipe(request).await;

        assert!(response.is_ok());
        let pipe_response = response.unwrap();
        let fallacy_response = FallacyDetectionResponse::from_completion(&pipe_response.completion);

        assert_eq!(fallacy_response.detections.len(), 2);
        assert_eq!(fallacy_response.detections[0].fallacy_type, "ad_hominem");
        assert_eq!(fallacy_response.detections[0].category, "informal");
        assert_eq!(fallacy_response.detections[0].severity, 4);
        assert_eq!(
            fallacy_response.detections[1].fallacy_type,
            "affirming_consequent"
        );
        assert_eq!(fallacy_response.detections[1].category, "formal");
        assert!((fallacy_response.argument_validity - 0.4).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_fallacy_detection_stores_with_category() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let storage = create_test_storage(db_path).await;

        // First create a session (FK constraint)
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        // Create and store a fallacy detection with category metadata
        let detection = Detection::new(
            DetectionType::Fallacy,
            "ad_hominem",
            4,
            0.9,
            "Attacks the person instead of the argument",
        )
        .with_session(&session.id)
        .with_metadata(serde_json::json!({
            "category": "informal",
            "excerpt": "You can't trust him because..."
        }));

        let result = storage.create_detection(&detection).await;
        assert!(
            result.is_ok(),
            "Detection creation failed: {:?}",
            result.err()
        );

        // Retrieve and verify metadata
        let stored = storage.get_detection(&detection.id).await;
        assert!(stored.is_ok());
        let stored_detection = stored.unwrap().unwrap();

        assert!(stored_detection.metadata.is_some());
        let meta = stored_detection.metadata.unwrap();
        assert_eq!(meta["category"], "informal");
    }

    #[tokio::test]
    async fn test_fallacy_detection_filters_by_type() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let storage = create_test_storage(db_path).await;

        // First create a session (FK constraint)
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();
        let session_id = &session.id;

        // Create bias and fallacy detections
        let bias = Detection::new(DetectionType::Bias, "confirmation_bias", 3, 0.8, "Bias")
            .with_session(session_id);

        let fallacy = Detection::new(DetectionType::Fallacy, "ad_hominem", 4, 0.9, "Fallacy")
            .with_session(session_id);

        storage.create_detection(&bias).await.unwrap();
        storage.create_detection(&fallacy).await.unwrap();

        // Retrieve by type
        let fallacies = storage
            .get_session_detections_by_type(session_id, DetectionType::Fallacy)
            .await;
        assert!(fallacies.is_ok());
        let fallacy_list = fallacies.unwrap();
        assert_eq!(fallacy_list.len(), 1);
        assert_eq!(fallacy_list[0].detected_issue, "ad_hominem");

        let biases = storage
            .get_session_detections_by_type(session_id, DetectionType::Bias)
            .await;
        assert!(biases.is_ok());
        let bias_list = biases.unwrap();
        assert_eq!(bias_list.len(), 1);
        assert_eq!(bias_list[0].detected_issue, "confirmation_bias");
    }

    #[tokio::test]
    async fn test_fallacy_detection_retrieves_by_thought() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let storage = create_test_storage(db_path).await;

        // First create a session and thought (FK constraints)
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let thought = Thought::new(&session.id, "Test thought content", "linear");
        storage.create_thought(&thought).await.unwrap();
        let thought_id = &thought.id;

        // Create detections for a specific thought
        let detection1 = Detection::new(DetectionType::Fallacy, "straw_man", 3, 0.8, "Straw man")
            .with_thought(thought_id);

        let detection2 = Detection::new(
            DetectionType::Fallacy,
            "false_dichotomy",
            2,
            0.7,
            "False choice",
        )
        .with_thought(thought_id);

        storage.create_detection(&detection1).await.unwrap();
        storage.create_detection(&detection2).await.unwrap();

        // Retrieve by thought
        let thought_detections = storage.get_thought_detections(thought_id).await;
        assert!(thought_detections.is_ok());
        assert_eq!(thought_detections.unwrap().len(), 2);
    }
}

#[cfg(test)]
mod detection_error_handling {
    use super::*;
    use mcp_langbase_reasoning::langbase::{BiasDetectionResponse, FallacyDetectionResponse};

    #[tokio::test]
    async fn test_bias_detection_handles_invalid_json() {
        // Test fallback behavior for non-JSON responses
        let invalid_response = "This is not valid JSON but some plain text analysis";
        let bias_response = BiasDetectionResponse::from_completion(invalid_response);

        // Should return empty detections with the raw text as assessment
        assert!(bias_response.detections.is_empty());
        assert_eq!(bias_response.overall_assessment, invalid_response);
    }

    #[tokio::test]
    async fn test_fallacy_detection_handles_invalid_json() {
        let invalid_response = "Plain text fallacy analysis";
        let fallacy_response = FallacyDetectionResponse::from_completion(invalid_response);

        // Should return empty detections with the raw text as assessment
        assert!(fallacy_response.detections.is_empty());
        assert_eq!(fallacy_response.overall_assessment, invalid_response);
    }

    #[tokio::test]
    async fn test_detection_handles_langbase_error() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": {"message": "Internal server error"}
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        use mcp_langbase_reasoning::langbase::{Message, PipeRequest};
        let messages = vec![Message::user("Test content")];
        let request = PipeRequest::new("detect-biases-v1", messages);
        let response = langbase.call_pipe(request).await;

        assert!(response.is_err(), "Should return error on Langbase failure");
    }
}
