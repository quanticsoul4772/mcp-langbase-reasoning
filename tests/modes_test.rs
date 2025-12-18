//! Integration tests for Phase 2 reasoning modes
//!
//! Tests tree, divergent, and reflection modes using mocked Langbase responses.

use serde_json::json;
use tempfile::tempdir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use mcp_langbase_reasoning::config::{
    Config, DatabaseConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig, RequestConfig,
};
use mcp_langbase_reasoning::langbase::LangbaseClient;
use mcp_langbase_reasoning::modes::{
    DivergentMode, DivergentParams, ReflectionMode, ReflectionParams, TreeMode, TreeParams,
};
use mcp_langbase_reasoning::storage::{Session, SqliteStorage, Storage, Thought};

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

#[cfg(test)]
mod tree_mode_tests {
    use super::*;

    #[tokio::test]
    async fn test_tree_params_builder() {
        let params = TreeParams::new("Test content")
            .with_session("session-123")
            .with_confidence(0.9)
            .with_num_branches(4);

        assert_eq!(params.content, "Test content");
        assert_eq!(params.session_id, Some("session-123".to_string()));
        assert!((params.confidence - 0.9).abs() < 0.001);
        assert_eq!(params.num_branches, 4);
    }

    #[tokio::test]
    async fn test_tree_params_clamps_branches() {
        let params = TreeParams::new("Test").with_num_branches(10);
        assert_eq!(params.num_branches, 4); // Clamped to max 4

        let params = TreeParams::new("Test").with_num_branches(1);
        assert_eq!(params.num_branches, 2); // Clamped to min 2
    }

    #[tokio::test]
    async fn test_tree_params_clamps_confidence() {
        let params = TreeParams::new("Test").with_confidence(1.5);
        assert!((params.confidence - 1.0).abs() < 0.001);

        let params = TreeParams::new("Test").with_confidence(-0.5);
        assert!((params.confidence - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_tree_mode_validates_empty_content() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let params = TreeParams::new("  "); // Empty after trim
        let result = tree_mode.process(params).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Content cannot be empty"));
    }

    #[tokio::test]
    async fn test_tree_mode_creates_session() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        // Mock successful tree response
        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "branches": [
                        {"thought": "Branch 1", "confidence": 0.8, "rationale": "Reason 1"},
                        {"thought": "Branch 2", "confidence": 0.7, "rationale": "Reason 2"}
                    ],
                    "recommended_branch": 0,
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let tree_mode = TreeMode::new(storage.clone(), langbase, &config);

        let params = TreeParams::new("Explore options for solving this problem");
        let result = tree_mode.process(params).await;

        assert!(
            result.is_ok(),
            "Tree processing should succeed: {:?}",
            result.err()
        );
        let result = result.unwrap();

        // Verify session was created
        let session = storage.get_session(&result.session_id).await.unwrap();
        assert!(session.is_some());
        assert_eq!(session.unwrap().mode, "tree");
    }

    #[tokio::test]
    async fn test_tree_mode_creates_branches() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "branches": [
                        {"thought": "Option A", "confidence": 0.85, "rationale": "Most efficient"},
                        {"thought": "Option B", "confidence": 0.75, "rationale": "Most flexible"},
                        {"thought": "Option C", "confidence": 0.65, "rationale": "Most innovative"}
                    ],
                    "recommended_branch": 0,
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let tree_mode = TreeMode::new(storage.clone(), langbase, &config);

        let params = TreeParams::new("Explore options").with_num_branches(3);
        let result = tree_mode.process(params).await.unwrap();

        assert_eq!(result.child_branches.len(), 3);
        assert_eq!(result.recommended_branch_index, 0);
        assert!((result.child_branches[0].confidence - 0.85).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_tree_mode_focus_branch() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "branches": [
                        {"thought": "Branch 1", "confidence": 0.8, "rationale": "Reason 1"},
                        {"thought": "Branch 2", "confidence": 0.7, "rationale": "Reason 2"}
                    ],
                    "recommended_branch": 0,
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let tree_mode = TreeMode::new(storage.clone(), langbase, &config);

        let params = TreeParams::new("Initial exploration");
        let result = tree_mode.process(params).await.unwrap();

        // Focus on second branch
        let branch_to_focus = &result.child_branches[1].id;
        let focus_result = tree_mode
            .focus_branch(&result.session_id, branch_to_focus)
            .await;

        assert!(focus_result.is_ok());

        // Verify session was updated
        let session = storage
            .get_session(&result.session_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.active_branch_id, Some(branch_to_focus.clone()));
    }

    #[tokio::test]
    async fn test_tree_mode_list_branches() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "branches": [
                        {"thought": "Branch 1", "confidence": 0.8, "rationale": "Reason 1"},
                        {"thought": "Branch 2", "confidence": 0.7, "rationale": "Reason 2"}
                    ],
                    "recommended_branch": 0,
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let params = TreeParams::new("Exploration");
        let result = tree_mode.process(params).await.unwrap();

        let branches = tree_mode.list_branches(&result.session_id).await.unwrap();

        // Root branch + 2 child branches
        assert!(branches.len() >= 3);
    }
}

