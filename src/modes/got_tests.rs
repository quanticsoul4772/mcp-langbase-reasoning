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
    let resp = GenerateResponse::from_completion(json);
    assert_eq!(resp.continuations.len(), 1);
    assert_eq!(resp.continuations[0].thought, "Idea 1");
    assert_eq!(resp.continuations[0].confidence, 0.9);
}

#[test]
fn test_generate_response_from_plain_text() {
    let text = "Plain text response";
    let resp = GenerateResponse::from_completion(text);
    assert_eq!(resp.continuations.len(), 1);
    assert_eq!(resp.continuations[0].thought, "Plain text response");
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
    let resp = GenerateResponse::from_completion(json);
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
    let resp = GenerateResponse::from_completion(json);
    assert!(resp.metadata.is_some());
    assert_eq!(resp.continuations.len(), 1);
}

#[test]
fn test_generate_response_with_defaults() {
    let json = r#"{"continuations": [{"thought": "Minimal"}]}"#;
    let resp = GenerateResponse::from_completion(json);
    assert_eq!(resp.continuations[0].confidence, 0.7); // default
    assert_eq!(resp.continuations[0].novelty, 0.0); // default
}

#[test]
fn test_generate_response_empty_continuations() {
    let json = r#"{"continuations": []}"#;
    let resp = GenerateResponse::from_completion(json);
    assert!(resp.continuations.is_empty());
}

// ============================================================================
// Response Parsing Tests - Score
// ============================================================================

#[test]
fn test_score_response_from_json() {
    let json = r#"{"overall_score": 0.85, "breakdown": {"relevance": 0.9, "validity": 0.8, "depth": 0.7, "novelty": 0.6}, "is_terminal_candidate": true, "rationale": "Good"}"#;
    let resp = ScoreResponse::from_completion(json);
    assert_eq!(resp.overall_score, 0.85);
    assert!(resp.is_terminal_candidate);
    assert_eq!(resp.breakdown.relevance, 0.9);
}

#[test]
fn test_score_response_from_plain_text() {
    let text = "Invalid";
    let resp = ScoreResponse::from_completion(text);
    assert_eq!(resp.overall_score, 0.5);
    assert!(!resp.is_terminal_candidate);
}

#[test]
fn test_score_response_partial_breakdown() {
    let json = r#"{
        "overall_score": 0.75,
        "breakdown": {"relevance": 0.8},
        "is_terminal_candidate": false,
        "rationale": "Partial"
    }"#;
    let resp = ScoreResponse::from_completion(json);
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
    let resp = AggregateResponse::from_completion(json);
    assert_eq!(resp.aggregated_thought, "Combined insight");
    assert_eq!(resp.confidence, 0.88);
}

#[test]
fn test_aggregate_response_plain_text() {
    let text = "Non-JSON aggregate response";
    let resp = AggregateResponse::from_completion(text);
    assert_eq!(resp.aggregated_thought, text);
    assert_eq!(resp.confidence, 0.7); // default
}

// ============================================================================
// Response Parsing Tests - Refine
// ============================================================================

#[test]
fn test_refine_response_from_json() {
    let json = r#"{"refined_thought": "Improved", "confidence": 0.9, "improvements_made": ["Clarity"], "quality_delta": 0.15}"#;
    let resp = RefineResponse::from_completion(json);
    assert_eq!(resp.refined_thought, "Improved");
    assert_eq!(resp.quality_delta, 0.15);
    assert_eq!(resp.improvements_made.len(), 1);
}

#[test]
fn test_refine_response_plain_text() {
    let text = "Improved thought content";
    let resp = RefineResponse::from_completion(text);
    assert_eq!(resp.refined_thought, text);
    // Fallback includes a default improvement
    assert_eq!(resp.improvements_made.len(), 1);
    assert!(resp.improvements_made[0].contains("fallback"));
}

#[test]
fn test_refine_response_with_all_fields() {
    let json = r#"{
        "refined_thought": "Better version",
        "confidence": 0.95,
        "improvements_made": ["Clarity", "Structure", "Evidence"],
        "quality_delta": 0.25
    }"#;
    let resp = RefineResponse::from_completion(json);
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
