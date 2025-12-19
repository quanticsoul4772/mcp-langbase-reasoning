//! Unit tests for Graph-of-Thoughts (GoT) reasoning mode.
//!
//! Tests configuration, parameter builders, response parsing,
//! result serialization, and edge cases for GoT operations.

use super::*;

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_got_config_default() {
    let config = GotConfig::default();
    assert_eq!(config.max_nodes, 100);
    assert_eq!(config.max_depth, 10);
    assert_eq!(config.default_k, 3);
    assert!((config.prune_threshold - 0.3).abs() < f64::EPSILON);
}

#[test]
fn test_got_config_serialize() {
    let config = GotConfig {
        max_nodes: 50,
        max_depth: 5,
        default_k: 4,
        prune_threshold: 0.4,
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"max_nodes\":50"));
    assert!(json.contains("\"max_depth\":5"));
    assert!(json.contains("\"default_k\":4"));
}

#[test]
fn test_got_config_deserialize() {
    let json = r#"{"max_nodes": 200, "max_depth": 15}"#;
    let config: GotConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.max_nodes, 200);
    assert_eq!(config.max_depth, 15);
    assert_eq!(config.default_k, 3); // default
    assert!((config.prune_threshold - 0.3).abs() < f64::EPSILON); // default
}

// ============================================================================
// Init Params Tests
// ============================================================================

#[test]
fn test_got_init_params_new() {
    let params = GotInitParams::new("Test thought");
    assert_eq!(params.content, "Test thought");
    assert!(params.problem.is_none());
    assert!(params.session_id.is_none());
    assert!(params.config.is_none());
}

#[test]
fn test_got_init_params_with_problem() {
    let params = GotInitParams::new("Test").with_problem("Solve X");
    assert_eq!(params.problem, Some("Solve X".to_string()));
}

#[test]
fn test_got_init_params_with_session() {
    let params = GotInitParams::new("Test").with_session("sess-123");
    assert_eq!(params.session_id, Some("sess-123".to_string()));
}

#[test]
fn test_got_init_params_builder_chain() {
    let params = GotInitParams::new("Content")
        .with_problem("Problem")
        .with_session("sess-1")
        .with_config(GotConfig {
            max_nodes: 50,
            ..Default::default()
        });

    assert_eq!(params.content, "Content");
    assert_eq!(params.problem, Some("Problem".to_string()));
    assert_eq!(params.session_id, Some("sess-1".to_string()));
    assert!(params.config.is_some());
    assert_eq!(params.config.unwrap().max_nodes, 50);
}

#[test]
fn test_got_init_params_deserialize() {
    let json = r#"{"content": "Test content", "problem": "Test problem"}"#;
    let params: GotInitParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.content, "Test content");
    assert_eq!(params.problem, Some("Test problem".to_string()));
}

// ============================================================================
// Generate Params Tests
// ============================================================================

#[test]
fn test_got_generate_params_new() {
    let params = GotGenerateParams::new("sess-123");
    assert_eq!(params.session_id, "sess-123");
    assert!(params.node_id.is_none());
    assert_eq!(params.k, 3);
}

#[test]
fn test_got_generate_params_with_node() {
    let params = GotGenerateParams::new("sess-123").with_node("node-1");
    assert_eq!(params.node_id, Some("node-1".to_string()));
}

#[test]
fn test_got_generate_params_with_k() {
    let params = GotGenerateParams::new("sess-123").with_k(5);
    assert_eq!(params.k, 5);
}

#[test]
fn test_got_generate_params_builder_chain() {
    let params = GotGenerateParams::new("sess-123")
        .with_node("node-1")
        .with_k(5)
        .with_problem("Find solutions");

    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, Some("node-1".to_string()));
    assert_eq!(params.k, 5);
    assert_eq!(params.problem, Some("Find solutions".to_string()));
}

#[test]
fn test_got_generate_params_deserialize() {
    let json = r#"{"session_id": "sess-123", "node_id": "node-1", "k": 5}"#;
    let params: GotGenerateParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, Some("node-1".to_string()));
    assert_eq!(params.k, 5);
}

// ============================================================================
// Score Params Tests
// ============================================================================

#[test]
fn test_got_score_params_new() {
    let params = GotScoreParams::new("sess-123", "node-1");
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, "node-1");
    assert!(params.problem.is_none());
}

#[test]
fn test_got_score_params_builder() {
    let params = GotScoreParams::new("sess-123", "node-1").with_problem("Evaluate quality");

    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, "node-1");
    assert_eq!(params.problem, Some("Evaluate quality".to_string()));
}

#[test]
fn test_got_score_params_deserialize() {
    let json = r#"{"session_id": "sess-123", "node_id": "node-1"}"#;
    let params: GotScoreParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, "node-1");
}

// ============================================================================
// Aggregate Params Tests
// ============================================================================

#[test]
fn test_got_aggregate_params_new() {
    let params =
        GotAggregateParams::new("sess-123", vec!["node-1".to_string(), "node-2".to_string()]);
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_ids.len(), 2);
}

#[test]
fn test_got_aggregate_params_builder() {
    let params = GotAggregateParams::new(
        "sess-123",
        vec!["n1".to_string(), "n2".to_string(), "n3".to_string()],
    )
    .with_problem("Synthesize ideas");

    assert_eq!(params.node_ids.len(), 3);
    assert_eq!(params.problem, Some("Synthesize ideas".to_string()));
}

// ============================================================================
// Refine Params Tests
// ============================================================================

#[test]
fn test_got_refine_params_new() {
    let params = GotRefineParams::new("sess-123", "node-1");
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, "node-1");
}

#[test]
fn test_got_refine_params_builder() {
    let params = GotRefineParams::new("sess-123", "node-1").with_problem("Improve clarity");

    assert_eq!(params.problem, Some("Improve clarity".to_string()));
}

// ============================================================================
// Prune Params Tests
// ============================================================================

#[test]
fn test_got_prune_params_new() {
    let params = GotPruneParams::new("sess-123");
    assert_eq!(params.session_id, "sess-123");
    assert!(params.threshold.is_none());
}

#[test]
fn test_got_prune_params_with_threshold() {
    let params = GotPruneParams::new("sess-123").with_threshold(0.5);
    assert_eq!(params.threshold, Some(0.5));
}

#[test]
fn test_got_prune_params_deserialize() {
    let json = r#"{"session_id": "sess-123", "threshold": 0.5}"#;
    let params: GotPruneParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.threshold, Some(0.5));
}

// ============================================================================
// Finalize Params Tests
// ============================================================================

#[test]
fn test_got_finalize_params_new() {
    let params = GotFinalizeParams::new("sess-123");
    assert_eq!(params.session_id, "sess-123");
    assert!(params.terminal_node_ids.is_empty());
}

