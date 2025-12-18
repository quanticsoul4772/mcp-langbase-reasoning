//! Unit tests for storage types and builder patterns.
//!
//! Tests validation, clamping, serialization, and builder methods
//! for Session, Thought, Branch, CrossRef, Checkpoint, Invocation,
//! GraphNode, GraphEdge, StateSnapshot, and Detection types.

use super::*;
use serde_json::json;

// ============================================================================
// Session tests
// ============================================================================

#[test]
fn test_session_new() {
    let session = Session::new("linear");
    assert!(!session.id.is_empty());
    assert_eq!(session.mode, "linear");
    assert!(session.metadata.is_none());
    assert!(session.active_branch_id.is_none());
}

#[test]
fn test_session_with_active_branch() {
    let session = Session::new("tree").with_active_branch("branch-123");
    assert_eq!(session.mode, "tree");
    assert_eq!(session.active_branch_id, Some("branch-123".to_string()));
}

// ============================================================================
// Thought tests
// ============================================================================

#[test]
fn test_thought_new() {
    let thought = Thought::new("sess-1", "Test content", "linear");
    assert!(!thought.id.is_empty());
    assert_eq!(thought.session_id, "sess-1");
    assert_eq!(thought.content, "Test content");
    assert_eq!(thought.mode, "linear");
    assert_eq!(thought.confidence, 0.8); // default
    assert!(thought.parent_id.is_none());
    assert!(thought.branch_id.is_none());
}

#[test]
fn test_thought_with_confidence() {
    let thought = Thought::new("sess-1", "Test", "linear").with_confidence(0.95);
    assert_eq!(thought.confidence, 0.95);
}

#[test]
fn test_thought_confidence_clamp() {
    let high = Thought::new("sess-1", "Test", "linear").with_confidence(1.5);
    assert_eq!(high.confidence, 1.0);

    let low = Thought::new("sess-1", "Test", "linear").with_confidence(-0.5);
    assert_eq!(low.confidence, 0.0);
}

#[test]
fn test_thought_with_parent() {
    let thought = Thought::new("sess-1", "Test", "linear").with_parent("parent-123");
    assert_eq!(thought.parent_id, Some("parent-123".to_string()));
}

#[test]
fn test_thought_with_branch() {
    let thought = Thought::new("sess-1", "Test", "tree").with_branch("branch-456");
    assert_eq!(thought.branch_id, Some("branch-456".to_string()));
}

#[test]
fn test_thought_with_metadata() {
    let metadata = json!({"key": "value"});
    let thought = Thought::new("sess-1", "Test", "linear").with_metadata(metadata.clone());
    assert_eq!(thought.metadata, Some(metadata));
}

#[test]
fn test_thought_builder_chain() {
    let thought = Thought::new("sess-1", "Complex thought", "tree")
        .with_confidence(0.9)
        .with_parent("parent-1")
        .with_branch("branch-1")
        .with_metadata(json!({"priority": "high"}));

    assert_eq!(thought.confidence, 0.9);
    assert_eq!(thought.parent_id, Some("parent-1".to_string()));
    assert_eq!(thought.branch_id, Some("branch-1".to_string()));
    assert!(thought.metadata.is_some());
}

// ============================================================================
// Branch tests
// ============================================================================

#[test]
fn test_branch_new() {
    let branch = Branch::new("sess-1");
    assert!(!branch.id.is_empty());
    assert_eq!(branch.session_id, "sess-1");
    assert!(branch.name.is_none());
    assert!(branch.parent_branch_id.is_none());
    assert_eq!(branch.priority, 1.0);
    assert_eq!(branch.confidence, 0.8);
    assert_eq!(branch.state, BranchState::Active);
}

#[test]
fn test_branch_with_name() {
    let branch = Branch::new("sess-1").with_name("Main branch");
    assert_eq!(branch.name, Some("Main branch".to_string()));
}

#[test]
fn test_branch_with_parent() {
    let branch = Branch::new("sess-1").with_parent("parent-branch");
    assert_eq!(branch.parent_branch_id, Some("parent-branch".to_string()));
}

#[test]
fn test_branch_with_priority() {
    let branch = Branch::new("sess-1").with_priority(0.5);
    assert_eq!(branch.priority, 0.5);
}

#[test]
fn test_branch_with_confidence() {
    let branch = Branch::new("sess-1").with_confidence(0.95);
    assert_eq!(branch.confidence, 0.95);
}

#[test]
fn test_branch_confidence_clamp() {
    let high = Branch::new("sess-1").with_confidence(1.5);
    assert_eq!(high.confidence, 1.0);

    let low = Branch::new("sess-1").with_confidence(-0.5);
    assert_eq!(low.confidence, 0.0);
}

#[test]
fn test_branch_with_state() {
    let completed = Branch::new("sess-1").with_state(BranchState::Completed);
    assert_eq!(completed.state, BranchState::Completed);

    let abandoned = Branch::new("sess-1").with_state(BranchState::Abandoned);
    assert_eq!(abandoned.state, BranchState::Abandoned);
}

// ============================================================================
// BranchState tests
// ============================================================================

#[test]
fn test_branch_state_display() {
    assert_eq!(BranchState::Active.to_string(), "active");
    assert_eq!(BranchState::Completed.to_string(), "completed");
    assert_eq!(BranchState::Abandoned.to_string(), "abandoned");
}

#[test]
fn test_branch_state_from_str() {
    assert_eq!(
        "active".parse::<BranchState>().unwrap(),
        BranchState::Active
    );
    assert_eq!(
        "completed".parse::<BranchState>().unwrap(),
        BranchState::Completed
    );
    assert_eq!(
        "abandoned".parse::<BranchState>().unwrap(),
        BranchState::Abandoned
    );
    assert_eq!(
        "ACTIVE".parse::<BranchState>().unwrap(),
        BranchState::Active
    ); // case insensitive
}

#[test]
fn test_branch_state_from_str_invalid() {
    let result = "invalid".parse::<BranchState>();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown branch state"));
}

#[test]
fn test_branch_state_default() {
    assert_eq!(BranchState::default(), BranchState::Active);
}

// ============================================================================
// CrossRef tests
// ============================================================================

#[test]
fn test_cross_ref_new() {
    let cross_ref = CrossRef::new("branch-1", "branch-2", CrossRefType::Supports);
    assert!(!cross_ref.id.is_empty());
    assert_eq!(cross_ref.from_branch_id, "branch-1");
    assert_eq!(cross_ref.to_branch_id, "branch-2");
    assert_eq!(cross_ref.ref_type, CrossRefType::Supports);
    assert!(cross_ref.reason.is_none());
    assert_eq!(cross_ref.strength, 1.0);
}

