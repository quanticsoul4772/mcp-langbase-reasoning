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