#[test]
fn test_got_finalize_params_with_terminal_nodes() {
    let params = GotFinalizeParams::new("sess-123")
        .with_terminal_nodes(vec!["node-1".to_string(), "node-2".to_string()]);
    assert_eq!(params.terminal_node_ids.len(), 2);
}

#[test]
fn test_got_finalize_params_deserialize() {
    let json = r#"{"session_id": "sess-123", "terminal_node_ids": ["n1", "n2"]}"#;
    let params: GotFinalizeParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.terminal_node_ids.len(), 2);
}

// ============================================================================
// Get State Params Tests
// ============================================================================

#[test]
fn test_got_get_state_params_new() {
    let params = GotGetStateParams::new("sess-123");
    assert_eq!(params.session_id, "sess-123");
}

// ============================================================================
// Response Parsing Tests - Generate
// ============================================================================

#[test]
fn test_generate_response_from_json() {
    let json = r#"{"continuations": [{"thought": "Idea 1", "confidence": 0.9, "novelty": 0.8, "rationale": "Test"}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations.len(), 1);
    assert_eq!(resp.continuations[0].thought, "Idea 1");
    assert_eq!(resp.continuations[0].confidence, 0.9);
}

#[test]
fn test_generate_response_from_plain_text_returns_error() {
    let text = "Plain text response";
    // Non-JSON input returns error
    let result = GenerateResponse::from_completion(text);
    assert!(result.is_err());
}

#[test]
fn test_generate_response_multiple_continuations() {
    let json = r#"{
        "continuations": [
            {"thought": "Idea 1", "confidence": 0.9, "novelty": 0.8, "rationale": "First approach"},
            {"thought": "Idea 2", "confidence": 0.7, "novelty": 0.9, "rationale": "Second approach"},
            {"thought": "Idea 3", "confidence": 0.8, "novelty": 0.6, "rationale": "Third approach"}
        ]
    }"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations.len(), 3);
    assert_eq!(resp.continuations[1].thought, "Idea 2");
    assert_eq!(resp.continuations[2].novelty, 0.6);
}

#[test]
fn test_generate_response_with_metadata() {
    let json = r#"{
        "continuations": [{"thought": "Test", "confidence": 0.8, "novelty": 0.5, "rationale": "Reason"}],
        "metadata": {"source": "test", "version": 1}
    }"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert!(resp.metadata.is_some());
    assert_eq!(resp.continuations.len(), 1);
}

#[test]
fn test_generate_response_with_defaults() {
    let json = r#"{"continuations": [{"thought": "Minimal"}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations[0].confidence, 0.7); // default
    assert_eq!(resp.continuations[0].novelty, 0.0); // default
}

#[test]
fn test_generate_response_empty_continuations() {
    let json = r#"{"continuations": []}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert!(resp.continuations.is_empty());
}

// ============================================================================
// Response Parsing Tests - Score
// ============================================================================

#[test]
fn test_score_response_from_json() {
    let json = r#"{"overall_score": 0.85, "breakdown": {"relevance": 0.9, "validity": 0.8, "depth": 0.7, "novelty": 0.6}, "is_terminal_candidate": true, "rationale": "Good"}"#;
    let resp = ScoreResponse::from_completion(json).unwrap();
    assert_eq!(resp.overall_score, 0.85);
    assert!(resp.is_terminal_candidate);
    assert_eq!(resp.breakdown.relevance, 0.9);
}

#[test]
fn test_score_response_from_plain_text_returns_error() {
    let text = "Invalid";
    // Non-JSON input returns error
    let result = ScoreResponse::from_completion(text);
    assert!(result.is_err());
}

#[test]
fn test_score_response_partial_breakdown() {
    let json = r#"{
        "overall_score": 0.75,
        "breakdown": {"relevance": 0.8},
        "is_terminal_candidate": false,
        "rationale": "Partial"
    }"#;
    let resp = ScoreResponse::from_completion(json).unwrap();
    assert_eq!(resp.overall_score, 0.75);
    assert_eq!(resp.breakdown.relevance, 0.8);
    assert_eq!(resp.breakdown.validity, 0.5); // default from default_score()
}

// ============================================================================
// Response Parsing Tests - Aggregate
// ============================================================================

#[test]
fn test_aggregate_response_from_json() {
    let json = r#"{"aggregated_thought": "Combined insight", "confidence": 0.88, "synthesis_approach": "Merge"}"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert_eq!(resp.aggregated_thought, "Combined insight");
    assert_eq!(resp.confidence, 0.88);
}

#[test]
fn test_aggregate_response_plain_text_returns_error() {
    let text = "Non-JSON aggregate response";
    // Non-JSON input returns error
    let result = AggregateResponse::from_completion(text);
    assert!(result.is_err());
}

// ============================================================================
// Response Parsing Tests - Refine
// ============================================================================

#[test]
fn test_refine_response_from_json() {
    let json = r#"{"refined_thought": "Improved", "confidence": 0.9, "improvements_made": ["Clarity"], "quality_delta": 0.15}"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.refined_thought, "Improved");
    assert_eq!(resp.quality_delta, 0.15);
    assert_eq!(resp.improvements_made.len(), 1);
}

#[test]
fn test_refine_response_plain_text_returns_error() {
    let text = "Improved thought content";
    // Non-JSON input returns error
    let result = RefineResponse::from_completion(text);
    assert!(result.is_err());
}

#[test]
fn test_refine_response_with_all_fields() {
    let json = r#"{
        "refined_thought": "Better version",
        "confidence": 0.95,
        "improvements_made": ["Clarity", "Structure", "Evidence"],
        "quality_delta": 0.25
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.refined_thought, "Better version");
    assert_eq!(resp.confidence, 0.95);
    assert_eq!(resp.improvements_made.len(), 3);
    assert_eq!(resp.quality_delta, 0.25);
}

// ============================================================================
// Result Serialization Tests
// ============================================================================

#[test]
fn test_got_init_result_serialize() {
    let result = GotInitResult {
        session_id: "sess-abc".to_string(),
        root_node_id: "node-root".to_string(),
        content: "Initial thought".to_string(),
        config: GotConfig::default(),
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("sess-abc"));
    assert!(json.contains("node-root"));
    assert!(json.contains("Initial thought"));
}

#[test]
fn test_got_generate_result_serialize() {
    let result = GotGenerateResult {
        session_id: "sess-123".to_string(),
        source_node_id: "node-1".to_string(),
        continuations: vec![GeneratedContinuation {
            node_id: "node-2".to_string(),
            content: "Continuation 1".to_string(),
            confidence: 0.85,
            novelty: 0.7,
            rationale: "Reason 1".to_string(),
        }],
        count: 1,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"count\":1"));
    assert!(json.contains("Continuation 1"));
}