#[test]
fn test_cross_ref_with_reason() {
    let cross_ref =
        CrossRef::new("b1", "b2", CrossRefType::Contradicts).with_reason("Opposing views");
    assert_eq!(cross_ref.reason, Some("Opposing views".to_string()));
}

#[test]
fn test_cross_ref_with_strength() {
    let cross_ref = CrossRef::new("b1", "b2", CrossRefType::Extends).with_strength(0.7);
    assert_eq!(cross_ref.strength, 0.7);
}

#[test]
fn test_cross_ref_strength_clamp() {
    let high = CrossRef::new("b1", "b2", CrossRefType::Supports).with_strength(1.5);
    assert_eq!(high.strength, 1.0);

    let low = CrossRef::new("b1", "b2", CrossRefType::Supports).with_strength(-0.5);
    assert_eq!(low.strength, 0.0);
}

// ============================================================================
// CrossRefType tests
// ============================================================================

#[test]
fn test_cross_ref_type_display() {
    assert_eq!(CrossRefType::Supports.to_string(), "supports");
    assert_eq!(CrossRefType::Contradicts.to_string(), "contradicts");
    assert_eq!(CrossRefType::Extends.to_string(), "extends");
    assert_eq!(CrossRefType::Alternative.to_string(), "alternative");
    assert_eq!(CrossRefType::Depends.to_string(), "depends");
}

#[test]
fn test_cross_ref_type_from_str() {
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
    assert_eq!(
        "SUPPORTS".parse::<CrossRefType>().unwrap(),
        CrossRefType::Supports
    ); // case insensitive
}

#[test]
fn test_cross_ref_type_from_str_invalid() {
    let result = "invalid".parse::<CrossRefType>();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown cross-ref type"));
}

// ============================================================================
// Checkpoint tests
// ============================================================================

#[test]
fn test_checkpoint_new() {
    let snapshot = json!({"state": "saved"});
    let checkpoint = Checkpoint::new("sess-1", "Checkpoint 1", snapshot.clone());
    assert!(!checkpoint.id.is_empty());
    assert_eq!(checkpoint.session_id, "sess-1");
    assert_eq!(checkpoint.name, "Checkpoint 1");
    assert!(checkpoint.branch_id.is_none());
    assert!(checkpoint.description.is_none());
    assert_eq!(checkpoint.snapshot, snapshot);
}

#[test]
fn test_checkpoint_with_branch() {
    let checkpoint = Checkpoint::new("sess-1", "CP", json!({})).with_branch("branch-123");
    assert_eq!(checkpoint.branch_id, Some("branch-123".to_string()));
}

#[test]
fn test_checkpoint_with_description() {
    let checkpoint =
        Checkpoint::new("sess-1", "CP", json!({})).with_description("Before major changes");
    assert_eq!(
        checkpoint.description,
        Some("Before major changes".to_string())
    );
}

// ============================================================================
// Invocation tests
// ============================================================================

#[test]
fn test_invocation_new() {
    let input = json!({"content": "test"});
    let invocation = Invocation::new("reasoning.linear", input.clone());
    assert!(!invocation.id.is_empty());
    assert_eq!(invocation.tool_name, "reasoning.linear");
    assert_eq!(invocation.input, input);
    assert!(invocation.session_id.is_none());
    assert!(invocation.output.is_none());
    assert!(invocation.pipe_name.is_none());
    assert!(invocation.latency_ms.is_none());
    assert!(invocation.success);
    assert!(invocation.error.is_none());
}

#[test]
fn test_invocation_with_session() {
    let invocation = Invocation::new("reasoning.tree", json!({})).with_session("sess-123");
    assert_eq!(invocation.session_id, Some("sess-123".to_string()));
}

#[test]
fn test_invocation_with_pipe() {
    let invocation =
        Invocation::new("reasoning.linear", json!({})).with_pipe("linear-reasoning-v1");
    assert_eq!(
        invocation.pipe_name,
        Some("linear-reasoning-v1".to_string())
    );
}

#[test]
fn test_invocation_success() {
    let output = json!({"result": "success"});
    let invocation = Invocation::new("reasoning.linear", json!({})).success(output.clone(), 150);
    assert!(invocation.success);
    assert_eq!(invocation.output, Some(output));
    assert_eq!(invocation.latency_ms, Some(150));
    assert!(invocation.error.is_none());
}

#[test]
fn test_invocation_failure() {
    let invocation = Invocation::new("reasoning.linear", json!({})).failure("API timeout", 5000);
    assert!(!invocation.success);
    assert_eq!(invocation.error, Some("API timeout".to_string()));
    assert_eq!(invocation.latency_ms, Some(5000));
    assert!(invocation.output.is_none());
}

#[test]
fn test_invocation_builder_chain() {
    let invocation = Invocation::new("reasoning.tree", json!({"content": "test"}))
        .with_session("sess-1")
        .with_pipe("tree-reasoning-v1")
        .success(json!({"thought": "result"}), 200);

    assert_eq!(invocation.session_id, Some("sess-1".to_string()));
    assert_eq!(invocation.pipe_name, Some("tree-reasoning-v1".to_string()));
    assert!(invocation.success);
    assert_eq!(invocation.latency_ms, Some(200));
}

// ============================================================================
// GraphNode tests
// ============================================================================

#[test]
fn test_graph_node_new() {
    let node = GraphNode::new("sess-1", "Test content");
    assert!(!node.id.is_empty());
    assert_eq!(node.session_id, "sess-1");
    assert_eq!(node.content, "Test content");
    assert_eq!(node.node_type, NodeType::Thought);
    assert!(node.score.is_none());
    assert_eq!(node.depth, 0);
    assert!(!node.is_terminal);
    assert!(!node.is_root);
    assert!(node.is_active);
}

#[test]
fn test_graph_node_with_type() {
    let node = GraphNode::new("sess-1", "Hypothesis").with_type(NodeType::Hypothesis);
    assert_eq!(node.node_type, NodeType::Hypothesis);
}

#[test]
fn test_graph_node_with_score() {
    let node = GraphNode::new("sess-1", "Scored").with_score(0.85);
    assert_eq!(node.score, Some(0.85));
}

#[test]
fn test_graph_node_score_clamp() {
    let high = GraphNode::new("sess-1", "High").with_score(1.5);
    assert_eq!(high.score, Some(1.0));

    let low = GraphNode::new("sess-1", "Low").with_score(-0.5);
    assert_eq!(low.score, Some(0.0));
}