#[cfg(test)]
mod divergent_mode_tests {
    use super::*;

    #[tokio::test]
    async fn test_divergent_params_builder() {
        let params = DivergentParams::new("Creative problem")
            .with_session("session-456")
            .with_num_perspectives(4)
            .with_assumption_challenging()
            .with_rebellion();

        assert_eq!(params.content, "Creative problem");
        assert_eq!(params.session_id, Some("session-456".to_string()));
        assert_eq!(params.num_perspectives, 4);
        assert!(params.challenge_assumptions);
        assert!(params.force_rebellion);
    }

    #[tokio::test]
    async fn test_divergent_params_clamps_perspectives() {
        let params = DivergentParams::new("Test").with_num_perspectives(10);
        assert_eq!(params.num_perspectives, 5); // Clamped to max 5

        let params = DivergentParams::new("Test").with_num_perspectives(1);
        assert_eq!(params.num_perspectives, 2); // Clamped to min 2
    }

    #[tokio::test]
    async fn test_divergent_mode_validates_empty_content() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let divergent_mode = DivergentMode::new(storage, langbase, &config);

        let params = DivergentParams::new("");
        let result = divergent_mode.process(params).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Content cannot be empty"));
    }

    #[tokio::test]
    async fn test_divergent_mode_generates_perspectives() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "perspectives": [
                        {"thought": "Conventional view", "novelty": 0.3, "viability": 0.9},
                        {"thought": "Alternative approach", "novelty": 0.7, "viability": 0.6},
                        {"thought": "Radical idea", "novelty": 0.95, "viability": 0.3}
                    ],
                    "synthesis": "Combining these perspectives reveals new possibilities",
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let divergent_mode = DivergentMode::new(storage.clone(), langbase, &config);

        let params = DivergentParams::new("How might we improve user engagement?");
        let result = divergent_mode.process(params).await;

        assert!(
            result.is_ok(),
            "Divergent processing should succeed: {:?}",
            result.err()
        );
        let result = result.unwrap();

        assert_eq!(result.perspectives.len(), 3);
        assert!(!result.synthesis.is_empty());
        assert_eq!(result.most_novel_perspective, 2); // Index of highest novelty
        assert_eq!(result.most_viable_perspective, 0); // Index of highest viability
    }

    #[tokio::test]
    async fn test_divergent_mode_tracks_novelty() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "perspectives": [
                        {"thought": "P1", "novelty": 0.6, "viability": 0.8},
                        {"thought": "P2", "novelty": 0.8, "viability": 0.6}
                    ],
                    "synthesis": "Combined insight",
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let divergent_mode = DivergentMode::new(storage, langbase, &config);

        let params = DivergentParams::new("Test input");
        let result = divergent_mode.process(params).await.unwrap();

        // Average novelty: (0.6 + 0.8) / 2 = 0.7
        assert!((result.total_novelty_score - 0.7).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_divergent_mode_creates_synthesis_thought() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "perspectives": [
                        {"thought": "P1", "novelty": 0.5, "viability": 0.5}
                    ],
                    "synthesis": "This is the synthesis",
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let divergent_mode = DivergentMode::new(storage.clone(), langbase, &config);

        let params = DivergentParams::new("Test");
        let result = divergent_mode.process(params).await.unwrap();

        // Verify synthesis thought was created
        let synthesis = storage
            .get_thought(&result.synthesis_thought_id)
            .await
            .unwrap();
        assert!(synthesis.is_some());
        assert_eq!(synthesis.unwrap().content, "This is the synthesis");
    }
}

#[cfg(test)]
mod reflection_mode_tests {
    use super::*;

    #[tokio::test]
    async fn test_reflection_params_for_thought() {
        let params = ReflectionParams::for_thought("thought-789")
            .with_session("session-abc")
            .with_max_iterations(5)
            .with_quality_threshold(0.9)
            .with_chain();

        assert_eq!(params.thought_id, Some("thought-789".to_string()));
        assert!(params.content.is_none());
        assert_eq!(params.max_iterations, 5);
        assert!((params.quality_threshold - 0.9).abs() < 0.001);
        assert!(params.include_chain);
    }

    #[tokio::test]
    async fn test_reflection_params_for_content() {
        let params = ReflectionParams::for_content("Content to reflect on");

        assert!(params.thought_id.is_none());
        assert_eq!(params.content, Some("Content to reflect on".to_string()));
    }

    #[tokio::test]
    async fn test_reflection_params_clamps_iterations() {
        let params = ReflectionParams::for_content("Test").with_max_iterations(10);
        assert_eq!(params.max_iterations, 5); // Clamped to max 5

        let params = ReflectionParams::for_content("Test").with_max_iterations(0);
        assert_eq!(params.max_iterations, 1); // Clamped to min 1
    }

    #[tokio::test]
    async fn test_reflection_mode_requires_input() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let reflection_mode = ReflectionMode::new(storage, langbase, &config);

        // Neither thought_id nor content
        let params = ReflectionParams {
            thought_id: None,
            content: None,
            session_id: None,
            branch_id: None,
            max_iterations: 3,
            quality_threshold: 0.8,
            include_chain: false,
        };