#[test]
fn test_got_score_result_serialize() {
    let result = GotScoreResult {
        session_id: "sess-123".to_string(),
        node_id: "node-1".to_string(),
        overall_score: 0.82,
        breakdown: ScoreBreakdown {
            relevance: 0.9,
            validity: 0.8,
            depth: 0.7,
            novelty: 0.8,
        },
        is_terminal_candidate: true,
        rationale: "High quality".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("0.82"));
    assert!(json.contains("is_terminal_candidate"));
}

#[test]
fn test_got_aggregate_result_serialize() {
    let result = GotAggregateResult {
        session_id: "sess-123".to_string(),
        aggregated_node_id: "n-agg".to_string(),
        content: "Synthesized insight".to_string(),
        confidence: 0.88,
        source_nodes: vec!["n1".to_string(), "n2".to_string()],
        synthesis_approach: "Consensus".to_string(),
        conflicts_resolved: vec!["Disagreement on priority".to_string()],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("Synthesized insight"));
    assert!(json.contains("Consensus"));
    assert!(json.contains("conflicts_resolved"));
}

#[test]
fn test_got_refine_result_serialize() {
    let result = GotRefineResult {
        session_id: "sess-123".to_string(),
        original_node_id: "n1".to_string(),
        refined_node_id: "n1-refined".to_string(),
        content: "Refined content with improvements".to_string(),
        confidence: 0.9,
        improvements_made: vec!["Clarity".to_string(), "Depth".to_string()],
        quality_delta: 0.15,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("quality_delta"));
    assert!(json.contains("Refined content"));
    assert!(json.contains("improvements_made"));
}

#[test]
fn test_got_prune_result_serialize() {
    let result = GotPruneResult {
        session_id: "sess-123".to_string(),
        pruned_count: 5,
        remaining_count: 15,
        threshold_used: 0.3,
        pruned_node_ids: vec!["p1".to_string(), "p2".to_string()],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"pruned_count\":5"));
    assert!(json.contains("\"remaining_count\":15"));
    assert!(json.contains("threshold_used"));
}

#[test]
fn test_got_finalize_result_serialize() {
    let result = GotFinalizeResult {
        session_id: "sess-123".to_string(),
        terminal_count: 2,
        conclusions: vec![TerminalConclusion {
            node_id: "t1".to_string(),
            content: "First conclusion".to_string(),
            score: Some(0.9),
            depth: 5,
        }],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("First conclusion"));
    assert!(json.contains("terminal_count"));
}

#[test]
fn test_got_state_result_serialize() {
    let result = GotStateResult {
        session_id: "sess-123".to_string(),
        total_nodes: 10,
        active_nodes: 3,
        terminal_nodes: 2,
        total_edges: 12,
        max_depth: 4,
        root_node_ids: vec!["root-1".to_string()],
        active_node_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        terminal_node_ids: vec!["t1".to_string(), "t2".to_string()],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"total_nodes\":10"));
    assert!(json.contains("\"max_depth\":4"));
}

// ============================================================================
// Component Serialization Tests
// ============================================================================

#[test]
fn test_score_breakdown_serialize() {
    let breakdown = ScoreBreakdown {
        relevance: 0.8,
        validity: 0.7,
        depth: 0.6,
        novelty: 0.5,
    };
    let json = serde_json::to_string(&breakdown).unwrap();
    assert!(json.contains("relevance"));
    assert!(json.contains("0.8"));
}

#[test]
fn test_terminal_conclusion_serialize() {
    let conclusion = TerminalConclusion {
        node_id: "node-1".to_string(),
        content: "Final insight".to_string(),
        score: Some(0.95),
        depth: 7,
    };
    let json = serde_json::to_string(&conclusion).unwrap();
    assert!(json.contains("Final insight"));
    assert!(json.contains("0.95"));
}

#[test]
fn test_terminal_conclusion_no_score() {
    let conclusion = TerminalConclusion {
        node_id: "node-1".to_string(),
        content: "No score".to_string(),
        score: None,
        depth: 3,
    };
    let json = serde_json::to_string(&conclusion).unwrap();
    assert!(json.contains("No score"));
}

#[test]
fn test_generated_continuation_serialize() {
    let cont = GeneratedContinuation {
        node_id: "node-1".to_string(),
        content: "New idea".to_string(),
        confidence: 0.8,
        novelty: 0.9,
        rationale: "Because".to_string(),
    };
    let json = serde_json::to_string(&cont).unwrap();
    assert!(json.contains("New idea"));
    assert!(json.contains("rationale"));
}

// ============================================================================
// Helper Function Tests
// ============================================================================

#[test]
fn test_default_k() {
    assert_eq!(default_k(), 3);
}

#[test]
fn test_default_max_nodes() {
    assert_eq!(default_max_nodes(), 100);
}

#[test]
fn test_default_max_depth() {
    assert_eq!(default_max_depth(), 10);
}

#[test]
fn test_default_prune_threshold() {
    assert!((default_prune_threshold() - 0.3).abs() < f64::EPSILON);
}

#[test]
fn test_default_confidence() {
    assert!((default_confidence() - 0.7).abs() < f64::EPSILON);
}

#[test]
fn test_default_score() {
    assert!((default_score() - 0.5).abs() < f64::EPSILON);
}

// ============================================================================
// Edge Cases - Generate Params
// ============================================================================

#[test]
fn test_generate_params_k_zero() {
    let params = GotGenerateParams::new("sess-123").with_k(0);
    assert_eq!(params.k, 0);
}

#[test]
fn test_generate_params_k_large() {
    let params = GotGenerateParams::new("sess-123").with_k(100);
    assert_eq!(params.k, 100);
}

#[test]
fn test_generate_params_empty_session_id() {
    let params = GotGenerateParams::new("");
    assert_eq!(params.session_id, "");
}

#[test]
fn test_generate_params_deserialize_with_defaults() {
    let json = r#"{"session_id": "sess-123"}"#;
    let params: GotGenerateParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
    assert!(params.node_id.is_none());
    assert_eq!(params.k, 3); // default
    assert!(params.problem.is_none());
}

#[test]
fn test_generate_params_serialize() {
    let params = GotGenerateParams::new("sess-123")
        .with_node("node-1")
        .with_k(5)
        .with_problem("Test problem");
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"node_id\":\"node-1\""));
    assert!(json.contains("\"k\":5"));
    assert!(json.contains("Test problem"));
}

// ============================================================================
// Edge Cases - Score Params
// ============================================================================

#[test]
fn test_score_params_serialize() {
    let params = GotScoreParams::new("sess-123", "node-1").with_problem("Quality check");
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"node_id\":\"node-1\""));
    assert!(json.contains("Quality check"));
}

#[test]
fn test_score_params_empty_strings() {
    let params = GotScoreParams::new("", "");
    assert_eq!(params.session_id, "");
    assert_eq!(params.node_id, "");
}