#[test]
fn test_graph_node_with_depth() {
    let node = GraphNode::new("sess-1", "Deep").with_depth(3);
    assert_eq!(node.depth, 3);
}

#[test]
fn test_graph_node_as_terminal() {
    let node = GraphNode::new("sess-1", "Terminal").as_terminal();
    assert!(node.is_terminal);
}

#[test]
fn test_graph_node_as_root() {
    let node = GraphNode::new("sess-1", "Root").as_root();
    assert!(node.is_root);
}

#[test]
fn test_graph_node_as_inactive() {
    let node = GraphNode::new("sess-1", "Pruned").as_inactive();
    assert!(!node.is_active);
}

#[test]
fn test_graph_node_builder_chain() {
    let node = GraphNode::new("sess-1", "Complex node")
        .with_type(NodeType::Conclusion)
        .with_score(0.9)
        .with_depth(2)
        .as_terminal();

    assert_eq!(node.node_type, NodeType::Conclusion);
    assert_eq!(node.score, Some(0.9));
    assert_eq!(node.depth, 2);
    assert!(node.is_terminal);
}

#[test]
fn test_graph_node_as_active() {
    let node = GraphNode::new("sess-1", "Active").as_inactive().as_active();
    assert!(node.is_active);
}

// ============================================================================
// NodeType tests
// ============================================================================

#[test]
fn test_node_type_display() {
    assert_eq!(NodeType::Thought.to_string(), "thought");
    assert_eq!(NodeType::Hypothesis.to_string(), "hypothesis");
    assert_eq!(NodeType::Conclusion.to_string(), "conclusion");
    assert_eq!(NodeType::Aggregation.to_string(), "aggregation");
    assert_eq!(NodeType::Root.to_string(), "root");
    assert_eq!(NodeType::Refinement.to_string(), "refinement");
    assert_eq!(NodeType::Terminal.to_string(), "terminal");
}

#[test]
fn test_node_type_from_str() {
    assert_eq!("thought".parse::<NodeType>().unwrap(), NodeType::Thought);
    assert_eq!(
        "hypothesis".parse::<NodeType>().unwrap(),
        NodeType::Hypothesis
    );
    assert_eq!(
        "conclusion".parse::<NodeType>().unwrap(),
        NodeType::Conclusion
    );
    assert_eq!(
        "aggregation".parse::<NodeType>().unwrap(),
        NodeType::Aggregation
    );
    assert_eq!("root".parse::<NodeType>().unwrap(), NodeType::Root);
    assert_eq!(
        "refinement".parse::<NodeType>().unwrap(),
        NodeType::Refinement
    );
    assert_eq!("terminal".parse::<NodeType>().unwrap(), NodeType::Terminal);
    assert_eq!("THOUGHT".parse::<NodeType>().unwrap(), NodeType::Thought);
}

#[test]
fn test_node_type_from_str_invalid() {
    let result = "invalid".parse::<NodeType>();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown node type"));
}

#[test]
fn test_node_type_default() {
    assert_eq!(NodeType::default(), NodeType::Thought);
}

// ============================================================================
// GraphEdge tests
// ============================================================================

#[test]
fn test_graph_edge_new() {
    let edge = GraphEdge::new("sess-1", "node-1", "node-2");
    assert!(!edge.id.is_empty());
    assert_eq!(edge.session_id, "sess-1");
    assert_eq!(edge.from_node, "node-1");
    assert_eq!(edge.to_node, "node-2");
    assert_eq!(edge.edge_type, EdgeType::Generates);
    assert_eq!(edge.weight, 1.0);
}

#[test]
fn test_graph_edge_with_type() {
    let edge = GraphEdge::new("sess-1", "n1", "n2").with_type(EdgeType::Refines);
    assert_eq!(edge.edge_type, EdgeType::Refines);
}

#[test]
fn test_graph_edge_with_weight() {
    let edge = GraphEdge::new("sess-1", "n1", "n2").with_weight(0.75);
    assert_eq!(edge.weight, 0.75);
}

#[test]
fn test_graph_edge_weight_clamp() {
    let high = GraphEdge::new("sess-1", "n1", "n2").with_weight(1.5);
    assert_eq!(high.weight, 1.0);

    let low = GraphEdge::new("sess-1", "n1", "n2").with_weight(-0.5);
    assert_eq!(low.weight, 0.0);
}

// ============================================================================
// EdgeType tests
// ============================================================================

#[test]
fn test_edge_type_display() {
    assert_eq!(EdgeType::Generates.to_string(), "generates");
    assert_eq!(EdgeType::Refines.to_string(), "refines");
    assert_eq!(EdgeType::Aggregates.to_string(), "aggregates");
    assert_eq!(EdgeType::Supports.to_string(), "supports");
    assert_eq!(EdgeType::Contradicts.to_string(), "contradicts");
}

#[test]
fn test_edge_type_from_str() {
    assert_eq!(
        "generates".parse::<EdgeType>().unwrap(),
        EdgeType::Generates
    );
    assert_eq!("refines".parse::<EdgeType>().unwrap(), EdgeType::Refines);
    assert_eq!(
        "aggregates".parse::<EdgeType>().unwrap(),
        EdgeType::Aggregates
    );
    assert_eq!("supports".parse::<EdgeType>().unwrap(), EdgeType::Supports);
    assert_eq!(
        "contradicts".parse::<EdgeType>().unwrap(),
        EdgeType::Contradicts
    );
    assert_eq!(
        "GENERATES".parse::<EdgeType>().unwrap(),
        EdgeType::Generates
    );
}

#[test]
fn test_edge_type_from_str_invalid() {
    let result = "invalid".parse::<EdgeType>();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown edge type"));
}

#[test]
fn test_edge_type_default() {
    assert_eq!(EdgeType::default(), EdgeType::Generates);
}

// ============================================================================
// StateSnapshot tests
// ============================================================================

#[test]
fn test_state_snapshot_new() {
    let data = json!({"state": "saved"});
    let snapshot = StateSnapshot::new("sess-1", data.clone());
    assert!(!snapshot.id.is_empty());
    assert_eq!(snapshot.session_id, "sess-1");
    assert_eq!(snapshot.snapshot_type, SnapshotType::Full);
    assert_eq!(snapshot.state_data, data);
    assert!(snapshot.parent_snapshot_id.is_none());
    assert!(snapshot.description.is_none());
}

#[test]
fn test_state_snapshot_with_type() {
    let snapshot = StateSnapshot::new("sess-1", json!({})).with_type(SnapshotType::Incremental);
    assert_eq!(snapshot.snapshot_type, SnapshotType::Incremental);
}