        let result = reflection_mode.process(params).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("thought_id or content"));
    }

    #[tokio::test]
    async fn test_reflection_mode_analyzes_content() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "analysis": "The reasoning is sound but could be strengthened",
                    "strengths": ["Clear structure", "Good evidence"],
                    "weaknesses": ["Missing counterarguments"],
                    "recommendations": ["Add alternative perspectives"],
                    "confidence": 0.85,
                    "quality_score": 0.75,
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let reflection_mode = ReflectionMode::new(storage.clone(), langbase, &config);

        let params =
            ReflectionParams::for_content("The market will grow because demand is increasing");
        let result = reflection_mode.process(params).await;

        assert!(
            result.is_ok(),
            "Reflection processing should succeed: {:?}",
            result.err()
        );
        let result = result.unwrap();

        assert!(!result.analysis.is_empty());
        assert!(!result.strengths.is_empty());
        assert!(!result.weaknesses.is_empty());
        assert!(!result.recommendations.is_empty());
    }

    #[tokio::test]
    async fn test_reflection_mode_evaluates_existing_thought() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "analysis": "Analysis of stored thought",
                    "strengths": ["S1"],
                    "weaknesses": ["W1"],
                    "recommendations": ["R1"],
                    "confidence": 0.8,
                    "metadata": {}
                }).to_string()
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        // Create a thought to reflect on
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let thought =
            Thought::new(&session.id, "Original thought content", "linear").with_confidence(0.7);
        storage.create_thought(&thought).await.unwrap();

        let reflection_mode = ReflectionMode::new(storage.clone(), langbase, &config);

        let params = ReflectionParams::for_thought(&thought.id).with_session(&session.id);
        let result = reflection_mode.process(params).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.original_thought_id, Some(thought.id));
    }

    #[tokio::test]
    async fn test_reflection_mode_evaluate_session() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        // Create session with thoughts
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let t1 = Thought::new(&session.id, "First thought", "linear").with_confidence(0.8);
        storage.create_thought(&t1).await.unwrap();

        let t2 = Thought::new(&session.id, "Second thought", "linear")
            .with_confidence(0.9)
            .with_parent(&t1.id);
        storage.create_thought(&t2).await.unwrap();

        let reflection_mode = ReflectionMode::new(storage, langbase, &config);

        let evaluation = reflection_mode.evaluate_session(&session.id).await;

        assert!(evaluation.is_ok());
        let eval = evaluation.unwrap();
        assert_eq!(eval.total_thoughts, 2);
        assert!((eval.average_confidence - 0.85).abs() < 0.001);
        assert!(eval.coherence_score > 0.0);
    }

    #[tokio::test]
    async fn test_reflection_mode_empty_session_error() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        // Create empty session
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let reflection_mode = ReflectionMode::new(storage, langbase, &config);

        let evaluation = reflection_mode.evaluate_session(&session.id).await;

        assert!(evaluation.is_err());
        let err = evaluation.unwrap_err().to_string();
        assert!(err.contains("no thoughts"));
    }
}

#[cfg(test)]
mod cross_ref_tests {
    use super::*;
    use mcp_langbase_reasoning::storage::CrossRefType;

    #[test]
    fn test_cross_ref_type_parsing() {
        assert_eq!(
            "supports".parse::<CrossRefType>().unwrap(),
            CrossRefType::Supports
        );
        assert_eq!(
            "contradicts".parse::<CrossRefType>().unwrap(),
            CrossRefType::Contradicts
        );
        assert_eq!(
            "extends".parse::<CrossRefType>().unwrap(),
            CrossRefType::Extends
        );
        assert_eq!(
            "alternative".parse::<CrossRefType>().unwrap(),
            CrossRefType::Alternative
        );
        assert_eq!(
            "depends".parse::<CrossRefType>().unwrap(),
            CrossRefType::Depends
        );
    }

    #[tokio::test]
    async fn test_tree_mode_with_cross_refs() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/pipes/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "branches": [
                        {"thought": "B1", "confidence": 0.8, "rationale": "R1"}
                    ],
                    "recommended_branch": 0,
                    "metadata": {}
                }).to_string()
            })))
            .expect(2)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri(), db_path.clone());
        let storage = create_test_storage(db_path).await;
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let tree_mode = TreeMode::new(storage.clone(), langbase, &config);

        // First exploration
        let params1 = TreeParams::new("First exploration");
        let result1 = tree_mode.process(params1).await.unwrap();

        // Second exploration with cross-ref to first
        let params2 = TreeParams::new("Second exploration")
            .with_session(&result1.session_id)
            .with_cross_ref(&result1.branch_id, "extends");
        let result2 = tree_mode.process(params2).await.unwrap();

        assert_eq!(result2.cross_refs_created, 1);

        // Verify cross-ref was stored
        let cross_refs = storage
            .get_cross_refs_from(&result2.branch_id)
            .await
            .unwrap();
        assert_eq!(cross_refs.len(), 1);
        assert_eq!(cross_refs[0].to_branch_id, result1.branch_id);
    }
}