// ============================================================================
// Edge Cases - Aggregate Params
// ============================================================================

#[test]
fn test_aggregate_params_empty_nodes() {
    let params = GotAggregateParams::new("sess-123", vec![]);
    assert_eq!(params.node_ids.len(), 0);
}

#[test]
fn test_aggregate_params_single_node() {
    let params = GotAggregateParams::new("sess-123", vec!["node-1".to_string()]);
    assert_eq!(params.node_ids.len(), 1);
}

#[test]
fn test_aggregate_params_serialize() {
    let params = GotAggregateParams::new("sess-123", vec!["n1".to_string(), "n2".to_string()])
        .with_problem("Combine insights");
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"node_ids\""));
    assert!(json.contains("Combine insights"));
}

#[test]
fn test_aggregate_params_deserialize() {
    let json = r#"{"session_id": "sess-123", "node_ids": ["n1", "n2", "n3"]}"#;
    let params: GotAggregateParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_ids.len(), 3);
    assert!(params.problem.is_none());
}

// ============================================================================
// Edge Cases - Refine Params
// ============================================================================

#[test]
fn test_refine_params_serialize() {
    let params = GotRefineParams::new("sess-123", "node-1").with_problem("Enhance clarity");
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"node_id\":\"node-1\""));
    assert!(json.contains("Enhance clarity"));
}

#[test]
fn test_refine_params_deserialize() {
    let json = r#"{"session_id": "sess-123", "node_id": "node-1", "problem": "Improve"}"#;
    let params: GotRefineParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
    assert_eq!(params.node_id, "node-1");
    assert_eq!(params.problem, Some("Improve".to_string()));
}

// ============================================================================
// Edge Cases - Prune Params
// ============================================================================

#[test]
fn test_prune_params_threshold_zero() {
    let params = GotPruneParams::new("sess-123").with_threshold(0.0);
    assert_eq!(params.threshold, Some(0.0));
}

#[test]
fn test_prune_params_threshold_one() {
    let params = GotPruneParams::new("sess-123").with_threshold(1.0);
    assert_eq!(params.threshold, Some(1.0));
}

#[test]
fn test_prune_params_serialize() {
    let params = GotPruneParams::new("sess-123").with_threshold(0.5);
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"threshold\":0.5"));
}

// ============================================================================
// Edge Cases - Finalize Params
// ============================================================================

#[test]
fn test_finalize_params_serialize() {
    let params = GotFinalizeParams::new("sess-123")
        .with_terminal_nodes(vec!["t1".to_string(), "t2".to_string()]);
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"terminal_node_ids\""));
}

#[test]
fn test_finalize_params_empty_terminal_nodes_deserialize() {
    let json = r#"{"session_id": "sess-123"}"#;
    let params: GotFinalizeParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
    assert!(params.terminal_node_ids.is_empty());
}

// ============================================================================
// Edge Cases - Get State Params
// ============================================================================

#[test]
fn test_get_state_params_serialize() {
    let params = GotGetStateParams::new("sess-123");
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
}

#[test]
fn test_get_state_params_deserialize() {
    let json = r#"{"session_id": "sess-123"}"#;
    let params: GotGetStateParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.session_id, "sess-123");
}

// ============================================================================
// Edge Cases - Init Params
// ============================================================================

#[test]
fn test_init_params_serialize() {
    let params = GotInitParams::new("Test content")
        .with_problem("Problem")
        .with_session("sess-123");
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"content\":\"Test content\""));
    assert!(json.contains("\"problem\":\"Problem\""));
    assert!(json.contains("\"session_id\":\"sess-123\""));
}

#[test]
fn test_init_params_empty_content() {
    let params = GotInitParams::new("");
    assert_eq!(params.content, "");
}

#[test]
fn test_init_params_skip_none_fields() {
    let params = GotInitParams::new("Test");
    let json = serde_json::to_string(&params).unwrap();
    // Optional fields should be skipped when None
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("problem").is_none());
    assert!(parsed.get("session_id").is_none());
    assert!(parsed.get("config").is_none());
}

#[test]
fn test_init_params_deserialize_minimal() {
    let json = r#"{"content": "Test"}"#;
    let params: GotInitParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.content, "Test");
    assert!(params.problem.is_none());
    assert!(params.session_id.is_none());
    assert!(params.config.is_none());
}

// ============================================================================
// Edge Cases - Generate Response Parsing
// ============================================================================