#[test]
fn test_state_snapshot_with_parent() {
    let snapshot = StateSnapshot::new("sess-1", json!({})).with_parent("snap-parent");
    assert_eq!(snapshot.parent_snapshot_id, Some("snap-parent".to_string()));
}

#[test]
fn test_state_snapshot_with_description() {
    let snapshot =
        StateSnapshot::new("sess-1", json!({})).with_description("Before major refactor");
    assert_eq!(
        snapshot.description,
        Some("Before major refactor".to_string())
    );
}

#[test]
fn test_state_snapshot_builder_chain() {
    let snapshot = StateSnapshot::new("sess-1", json!({"key": "value"}))
        .with_type(SnapshotType::Branch)
        .with_parent("parent-snap")
        .with_description("Branch snapshot");

    assert_eq!(snapshot.snapshot_type, SnapshotType::Branch);
    assert_eq!(snapshot.parent_snapshot_id, Some("parent-snap".to_string()));
    assert_eq!(snapshot.description, Some("Branch snapshot".to_string()));
}

// ============================================================================
// SnapshotType tests
// ============================================================================

#[test]
fn test_snapshot_type_display() {
    assert_eq!(SnapshotType::Full.to_string(), "full");
    assert_eq!(SnapshotType::Incremental.to_string(), "incremental");
    assert_eq!(SnapshotType::Branch.to_string(), "branch");
}

#[test]
fn test_snapshot_type_from_str() {
    assert_eq!("full".parse::<SnapshotType>().unwrap(), SnapshotType::Full);
    assert_eq!(
        "incremental".parse::<SnapshotType>().unwrap(),
        SnapshotType::Incremental
    );
    assert_eq!(
        "branch".parse::<SnapshotType>().unwrap(),
        SnapshotType::Branch
    );
    assert_eq!("FULL".parse::<SnapshotType>().unwrap(), SnapshotType::Full);
}

#[test]
fn test_snapshot_type_from_str_invalid() {
    let result = "invalid".parse::<SnapshotType>();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown snapshot type"));
}

#[test]
fn test_snapshot_type_default() {
    assert_eq!(SnapshotType::default(), SnapshotType::Full);
}

// ============================================================================
// Detection tests
// ============================================================================

#[test]
fn test_detection_new() {
    let detection = Detection::new(
        DetectionType::Bias,
        "confirmation_bias",
        3,
        0.85,
        "Evidence shows selective information gathering",
    );
    assert!(!detection.id.is_empty());
    assert!(detection.session_id.is_none());
    assert!(detection.thought_id.is_none());
    assert_eq!(detection.detection_type, DetectionType::Bias);
    assert_eq!(detection.detected_issue, "confirmation_bias");
    assert_eq!(detection.severity, 3);
    assert!((detection.confidence - 0.85).abs() < 0.001);
    assert_eq!(
        detection.explanation,
        "Evidence shows selective information gathering"
    );
    assert!(detection.remediation.is_none());
}

#[test]
fn test_detection_with_session() {
    let detection = Detection::new(
        DetectionType::Fallacy,
        "ad_hominem",
        4,
        0.9,
        "Attack on person",
    )
    .with_session("sess-123");
    assert_eq!(detection.session_id, Some("sess-123".to_string()));
}

#[test]
fn test_detection_with_thought() {
    let detection = Detection::new(
        DetectionType::Bias,
        "anchoring",
        2,
        0.75,
        "Over-reliance on first data",
    )
    .with_thought("thought-456");
    assert_eq!(detection.thought_id, Some("thought-456".to_string()));
}

#[test]
fn test_detection_with_remediation() {
    let detection = Detection::new(
        DetectionType::Fallacy,
        "straw_man",
        3,
        0.8,
        "Misrepresentation of argument",
    )
    .with_remediation("Address the actual argument as stated");
    assert_eq!(
        detection.remediation,
        Some("Address the actual argument as stated".to_string())
    );
}

#[test]
fn test_detection_with_metadata() {
    let metadata = json!({"source": "analysis", "context": "debate"});
    let detection = Detection::new(
        DetectionType::Bias,
        "sunk_cost",
        4,
        0.88,
        "Continuing due to investment",
    )
    .with_metadata(metadata.clone());
    assert_eq!(detection.metadata, Some(metadata));
}

#[test]
fn test_detection_severity_clamp() {
    let too_high = Detection::new(DetectionType::Bias, "test", 10, 0.5, "test");
    assert_eq!(too_high.severity, 5);

    let too_low = Detection::new(DetectionType::Fallacy, "test", -5, 0.5, "test");
    assert_eq!(too_low.severity, 1);
}

#[test]
fn test_detection_confidence_clamp() {
    let too_high = Detection::new(DetectionType::Bias, "test", 3, 1.5, "test");
    assert!((too_high.confidence - 1.0).abs() < 0.001);

    let too_low = Detection::new(DetectionType::Fallacy, "test", 3, -0.5, "test");
    assert!((too_low.confidence - 0.0).abs() < 0.001);
}

#[test]
fn test_detection_builder_chain() {
    let detection = Detection::new(
        DetectionType::Fallacy,
        "circular_reasoning",
        4,
        0.92,
        "Conclusion used as premise",
    )
    .with_session("sess-1")
    .with_thought("thought-1")
    .with_remediation("Provide independent evidence")
    .with_metadata(json!({"category": "formal"}));

    assert_eq!(detection.session_id, Some("sess-1".to_string()));
    assert_eq!(detection.thought_id, Some("thought-1".to_string()));
    assert!(detection.remediation.is_some());
    assert!(detection.metadata.is_some());
}

// ============================================================================
// DetectionType tests
// ============================================================================

#[test]
fn test_detection_type_display() {
    assert_eq!(DetectionType::Bias.to_string(), "bias");
    assert_eq!(DetectionType::Fallacy.to_string(), "fallacy");
}

#[test]
fn test_detection_type_from_str() {
    assert_eq!(
        "bias".parse::<DetectionType>().unwrap(),
        DetectionType::Bias
    );
    assert_eq!(
        "fallacy".parse::<DetectionType>().unwrap(),
        DetectionType::Fallacy
    );
    assert_eq!(
        "BIAS".parse::<DetectionType>().unwrap(),
        DetectionType::Bias
    ); // case insensitive
}

#[test]
fn test_detection_type_from_str_invalid() {
    let result = "invalid".parse::<DetectionType>();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown detection type"));
}

#[test]
fn test_detection_type_default() {
    assert_eq!(DetectionType::default(), DetectionType::Bias);
}