#[test]
fn test_generate_response_malformed_json_returns_error() {
    let json = r#"{"continuations": [{"thought": "incomplete""#;
    // Malformed JSON returns error
    let result = GenerateResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_generate_response_missing_thought_field_returns_error() {
    let json = r#"{"continuations": [{"confidence": 0.8}]}"#;
    // Missing required field returns error
    let result = GenerateResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_generate_response_empty_string_returns_error() {
    // Empty string returns error
    let result = GenerateResponse::from_completion("");
    assert!(result.is_err());
}

#[test]
fn test_generate_response_long_text_returns_error() {
    let long_text = "a".repeat(300);
    // Non-JSON text returns error
    let result = GenerateResponse::from_completion(&long_text);
    assert!(result.is_err());
}

// ============================================================================
// Edge Cases - Score Response Parsing
// ============================================================================

#[test]
fn test_score_response_malformed_json_returns_error() {
    let json = r#"{"overall_score": 0.8"#;
    // Malformed JSON returns error
    let result = ScoreResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_score_response_missing_breakdown_returns_error() {
    // Missing breakdown field returns error
    let json = r#"{"overall_score": 0.75}"#;
    let result = ScoreResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_score_response_empty_string_returns_error() {
    // Empty string returns error
    let result = ScoreResponse::from_completion("");
    assert!(result.is_err());
}

#[test]
fn test_score_response_with_metadata() {
    let json = r#"{
        "overall_score": 0.85,
        "breakdown": {"relevance": 0.9, "validity": 0.8, "depth": 0.7, "novelty": 0.6},
        "is_terminal_candidate": true,
        "rationale": "Good",
        "metadata": {"timestamp": "2024-01-01"}
    }"#;
    let resp = ScoreResponse::from_completion(json).unwrap();
    assert!(resp.metadata.is_some());
}

#[test]
fn test_score_breakdown_defaults() {
    let json = r#"{"overall_score": 0.8, "breakdown": {}}"#;
    let resp = ScoreResponse::from_completion(json).unwrap();
    // All breakdown fields should use defaults
    assert_eq!(resp.breakdown.relevance, 0.5);
    assert_eq!(resp.breakdown.validity, 0.5);
    assert_eq!(resp.breakdown.depth, 0.5);
    assert_eq!(resp.breakdown.novelty, 0.5);
}

// ============================================================================
// Edge Cases - Aggregate Response Parsing
// ============================================================================

#[test]
fn test_aggregate_response_malformed_json_returns_error() {
    let json = r#"{"aggregated_thought": "test"#;
    // Malformed JSON returns error
    let result = AggregateResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_aggregate_response_empty_string_returns_error() {
    // Empty string returns error
    let result = AggregateResponse::from_completion("");
    assert!(result.is_err());
}

#[test]
fn test_aggregate_response_with_all_fields() {
    let json = r#"{
        "aggregated_thought": "Combined",
        "confidence": 0.9,
        "sources_used": ["s1", "s2"],
        "synthesis_approach": "Merge",
        "conflicts_resolved": ["c1"],
        "metadata": {"test": true}
    }"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert_eq!(resp.aggregated_thought, "Combined");
    assert_eq!(resp.confidence, 0.9);
    assert_eq!(resp.sources_used.len(), 2);
    assert_eq!(resp.synthesis_approach, "Merge");
    assert_eq!(resp.conflicts_resolved.len(), 1);
    assert!(resp.metadata.is_some());
}

#[test]
fn test_aggregate_response_empty_arrays() {
    let json = r#"{
        "aggregated_thought": "Test",
        "sources_used": [],
        "conflicts_resolved": []
    }"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert!(resp.sources_used.is_empty());
    assert!(resp.conflicts_resolved.is_empty());
}

// ============================================================================
// Edge Cases - Refine Response Parsing
// ============================================================================

#[test]
fn test_refine_response_malformed_json_returns_error() {
    let json = r#"{"refined_thought": "test"#;
    // Malformed JSON returns error
    let result = RefineResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_refine_response_empty_string_returns_error() {
    // Empty string returns error
    let result = RefineResponse::from_completion("");
    assert!(result.is_err());
}

#[test]
fn test_refine_response_negative_quality_delta() {
    let json = r#"{
        "refined_thought": "Worse",
        "quality_delta": -0.2
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.quality_delta, -0.2);
}

#[test]
fn test_refine_response_empty_improvements() {
    let json = r#"{
        "refined_thought": "Test",
        "improvements_made": []
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert!(resp.improvements_made.is_empty());
}

#[test]
fn test_refine_response_many_improvements() {
    let json = r#"{
        "refined_thought": "Better",
        "improvements_made": ["Clarity", "Depth", "Structure", "Evidence", "Logic"]
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.improvements_made.len(), 5);
}

#[test]
fn test_refine_response_with_aspects_unchanged() {
    let json = r#"{
        "refined_thought": "Refined",
        "improvements_made": ["Clarity"],
        "aspects_unchanged": ["Core argument", "Evidence"]
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.improvements_made.len(), 1);
    assert_eq!(resp.aspects_unchanged.len(), 2);
}

// ============================================================================
// Edge Cases - Result Serialization with Boundary Values
// ============================================================================

#[test]
fn test_score_breakdown_boundary_values() {
    let breakdown = ScoreBreakdown {
        relevance: 0.0,
        validity: 1.0,
        depth: 0.5,
        novelty: 0.0,
    };
    let json = serde_json::to_string(&breakdown).unwrap();
    assert!(json.contains("\"relevance\":0.0"));
    assert!(json.contains("\"validity\":1.0"));
}

#[test]
fn test_terminal_conclusion_zero_depth() {
    let conclusion = TerminalConclusion {
        node_id: "node-1".to_string(),
        content: "Root conclusion".to_string(),
        score: Some(0.95),
        depth: 0,
    };
    let json = serde_json::to_string(&conclusion).unwrap();
    assert!(json.contains("\"depth\":0"));
}

#[test]
fn test_terminal_conclusion_negative_depth() {
    let conclusion = TerminalConclusion {
        node_id: "node-1".to_string(),
        content: "Test".to_string(),
        score: Some(0.5),
        depth: -1,
    };
    let json = serde_json::to_string(&conclusion).unwrap();
    assert!(json.contains("\"depth\":-1"));
}

#[test]
fn test_generated_continuation_zero_confidence() {
    let cont = GeneratedContinuation {
        node_id: "node-1".to_string(),
        content: "Low confidence".to_string(),
        confidence: 0.0,
        novelty: 0.0,
        rationale: "Uncertain".to_string(),
    };
    let json = serde_json::to_string(&cont).unwrap();
    assert!(json.contains("\"confidence\":0.0"));
    assert!(json.contains("\"novelty\":0.0"));
}

#[test]
fn test_generated_continuation_max_confidence() {
    let cont = GeneratedContinuation {
        node_id: "node-1".to_string(),
        content: "High confidence".to_string(),
        confidence: 1.0,
        novelty: 1.0,
        rationale: "Certain".to_string(),
    };
    let json = serde_json::to_string(&cont).unwrap();
    assert!(json.contains("\"confidence\":1.0"));
    assert!(json.contains("\"novelty\":1.0"));
}

// ============================================================================
// Edge Cases - State Result with Empty Graph
// ============================================================================

#[test]
fn test_state_result_empty_graph() {
    let result = GotStateResult {
        session_id: "sess-123".to_string(),
        total_nodes: 0,
        active_nodes: 0,
        terminal_nodes: 0,
        total_edges: 0,
        max_depth: 0,
        root_node_ids: vec![],
        active_node_ids: vec![],
        terminal_node_ids: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"total_nodes\":0"));
    assert!(json.contains("\"max_depth\":0"));
}

#[test]
fn test_state_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "total_nodes": 10,
        "active_nodes": 3,
        "terminal_nodes": 2,
        "total_edges": 12,
        "max_depth": 4,
        "root_node_ids": ["r1"],
        "active_node_ids": ["a1", "a2", "a3"],
        "terminal_node_ids": ["t1", "t2"]
    }"#;
    let result: GotStateResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.total_nodes, 10);
    assert_eq!(result.active_nodes, 3);
    assert_eq!(result.terminal_nodes, 2);
    assert_eq!(result.max_depth, 4);
}

// ============================================================================
// Edge Cases - Prune Result
// ============================================================================

#[test]
fn test_prune_result_no_pruning() {
    let result = GotPruneResult {
        session_id: "sess-123".to_string(),
        pruned_count: 0,
        remaining_count: 10,
        threshold_used: 0.3,
        pruned_node_ids: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"pruned_count\":0"));
    assert!(json.contains("\"remaining_count\":10"));
}

#[test]
fn test_prune_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "pruned_count": 5,
        "remaining_count": 15,
        "threshold_used": 0.3,
        "pruned_node_ids": ["p1", "p2", "p3"]
    }"#;
    let result: GotPruneResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.pruned_count, 5);
    assert_eq!(result.remaining_count, 15);
    assert_eq!(result.pruned_node_ids.len(), 3);
}

// ============================================================================
// Edge Cases - Finalize Result
// ============================================================================

#[test]
fn test_finalize_result_no_conclusions() {
    let result = GotFinalizeResult {
        session_id: "sess-123".to_string(),
        terminal_count: 0,
        conclusions: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"terminal_count\":0"));
}

#[test]
fn test_finalize_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "terminal_count": 2,
        "conclusions": [
            {"node_id": "t1", "content": "C1", "score": 0.9, "depth": 5},
            {"node_id": "t2", "content": "C2", "score": null, "depth": 3}
        ]
    }"#;
    let result: GotFinalizeResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.terminal_count, 2);
    assert_eq!(result.conclusions.len(), 2);
    assert_eq!(result.conclusions[0].score, Some(0.9));
    assert_eq!(result.conclusions[1].score, None);
}

// ============================================================================
// Edge Cases - Generate Result
// ============================================================================

#[test]
fn test_generate_result_empty_continuations() {
    let result = GotGenerateResult {
        session_id: "sess-123".to_string(),
        source_node_id: "node-1".to_string(),
        continuations: vec![],
        count: 0,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"count\":0"));
    assert!(json.contains("\"continuations\":[]"));
}

#[test]
fn test_generate_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "source_node_id": "node-1",
        "continuations": [
            {"node_id": "n2", "content": "C1", "confidence": 0.8, "novelty": 0.7, "rationale": "R1"}
        ],
        "count": 1
    }"#;
    let result: GotGenerateResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.session_id, "sess-123");
    assert_eq!(result.continuations.len(), 1);
    assert_eq!(result.count, 1);
}

// ============================================================================
// Edge Cases - Config
// ============================================================================

#[test]
fn test_config_deserialize_empty_object() {
    let json = r#"{}"#;
    let config: GotConfig = serde_json::from_str(json).unwrap();
    // Should use all defaults
    assert_eq!(config.max_nodes, 100);
    assert_eq!(config.max_depth, 10);
    assert_eq!(config.default_k, 3);
    assert!((config.prune_threshold - 0.3).abs() < f64::EPSILON);
}

#[test]
fn test_config_boundary_values() {
    let config = GotConfig {
        max_nodes: 1,
        max_depth: 1,
        default_k: 1,
        prune_threshold: 0.0,
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"max_nodes\":1"));
    assert!(json.contains("\"max_depth\":1"));
    assert!(json.contains("\"default_k\":1"));
    assert!(json.contains("\"prune_threshold\":0.0"));
}

#[test]
fn test_config_large_values() {
    let config = GotConfig {
        max_nodes: 10000,
        max_depth: 1000,
        default_k: 100,
        prune_threshold: 1.0,
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"max_nodes\":10000"));
    assert!(json.contains("\"max_depth\":1000"));
    assert!(json.contains("\"default_k\":100"));
    assert!(json.contains("\"prune_threshold\":1"));
}

// ============================================================================
// Unicode and Special Characters Tests
// ============================================================================

#[test]
fn test_init_params_unicode_content() {
    let params = GotInitParams::new("测试内容 🚀 émojis");
    assert_eq!(params.content, "测试内容 🚀 émojis");
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotInitParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content, "测试内容 🚀 émojis");
}

#[test]
fn test_generate_response_unicode_thought() {
    let json = r#"{"continuations": [{"thought": "日本語のテキスト", "confidence": 0.9}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations[0].thought, "日本語のテキスト");
}

#[test]
fn test_score_params_unicode_problem() {
    let params = GotScoreParams::new("sess-123", "node-1").with_problem("Problème français 🇫🇷");
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotScoreParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.problem, Some("Problème français 🇫🇷".to_string()));
}

#[test]
fn test_aggregate_response_unicode_synthesis() {
    let json = r#"{"aggregated_thought": "综合分析结果", "synthesis_approach": "合并方法"}"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert_eq!(resp.aggregated_thought, "综合分析结果");
    assert_eq!(resp.synthesis_approach, "合并方法");
}

#[test]
fn test_refine_response_unicode_improvements() {
    let json = r#"{
        "refined_thought": "Verbesserte Gedanken",
        "improvements_made": ["Klarheit verbessert", "Tiefe hinzugefügt"]
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.refined_thought, "Verbesserte Gedanken");
    assert_eq!(resp.improvements_made.len(), 2);
}

#[test]
fn test_terminal_conclusion_unicode_content() {
    let conclusion = TerminalConclusion {
        node_id: "node-1".to_string(),
        content: "Conclusión final 🎯".to_string(),
        score: Some(0.95),
        depth: 3,
    };
    let json = serde_json::to_string(&conclusion).unwrap();
    let parsed: TerminalConclusion = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content, "Conclusión final 🎯");
}

#[test]
fn test_generate_params_unicode_session_id() {
    let params = GotGenerateParams::new("会话-123");
    assert_eq!(params.session_id, "会话-123");
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotGenerateParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, "会话-123");
}

#[test]
fn test_aggregate_params_unicode_node_ids() {
    let params =
        GotAggregateParams::new("sess-123", vec!["节点-1".to_string(), "节点-2".to_string()]);
    assert_eq!(params.node_ids[0], "节点-1");
    assert_eq!(params.node_ids[1], "节点-2");
}

// ============================================================================
// Response Parsing - Extreme Values
// ============================================================================

#[test]
fn test_generate_response_very_high_confidence() {
    let json = r#"{"continuations": [{"thought": "Test", "confidence": 0.99999999}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert!((resp.continuations[0].confidence - 0.99999999).abs() < 1e-6);
}

#[test]
fn test_generate_response_very_low_confidence() {
    let json = r#"{"continuations": [{"thought": "Test", "confidence": 0.00000001}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert!((resp.continuations[0].confidence - 0.00000001).abs() < 1e-6);
}

#[test]
fn test_score_response_extreme_scores() {
    let json = r#"{
        "overall_score": 0.999,
        "breakdown": {
            "relevance": 0.001,
            "validity": 0.999,
            "depth": 0.5,
            "novelty": 0.0
        }
    }"#;
    let resp = ScoreResponse::from_completion(json).unwrap();
    assert!((resp.overall_score - 0.999).abs() < 1e-6);
    assert!((resp.breakdown.relevance - 0.001).abs() < 1e-6);
}

#[test]
fn test_aggregate_response_very_low_confidence() {
    let json = r#"{"aggregated_thought": "Test", "confidence": 0.01}"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert!((resp.confidence - 0.01).abs() < 1e-6);
}

#[test]
fn test_refine_response_large_quality_delta() {
    let json = r#"{
        "refined_thought": "Test",
        "quality_delta": 0.9
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert!((resp.quality_delta - 0.9).abs() < 1e-6);
}

#[test]
fn test_refine_response_very_negative_quality_delta() {
    let json = r#"{
        "refined_thought": "Test",
        "quality_delta": -0.9
    }"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert!((resp.quality_delta + 0.9).abs() < 1e-6);
}