// ============================================================================
// Decision tests
// ============================================================================

#[test]
fn test_decision_new() {
    let options = vec!["Option A".to_string(), "Option B".to_string()];
    let recommendation = json!({"option": "Option A", "score": 0.9});
    let scores =
        json!([{"option": "Option A", "score": 0.9}, {"option": "Option B", "score": 0.7}]);
    let decision = Decision::new(
        "sess-1",
        "Choose best option",
        options.clone(),
        "weighted_sum",
        recommendation.clone(),
        scores.clone(),
    );

    assert!(!decision.id.is_empty());
    assert_eq!(decision.session_id, "sess-1");
    assert_eq!(decision.question, "Choose best option");
    assert_eq!(decision.options, options);
    assert!(decision.criteria.is_none());
    assert_eq!(decision.method, "weighted_sum");
    assert_eq!(decision.recommendation, recommendation);
    assert_eq!(decision.scores, scores);
    assert!(decision.sensitivity_analysis.is_none());
    assert!(decision.trade_offs.is_none());
    assert!(decision.constraints_satisfied.is_none());
    assert!(decision.metadata.is_none());
}

#[test]
fn test_decision_with_criteria() {
    let criteria = vec![
        StoredCriterion {
            name: "Cost".to_string(),
            weight: 0.4,
            description: Some("Financial impact".to_string()),
        },
        StoredCriterion {
            name: "Quality".to_string(),
            weight: 0.6,
            description: None,
        },
    ];
    let decision = Decision::new("sess-1", "Q", vec![], "weighted_sum", json!({}), json!({}))
        .with_criteria(criteria.clone());

    assert!(decision.criteria.is_some());
    assert_eq!(decision.criteria.unwrap().len(), 2);
}

#[test]
fn test_decision_with_sensitivity() {
    let sensitivity = json!({"weight_changes": [0.1, 0.2, 0.3]});
    let decision = Decision::new("sess-1", "Q", vec![], "weighted_sum", json!({}), json!({}))
        .with_sensitivity(sensitivity.clone());

    assert_eq!(decision.sensitivity_analysis, Some(sensitivity));
}

#[test]
fn test_decision_with_trade_offs() {
    let trade_offs = json!({"cost_vs_quality": "inverse relationship"});
    let decision = Decision::new("sess-1", "Q", vec![], "weighted_sum", json!({}), json!({}))
        .with_trade_offs(trade_offs.clone());

    assert_eq!(decision.trade_offs, Some(trade_offs));
}

#[test]
fn test_decision_with_constraints() {
    let constraints = json!({"budget": true, "timeline": false});
    let decision = Decision::new("sess-1", "Q", vec![], "weighted_sum", json!({}), json!({}))
        .with_constraints(constraints.clone());

    assert_eq!(decision.constraints_satisfied, Some(constraints));
}

#[test]
fn test_decision_with_metadata() {
    let metadata = json!({"source": "analysis", "version": "1.0"});
    let decision = Decision::new("sess-1", "Q", vec![], "weighted_sum", json!({}), json!({}))
        .with_metadata(metadata.clone());

    assert_eq!(decision.metadata, Some(metadata));
}

#[test]
fn test_decision_builder_chain() {
    let decision = Decision::new("sess-1", "Q", vec![], "weighted_sum", json!({}), json!({}))
        .with_criteria(vec![])
        .with_sensitivity(json!({}))
        .with_trade_offs(json!({}))
        .with_constraints(json!({}))
        .with_metadata(json!({}));

    assert!(decision.criteria.is_some());
    assert!(decision.sensitivity_analysis.is_some());
    assert!(decision.trade_offs.is_some());
    assert!(decision.constraints_satisfied.is_some());
    assert!(decision.metadata.is_some());
}

#[test]
fn test_stored_criterion() {
    let criterion = StoredCriterion {
        name: "Performance".to_string(),
        weight: 0.75,
        description: Some("Speed and efficiency".to_string()),
    };

    assert_eq!(criterion.name, "Performance");
    assert!((criterion.weight - 0.75).abs() < 0.001);
    assert_eq!(
        criterion.description,
        Some("Speed and efficiency".to_string())
    );
}

// ============================================================================
// PerspectiveAnalysis tests
// ============================================================================

#[test]
fn test_perspective_analysis_new() {
    let stakeholders = json!([{"name": "User", "interest": "high"}]);
    let synthesis = json!({"overview": "Balanced view"});
    let analysis = PerspectiveAnalysis::new(
        "sess-1",
        "Project decision",
        stakeholders.clone(),
        synthesis.clone(),
        0.85,
    );

    assert!(!analysis.id.is_empty());
    assert_eq!(analysis.session_id, "sess-1");
    assert_eq!(analysis.topic, "Project decision");
    assert_eq!(analysis.stakeholders, stakeholders);
    assert!(analysis.power_matrix.is_none());
    assert!(analysis.conflicts.is_none());
    assert!(analysis.alignments.is_none());
    assert_eq!(analysis.synthesis, synthesis);
    assert!((analysis.confidence - 0.85).abs() < 0.001);
    assert!(analysis.metadata.is_none());
}

#[test]
fn test_perspective_analysis_confidence_clamp() {
    let high = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 1.5);
    assert!((high.confidence - 1.0).abs() < 0.001);

    let low = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), -0.5);
    assert!((low.confidence - 0.0).abs() < 0.001);
}

#[test]
fn test_perspective_analysis_with_power_matrix() {
    let matrix = json!({"high_power_high_interest": ["Stakeholder A"]});
    let analysis = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 0.8)
        .with_power_matrix(matrix.clone());

    assert_eq!(analysis.power_matrix, Some(matrix));
}

#[test]
fn test_perspective_analysis_with_conflicts() {
    let conflicts = json!([{"parties": ["A", "B"], "issue": "Resource allocation"}]);
    let analysis = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 0.8)
        .with_conflicts(conflicts.clone());

    assert_eq!(analysis.conflicts, Some(conflicts));
}

#[test]
fn test_perspective_analysis_with_alignments() {
    let alignments = json!([{"parties": ["C", "D"], "goal": "Innovation"}]);
    let analysis = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 0.8)
        .with_alignments(alignments.clone());

    assert_eq!(analysis.alignments, Some(alignments));
}

#[test]
fn test_perspective_analysis_with_metadata() {
    let metadata = json!({"analyst": "System", "date": "2024-01-01"});
    let analysis = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 0.8)
        .with_metadata(metadata.clone());

    assert_eq!(analysis.metadata, Some(metadata));
}