#[test]
fn test_prune_params_threshold_negative() {
    let params = GotPruneParams::new("sess-123").with_threshold(-0.5);
    assert_eq!(params.threshold, Some(-0.5));
}

#[test]
fn test_prune_params_threshold_above_one() {
    let params = GotPruneParams::new("sess-123").with_threshold(1.5);
    assert_eq!(params.threshold, Some(1.5));
}

// ============================================================================
// Response Parsing - Empty and Missing Fields
// ============================================================================

#[test]
fn test_generate_response_empty_thought() {
    let json = r#"{"continuations": [{"thought": "", "confidence": 0.8}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations[0].thought, "");
}

#[test]
fn test_generate_response_empty_rationale() {
    let json = r#"{"continuations": [{"thought": "Test", "rationale": ""}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations[0].rationale, "");
}

#[test]
fn test_score_response_empty_rationale() {
    let json = r#"{
        "overall_score": 0.8,
        "breakdown": {"relevance": 0.8, "validity": 0.8, "depth": 0.8, "novelty": 0.8},
        "rationale": ""
    }"#;
    let resp = ScoreResponse::from_completion(json).unwrap();
    assert_eq!(resp.rationale, "");
}

#[test]
fn test_aggregate_response_empty_synthesis_approach() {
    let json = r#"{"aggregated_thought": "Test", "synthesis_approach": ""}"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert_eq!(resp.synthesis_approach, "");
}

#[test]
fn test_aggregate_response_missing_optional_fields() {
    let json = r#"{"aggregated_thought": "Test"}"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert_eq!(resp.aggregated_thought, "Test");
    assert_eq!(resp.confidence, 0.7); // default
    assert!(resp.sources_used.is_empty());
    assert_eq!(resp.synthesis_approach, "");
    assert!(resp.conflicts_resolved.is_empty());
}

#[test]
fn test_refine_response_missing_optional_fields() {
    let json = r#"{"refined_thought": "Test"}"#;
    let resp = RefineResponse::from_completion(json).unwrap();
    assert_eq!(resp.refined_thought, "Test");
    assert_eq!(resp.confidence, 0.7); // default
    assert!(resp.improvements_made.is_empty());
    assert_eq!(resp.quality_delta, 0.0); // default
}

// ============================================================================
// Response Parsing - Invalid JSON Structures
// ============================================================================

#[test]
fn test_generate_response_null_continuations_returns_error() {
    let json = r#"{"continuations": null}"#;
    // Null for required field returns error
    let result = GenerateResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_score_response_null_breakdown_returns_error() {
    let json = r#"{"overall_score": 0.8, "breakdown": null}"#;
    // Null for required field returns error
    let result = ScoreResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_aggregate_response_wrong_type_confidence_returns_error() {
    let json = r#"{"aggregated_thought": "Test", "confidence": "high"}"#;
    // Wrong type for field returns error
    let result = AggregateResponse::from_completion(json);
    assert!(result.is_err());
}

#[test]
fn test_refine_response_improvements_as_string_returns_error() {
    let json = r#"{"refined_thought": "Test", "improvements_made": "many"}"#;
    // Wrong type for field returns error
    let result = RefineResponse::from_completion(json);
    assert!(result.is_err());
}

// ============================================================================
// Result Deserialization - Additional Coverage
// ============================================================================

#[test]
fn test_init_result_deserialize() {
    let json = r#"{
        "session_id": "sess-abc",
        "root_node_id": "node-root",
        "content": "Initial thought",
        "config": {"max_nodes": 50, "max_depth": 5, "default_k": 3, "prune_threshold": 0.3}
    }"#;
    let result: GotInitResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.session_id, "sess-abc");
    assert_eq!(result.root_node_id, "node-root");
    assert_eq!(result.content, "Initial thought");
    assert_eq!(result.config.max_nodes, 50);
}

#[test]
fn test_score_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "node_id": "node-1",
        "overall_score": 0.82,
        "breakdown": {
            "relevance": 0.9,
            "validity": 0.8,
            "depth": 0.7,
            "novelty": 0.8
        },
        "is_terminal_candidate": true,
        "rationale": "High quality"
    }"#;
    let result: GotScoreResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.overall_score, 0.82);
    assert!(result.is_terminal_candidate);
    assert_eq!(result.breakdown.relevance, 0.9);
}

#[test]
fn test_aggregate_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "aggregated_node_id": "n-agg",
        "content": "Synthesized",
        "confidence": 0.88,
        "source_nodes": ["n1", "n2"],
        "synthesis_approach": "Consensus",
        "conflicts_resolved": ["Conflict 1"]
    }"#;
    let result: GotAggregateResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.aggregated_node_id, "n-agg");
    assert_eq!(result.confidence, 0.88);
    assert_eq!(result.source_nodes.len(), 2);
}

#[test]
fn test_refine_result_deserialize() {
    let json = r#"{
        "session_id": "sess-123",
        "original_node_id": "n1",
        "refined_node_id": "n1-refined",
        "content": "Refined",
        "confidence": 0.9,
        "improvements_made": ["Clarity"],
        "quality_delta": 0.15
    }"#;
    let result: GotRefineResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.original_node_id, "n1");
    assert_eq!(result.refined_node_id, "n1-refined");
    assert_eq!(result.quality_delta, 0.15);
}

// ============================================================================
// Long Content Tests
// ============================================================================

#[test]
fn test_init_params_very_long_content() {
    let long_content = "a".repeat(10000);
    let params = GotInitParams::new(&long_content);
    assert_eq!(params.content.len(), 10000);
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotInitParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content.len(), 10000);
}

#[test]
fn test_generate_response_very_long_thought() {
    let long_thought = "b".repeat(5000);
    let json = format!(
        r#"{{"continuations": [{{"thought": "{}"}}]}}"#,
        long_thought
    );
    let resp = GenerateResponse::from_completion(&json).unwrap();
    assert_eq!(resp.continuations[0].thought.len(), 5000);
}

#[test]
fn test_score_params_very_long_problem() {
    let long_problem = "c".repeat(3000);
    let params = GotScoreParams::new("sess-123", "node-1").with_problem(&long_problem);
    assert_eq!(params.problem.as_ref().unwrap().len(), 3000);
}

#[test]
fn test_aggregate_result_very_long_content() {
    let long_content = "d".repeat(8000);
    let result = GotAggregateResult {
        session_id: "sess-123".to_string(),
        aggregated_node_id: "n-agg".to_string(),
        content: long_content.clone(),
        confidence: 0.88,
        source_nodes: vec![],
        synthesis_approach: "Test".to_string(),
        conflicts_resolved: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: GotAggregateResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content.len(), 8000);
}