#[test]
fn test_perspective_analysis_builder_chain() {
    let analysis = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 0.8)
        .with_power_matrix(json!({}))
        .with_conflicts(json!([]))
        .with_alignments(json!([]))
        .with_metadata(json!({}));

    assert!(analysis.power_matrix.is_some());
    assert!(analysis.conflicts.is_some());
    assert!(analysis.alignments.is_some());
    assert!(analysis.metadata.is_some());
}

// ============================================================================
// EvidenceAssessment tests
// ============================================================================

#[test]
fn test_evidence_assessment_new() {
    let evidence = json!([{"source": "Study A", "quality": "high"}]);
    let support = json!({"level": "strong", "score": 0.9});
    let analysis = json!([{"evidence": "Study A", "relevance": "high"}]);
    let assessment = EvidenceAssessment::new(
        "sess-1",
        "Claim X",
        evidence.clone(),
        support.clone(),
        analysis.clone(),
    );

    assert!(!assessment.id.is_empty());
    assert_eq!(assessment.session_id, "sess-1");
    assert_eq!(assessment.claim, "Claim X");
    assert_eq!(assessment.evidence, evidence);
    assert_eq!(assessment.overall_support, support);
    assert_eq!(assessment.evidence_analysis, analysis);
    assert!(assessment.chain_analysis.is_none());
    assert!(assessment.contradictions.is_none());
    assert!(assessment.gaps.is_none());
    assert!(assessment.recommendations.is_none());
    assert!(assessment.metadata.is_none());
}

#[test]
fn test_evidence_assessment_with_chain_analysis() {
    let chain = json!({"reasoning_steps": ["A -> B", "B -> C"]});
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]))
        .with_chain_analysis(chain.clone());

    assert_eq!(assessment.chain_analysis, Some(chain));
}

#[test]
fn test_evidence_assessment_with_contradictions() {
    let contradictions = json!([{"evidence_a": "X", "evidence_b": "Y"}]);
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]))
        .with_contradictions(contradictions.clone());

    assert_eq!(assessment.contradictions, Some(contradictions));
}

#[test]
fn test_evidence_assessment_with_gaps() {
    let gaps = json!([{"missing": "Control group data"}]);
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]))
        .with_gaps(gaps.clone());

    assert_eq!(assessment.gaps, Some(gaps));
}

#[test]
fn test_evidence_assessment_with_recommendations() {
    let recommendations = json!([{"action": "Gather more data"}]);
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]))
        .with_recommendations(recommendations.clone());

    assert_eq!(assessment.recommendations, Some(recommendations));
}

#[test]
fn test_evidence_assessment_with_metadata() {
    let metadata = json!({"reviewer": "Expert", "confidence": 0.9});
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]))
        .with_metadata(metadata.clone());

    assert_eq!(assessment.metadata, Some(metadata));
}

#[test]
fn test_evidence_assessment_builder_chain() {
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]))
        .with_chain_analysis(json!({}))
        .with_contradictions(json!([]))
        .with_gaps(json!([]))
        .with_recommendations(json!([]))
        .with_metadata(json!({}));

    assert!(assessment.chain_analysis.is_some());
    assert!(assessment.contradictions.is_some());
    assert!(assessment.gaps.is_some());
    assert!(assessment.recommendations.is_some());
    assert!(assessment.metadata.is_some());
}

// ============================================================================
// ProbabilityUpdate tests
// ============================================================================

#[test]
fn test_probability_update_new() {
    let steps = json!([{"step": 1, "prior": 0.5, "likelihood": 0.8}]);
    let interpretation = json!({"conclusion": "Evidence supports hypothesis"});
    let update = ProbabilityUpdate::new(
        "sess-1",
        "Hypothesis A",
        0.5,
        0.75,
        steps.clone(),
        interpretation.clone(),
    );

    assert!(!update.id.is_empty());
    assert_eq!(update.session_id, "sess-1");
    assert_eq!(update.hypothesis, "Hypothesis A");
    assert!((update.prior - 0.5).abs() < 0.001);
    assert!((update.posterior - 0.75).abs() < 0.001);
    assert!(update.confidence_lower.is_none());
    assert!(update.confidence_upper.is_none());
    assert!(update.confidence_level.is_none());
    assert_eq!(update.update_steps, steps);
    assert!(update.uncertainty_analysis.is_none());
    assert!(update.sensitivity.is_none());
    assert_eq!(update.interpretation, interpretation);
    assert!(update.metadata.is_none());
}

#[test]
fn test_probability_update_clamp_probabilities() {
    let high_prior = ProbabilityUpdate::new("sess-1", "H", 1.5, 0.8, json!([]), json!({}));
    assert!((high_prior.prior - 1.0).abs() < 0.001);

    let low_prior = ProbabilityUpdate::new("sess-1", "H", -0.5, 0.8, json!([]), json!({}));
    assert!((low_prior.prior - 0.0).abs() < 0.001);

    let high_posterior = ProbabilityUpdate::new("sess-1", "H", 0.5, 1.5, json!([]), json!({}));
    assert!((high_posterior.posterior - 1.0).abs() < 0.001);

    let low_posterior = ProbabilityUpdate::new("sess-1", "H", 0.5, -0.5, json!([]), json!({}));
    assert!((low_posterior.posterior - 0.0).abs() < 0.001);
}

#[test]
fn test_probability_update_with_confidence_interval() {
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}))
        .with_confidence_interval(Some(0.65), Some(0.85), Some(0.95));

    assert_eq!(update.confidence_lower, Some(0.65));
    assert_eq!(update.confidence_upper, Some(0.85));
    assert_eq!(update.confidence_level, Some(0.95));
}

#[test]
fn test_probability_update_with_confidence_interval_clamp() {
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}))
        .with_confidence_interval(Some(-0.1), Some(1.5), Some(1.2));

    assert_eq!(update.confidence_lower, Some(0.0));
    assert_eq!(update.confidence_upper, Some(1.0));
    assert_eq!(update.confidence_level, Some(1.0));
}

#[test]
fn test_probability_update_with_uncertainty() {
    let uncertainty = json!({"sources": ["measurement", "model"]});
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}))
        .with_uncertainty(uncertainty.clone());

    assert_eq!(update.uncertainty_analysis, Some(uncertainty));
}

#[test]
fn test_probability_update_with_sensitivity() {
    let sensitivity = json!({"prior_sensitivity": 0.1});
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}))
        .with_sensitivity(sensitivity.clone());

    assert_eq!(update.sensitivity, Some(sensitivity));
}

#[test]
fn test_probability_update_with_metadata() {
    let metadata = json!({"method": "bayesian", "iterations": 1000});
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}))
        .with_metadata(metadata.clone());

    assert_eq!(update.metadata, Some(metadata));
}

#[test]
fn test_probability_update_builder_chain() {
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}))
        .with_confidence_interval(Some(0.6), Some(0.9), Some(0.95))
        .with_uncertainty(json!({}))
        .with_sensitivity(json!({}))
        .with_metadata(json!({}));

    assert!(update.confidence_lower.is_some());
    assert!(update.confidence_upper.is_some());
    assert!(update.confidence_level.is_some());
    assert!(update.uncertainty_analysis.is_some());
    assert!(update.sensitivity.is_some());
    assert!(update.metadata.is_some());
}

// ============================================================================
// CrossRefType default test
// ============================================================================

#[test]
fn test_cross_ref_type_default() {
    assert_eq!(CrossRefType::default(), CrossRefType::Supports);
}

// ============================================================================
// Serialization round-trip tests
// ============================================================================

#[test]
fn test_session_serialization_roundtrip() {
    let session = Session::new("linear").with_active_branch("branch-123");

    let json = serde_json::to_string(&session).unwrap();
    let deserialized: Session = serde_json::from_str(&json).unwrap();

    assert_eq!(session.id, deserialized.id);
    assert_eq!(session.mode, deserialized.mode);
    assert_eq!(session.active_branch_id, deserialized.active_branch_id);
}

#[test]
fn test_thought_serialization_roundtrip() {
    let thought = Thought::new("sess-1", "Test content", "linear")
        .with_confidence(0.95)
        .with_parent("parent-1")
        .with_branch("branch-1")
        .with_metadata(json!({"key": "value"}));

    let json = serde_json::to_string(&thought).unwrap();
    let deserialized: Thought = serde_json::from_str(&json).unwrap();

    assert_eq!(thought.id, deserialized.id);
    assert_eq!(thought.content, deserialized.content);
    assert_eq!(thought.confidence, deserialized.confidence);
    assert_eq!(thought.parent_id, deserialized.parent_id);
    assert_eq!(thought.branch_id, deserialized.branch_id);
}

#[test]
fn test_branch_serialization_roundtrip() {
    let branch = Branch::new("sess-1")
        .with_name("Test Branch")
        .with_state(BranchState::Completed);

    let json = serde_json::to_string(&branch).unwrap();
    let deserialized: Branch = serde_json::from_str(&json).unwrap();

    assert_eq!(branch.id, deserialized.id);
    assert_eq!(branch.name, deserialized.name);
    assert_eq!(branch.state, deserialized.state);
}

#[test]
fn test_graph_node_serialization_roundtrip() {
    let node = GraphNode::new("sess-1", "Test")
        .with_type(NodeType::Hypothesis)
        .with_score(0.9)
        .as_terminal();

    let json = serde_json::to_string(&node).unwrap();
    let deserialized: GraphNode = serde_json::from_str(&json).unwrap();

    assert_eq!(node.id, deserialized.id);
    assert_eq!(node.node_type, deserialized.node_type);
    assert_eq!(node.score, deserialized.score);
    assert_eq!(node.is_terminal, deserialized.is_terminal);
}

#[test]
fn test_graph_edge_serialization_roundtrip() {
    let edge = GraphEdge::new("sess-1", "node-1", "node-2")
        .with_type(EdgeType::Refines)
        .with_weight(0.8);

    let json = serde_json::to_string(&edge).unwrap();
    let deserialized: GraphEdge = serde_json::from_str(&json).unwrap();

    assert_eq!(edge.id, deserialized.id);
    assert_eq!(edge.edge_type, deserialized.edge_type);
    assert_eq!(edge.weight, deserialized.weight);
}

#[test]
fn test_detection_serialization_roundtrip() {
    let detection = Detection::new(
        DetectionType::Bias,
        "confirmation_bias",
        3,
        0.85,
        "Explanation",
    )
    .with_session("sess-1")
    .with_remediation("Fix it");

    let json = serde_json::to_string(&detection).unwrap();
    let deserialized: Detection = serde_json::from_str(&json).unwrap();

    assert_eq!(detection.id, deserialized.id);
    assert_eq!(detection.detection_type, deserialized.detection_type);
    assert_eq!(detection.severity, deserialized.severity);
    assert_eq!(detection.remediation, deserialized.remediation);
}

// ============================================================================
// Enum serialization tests
// ============================================================================

#[test]
fn test_branch_state_serialization() {
    let states = vec![
        BranchState::Active,
        BranchState::Completed,
        BranchState::Abandoned,
    ];
    for state in states {
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: BranchState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }
}

#[test]
fn test_cross_ref_type_serialization() {
    let types = vec![
        CrossRefType::Supports,
        CrossRefType::Contradicts,
        CrossRefType::Extends,
        CrossRefType::Alternative,
        CrossRefType::Depends,
    ];
    for ref_type in types {
        let json = serde_json::to_string(&ref_type).unwrap();
        let deserialized: CrossRefType = serde_json::from_str(&json).unwrap();
        assert_eq!(ref_type, deserialized);
    }
}

#[test]
fn test_node_type_serialization() {
    let types = vec![
        NodeType::Thought,
        NodeType::Hypothesis,
        NodeType::Conclusion,
        NodeType::Aggregation,
        NodeType::Root,
        NodeType::Refinement,
        NodeType::Terminal,
    ];
    for node_type in types {
        let json = serde_json::to_string(&node_type).unwrap();
        let deserialized: NodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(node_type, deserialized);
    }
}

#[test]
fn test_edge_type_serialization() {
    let types = vec![
        EdgeType::Generates,
        EdgeType::Refines,
        EdgeType::Aggregates,
        EdgeType::Supports,
        EdgeType::Contradicts,
    ];
    for edge_type in types {
        let json = serde_json::to_string(&edge_type).unwrap();
        let deserialized: EdgeType = serde_json::from_str(&json).unwrap();
        assert_eq!(edge_type, deserialized);
    }
}

#[test]
fn test_snapshot_type_serialization() {
    let types = vec![
        SnapshotType::Full,
        SnapshotType::Incremental,
        SnapshotType::Branch,
    ];
    for snapshot_type in types {
        let json = serde_json::to_string(&snapshot_type).unwrap();
        let deserialized: SnapshotType = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot_type, deserialized);
    }
}

#[test]
fn test_detection_type_serialization() {
    let types = vec![DetectionType::Bias, DetectionType::Fallacy];
    for detection_type in types {
        let json = serde_json::to_string(&detection_type).unwrap();
        let deserialized: DetectionType = serde_json::from_str(&json).unwrap();
        assert_eq!(detection_type, deserialized);
    }
}