#[test]
fn test_refine_response_many_improvements_long_text() {
    let improvements: Vec<String> = (0..50).map(|i| format!("Improvement {}", i)).collect();
    let json_improvements = improvements
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{"refined_thought": "Test", "improvements_made": [{}]}}"#,
        json_improvements
    );
    let resp = RefineResponse::from_completion(&json).unwrap();
    assert_eq!(resp.improvements_made.len(), 50);
}

// ============================================================================
// Special Characters in Strings
// ============================================================================

#[test]
fn test_init_params_special_chars() {
    let params = GotInitParams::new("Content with \"quotes\" and \n newlines \t tabs");
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotInitParams = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.content,
        "Content with \"quotes\" and \n newlines \t tabs"
    );
}

#[test]
fn test_generate_response_newlines_in_thought() {
    let json = r#"{"continuations": [{"thought": "Line 1\nLine 2\nLine 3"}]}"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert!(resp.continuations[0].thought.contains("\n"));
}

#[test]
fn test_score_params_backslashes() {
    let params =
        GotScoreParams::new("sess-123", "node-1").with_problem("Path: C:\\Users\\test\\file.txt");
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotScoreParams = serde_json::from_str(&json).unwrap();
    assert!(parsed.problem.unwrap().contains("\\"));
}

#[test]
fn test_aggregate_response_control_characters() {
    let json = r#"{"aggregated_thought": "Test\u0000\u0001\u0002"}"#;
    let resp = AggregateResponse::from_completion(json).unwrap();
    assert!(resp.aggregated_thought.contains("Test"));
}

// ============================================================================
// Multiple Continuations with Varied Data
// ============================================================================

#[test]
fn test_generate_response_mixed_confidence_values() {
    let json = r#"{
        "continuations": [
            {"thought": "T1", "confidence": 0.0},
            {"thought": "T2", "confidence": 0.5},
            {"thought": "T3", "confidence": 1.0},
            {"thought": "T4", "confidence": 0.3333},
            {"thought": "T5", "confidence": 0.6666}
        ]
    }"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations.len(), 5);
    assert_eq!(resp.continuations[0].confidence, 0.0);
    assert_eq!(resp.continuations[2].confidence, 1.0);
}

#[test]
fn test_generate_response_mixed_novelty_values() {
    let json = r#"{
        "continuations": [
            {"thought": "T1", "novelty": 0.1},
            {"thought": "T2", "novelty": 0.9},
            {"thought": "T3"}
        ]
    }"#;
    let resp = GenerateResponse::from_completion(json).unwrap();
    assert_eq!(resp.continuations[0].novelty, 0.1);
    assert_eq!(resp.continuations[1].novelty, 0.9);
    assert_eq!(resp.continuations[2].novelty, 0.0); // default
}

// ============================================================================
// Aggregate Params Edge Cases
// ============================================================================

#[test]
fn test_aggregate_params_many_nodes() {
    let node_ids: Vec<String> = (0..100).map(|i| format!("node-{}", i)).collect();
    let params = GotAggregateParams::new("sess-123", node_ids.clone());
    assert_eq!(params.node_ids.len(), 100);
    let json = serde_json::to_string(&params).unwrap();
    let parsed: GotAggregateParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.node_ids.len(), 100);
}

#[test]
fn test_aggregate_params_duplicate_node_ids() {
    let params = GotAggregateParams::new(
        "sess-123",
        vec![
            "node-1".to_string(),
            "node-1".to_string(),
            "node-2".to_string(),
        ],
    );
    assert_eq!(params.node_ids.len(), 3);
    assert_eq!(params.node_ids[0], params.node_ids[1]);
}

// ============================================================================
// State Result Edge Cases
// ============================================================================

#[test]
fn test_state_result_negative_max_depth() {
    let result = GotStateResult {
        session_id: "sess-123".to_string(),
        total_nodes: 1,
        active_nodes: 1,
        terminal_nodes: 0,
        total_edges: 0,
        max_depth: -1,
        root_node_ids: vec!["root".to_string()],
        active_node_ids: vec![],
        terminal_node_ids: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"max_depth\":-1"));
}

#[test]
fn test_state_result_large_graph() {
    let result = GotStateResult {
        session_id: "sess-123".to_string(),
        total_nodes: 10000,
        active_nodes: 500,
        terminal_nodes: 100,
        total_edges: 15000,
        max_depth: 50,
        root_node_ids: vec!["root".to_string()],
        active_node_ids: (0..500).map(|i| format!("a{}", i)).collect(),
        terminal_node_ids: (0..100).map(|i| format!("t{}", i)).collect(),
    };
    assert_eq!(result.active_node_ids.len(), 500);
    assert_eq!(result.terminal_node_ids.len(), 100);
}

// ============================================================================
// Generated Continuation Edge Cases
// ============================================================================

#[test]
fn test_generated_continuation_deserialize() {
    let json = r#"{
        "node_id": "node-1",
        "content": "Content",
        "confidence": 0.85,
        "novelty": 0.75,
        "rationale": "Reason"
    }"#;
    let cont: GeneratedContinuation = serde_json::from_str(json).unwrap();
    assert_eq!(cont.node_id, "node-1");
    assert_eq!(cont.confidence, 0.85);
}

#[test]
fn test_generated_continuation_empty_fields() {
    let cont = GeneratedContinuation {
        node_id: "".to_string(),
        content: "".to_string(),
        confidence: 0.0,
        novelty: 0.0,
        rationale: "".to_string(),
    };
    let json = serde_json::to_string(&cont).unwrap();
    let parsed: GeneratedContinuation = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.node_id, "");
}

// ============================================================================
// Score Breakdown Edge Cases
// ============================================================================

#[test]
fn test_score_breakdown_deserialize() {
    let json = r#"{
        "relevance": 0.8,
        "validity": 0.7,
        "depth": 0.6,
        "novelty": 0.5
    }"#;
    let breakdown: ScoreBreakdown = serde_json::from_str(json).unwrap();
    assert_eq!(breakdown.relevance, 0.8);
    assert_eq!(breakdown.validity, 0.7);
}

#[test]
fn test_score_breakdown_all_zeros() {
    let breakdown = ScoreBreakdown {
        relevance: 0.0,
        validity: 0.0,
        depth: 0.0,
        novelty: 0.0,
    };
    let json = serde_json::to_string(&breakdown).unwrap();
    let parsed: ScoreBreakdown = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.relevance, 0.0);
}

#[test]
fn test_score_breakdown_all_ones() {
    let breakdown = ScoreBreakdown {
        relevance: 1.0,
        validity: 1.0,
        depth: 1.0,
        novelty: 1.0,
    };
    let json = serde_json::to_string(&breakdown).unwrap();
    let parsed: ScoreBreakdown = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.novelty, 1.0);
}