// ============================================================================
// Clone tests
// ============================================================================

#[test]
fn test_session_clone() {
    let session = Session::new("linear").with_active_branch("branch-1");
    let cloned = session.clone();
    assert_eq!(session.id, cloned.id);
    assert_eq!(session.mode, cloned.mode);
    assert_eq!(session.active_branch_id, cloned.active_branch_id);
}

#[test]
fn test_thought_clone() {
    let thought = Thought::new("sess-1", "Content", "linear").with_confidence(0.9);
    let cloned = thought.clone();
    assert_eq!(thought.id, cloned.id);
    assert_eq!(thought.content, cloned.content);
    assert_eq!(thought.confidence, cloned.confidence);
}

#[test]
fn test_branch_clone() {
    let branch = Branch::new("sess-1")
        .with_name("Branch")
        .with_state(BranchState::Completed);
    let cloned = branch.clone();
    assert_eq!(branch.id, cloned.id);
    assert_eq!(branch.name, cloned.name);
    assert_eq!(branch.state, cloned.state);
}

#[test]
fn test_cross_ref_clone() {
    let cross_ref = CrossRef::new("b1", "b2", CrossRefType::Supports).with_strength(0.8);
    let cloned = cross_ref.clone();
    assert_eq!(cross_ref.id, cloned.id);
    assert_eq!(cross_ref.ref_type, cloned.ref_type);
    assert_eq!(cross_ref.strength, cloned.strength);
}

#[test]
fn test_checkpoint_clone() {
    let checkpoint = Checkpoint::new("sess-1", "CP", json!({})).with_description("Desc");
    let cloned = checkpoint.clone();
    assert_eq!(checkpoint.id, cloned.id);
    assert_eq!(checkpoint.name, cloned.name);
    assert_eq!(checkpoint.description, cloned.description);
}

#[test]
fn test_graph_node_clone() {
    let node = GraphNode::new("sess-1", "Node")
        .with_type(NodeType::Hypothesis)
        .as_terminal();
    let cloned = node.clone();
    assert_eq!(node.id, cloned.id);
    assert_eq!(node.node_type, cloned.node_type);
    assert_eq!(node.is_terminal, cloned.is_terminal);
}

#[test]
fn test_graph_edge_clone() {
    let edge = GraphEdge::new("sess-1", "n1", "n2").with_type(EdgeType::Refines);
    let cloned = edge.clone();
    assert_eq!(edge.id, cloned.id);
    assert_eq!(edge.edge_type, cloned.edge_type);
}

#[test]
fn test_state_snapshot_clone() {
    let snapshot = StateSnapshot::new("sess-1", json!({})).with_type(SnapshotType::Incremental);
    let cloned = snapshot.clone();
    assert_eq!(snapshot.id, cloned.id);
    assert_eq!(snapshot.snapshot_type, cloned.snapshot_type);
}

#[test]
fn test_detection_clone() {
    let detection = Detection::new(DetectionType::Bias, "issue", 3, 0.8, "Explanation");
    let cloned = detection.clone();
    assert_eq!(detection.id, cloned.id);
    assert_eq!(detection.detection_type, cloned.detection_type);
    assert_eq!(detection.severity, cloned.severity);
}

#[test]
fn test_invocation_clone() {
    let invocation = Invocation::new("tool", json!({})).with_session("sess-1");
    let cloned = invocation.clone();
    assert_eq!(invocation.id, cloned.id);
    assert_eq!(invocation.session_id, cloned.session_id);
}

#[test]
fn test_decision_clone() {
    let decision = Decision::new("sess-1", "Q", vec![], "method", json!({}), json!({}));
    let cloned = decision.clone();
    assert_eq!(decision.id, cloned.id);
    assert_eq!(decision.question, cloned.question);
}

#[test]
fn test_perspective_analysis_clone() {
    let analysis = PerspectiveAnalysis::new("sess-1", "Topic", json!([]), json!({}), 0.8);
    let cloned = analysis.clone();
    assert_eq!(analysis.id, cloned.id);
    assert_eq!(analysis.topic, cloned.topic);
}

#[test]
fn test_evidence_assessment_clone() {
    let assessment = EvidenceAssessment::new("sess-1", "Claim", json!([]), json!({}), json!([]));
    let cloned = assessment.clone();
    assert_eq!(assessment.id, cloned.id);
    assert_eq!(assessment.claim, cloned.claim);
}

#[test]
fn test_probability_update_clone() {
    let update = ProbabilityUpdate::new("sess-1", "H", 0.5, 0.75, json!([]), json!({}));
    let cloned = update.clone();
    assert_eq!(update.id, cloned.id);
    assert_eq!(update.hypothesis, cloned.hypothesis);
    assert_eq!(update.prior, cloned.prior);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_string_handling() {
    let session = Session::new("");
    assert_eq!(session.mode, "");

    let thought = Thought::new("", "", "");
    assert_eq!(thought.session_id, "");
    assert_eq!(thought.content, "");
}

#[test]
fn test_session_without_metadata() {
    let session = Session::new("linear");
    let json = serde_json::to_value(&session).unwrap();
    assert!(json.get("metadata").is_none() || json["metadata"].is_null());
}

#[test]
fn test_thought_without_optional_fields() {
    let thought = Thought::new("sess-1", "Content", "linear");
    let json = serde_json::to_value(&thought).unwrap();
    assert!(json.get("parent_id").is_none() || json["parent_id"].is_null());
    assert!(json.get("branch_id").is_none() || json["branch_id"].is_null());
}

#[test]
fn test_branch_state_copy() {
    let state = BranchState::Active;
    let copied = state;
    assert_eq!(state, copied);
}

#[test]
fn test_cross_ref_type_copy() {
    let ref_type = CrossRefType::Supports;
    let copied = ref_type;
    assert_eq!(ref_type, copied);
}

#[test]
fn test_node_type_copy() {
    let node_type = NodeType::Thought;
    let copied = node_type;
    assert_eq!(node_type, copied);
}

#[test]
fn test_edge_type_copy() {
    let edge_type = EdgeType::Generates;
    let copied = edge_type;
    assert_eq!(edge_type, copied);
}

#[test]
fn test_snapshot_type_copy() {
    let snapshot_type = SnapshotType::Full;
    let copied = snapshot_type;
    assert_eq!(snapshot_type, copied);
}

#[test]
fn test_detection_type_copy() {
    let detection_type = DetectionType::Bias;
    let copied = detection_type;
    assert_eq!(detection_type, copied);
}
