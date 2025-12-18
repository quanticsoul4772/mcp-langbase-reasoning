//! Storage layer for reasoning session persistence.
//!
//! This module provides SQLite-based storage for sessions, thoughts, branches,
//! checkpoints, graph nodes, and other reasoning artifacts.

mod sqlite;

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;

pub use sqlite::SqliteStorage;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::StorageResult;

/// A reasoning session context that groups related thoughts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// Reasoning mode (e.g., "linear", "tree", "got").
    pub mode: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional metadata for the session.
    pub metadata: Option<serde_json::Value>,
    /// Active branch for tree mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_branch_id: Option<String>,
}

/// A single reasoning step or thought within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    /// Unique thought identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// The thought content/text.
    pub content: String,
    /// Confidence score (0.0-1.0).
    pub confidence: f64,
    /// Reasoning mode that generated this thought.
    pub mode: String,
    /// Parent thought ID for chained reasoning.
    pub parent_id: Option<String>,
    /// Branch ID for tree mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    /// When the thought was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

/// A reasoning branch in tree mode, representing an exploration path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Unique branch identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Optional human-readable name.
    pub name: Option<String>,
    /// Parent branch ID for nested branches.
    pub parent_branch_id: Option<String>,
    /// Priority score for branch selection.
    pub priority: f64,
    /// Confidence score for this branch.
    pub confidence: f64,
    /// Current state of the branch.
    pub state: BranchState,
    /// When the branch was created.
    pub created_at: DateTime<Utc>,
    /// When the branch was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

/// State of a reasoning branch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchState {
    /// Branch is actively being explored.
    #[default]
    Active,
    /// Branch has been completed successfully.
    Completed,
    /// Branch has been abandoned.
    Abandoned,
}

impl std::fmt::Display for BranchState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BranchState::Active => write!(f, "active"),
            BranchState::Completed => write!(f, "completed"),
            BranchState::Abandoned => write!(f, "abandoned"),
        }
    }
}

impl std::str::FromStr for BranchState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(BranchState::Active),
            "completed" => Ok(BranchState::Completed),
            "abandoned" => Ok(BranchState::Abandoned),
            _ => Err(format!("Unknown branch state: {}", s)),
        }
    }
}

/// Cross-reference between branches for linking related reasoning paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossRef {
    /// Unique cross-reference identifier.
    pub id: String,
    /// Source branch ID.
    pub from_branch_id: String,
    /// Target branch ID.
    pub to_branch_id: String,
    /// Type of relationship between branches.
    pub ref_type: CrossRefType,
    /// Optional explanation for the cross-reference.
    pub reason: Option<String>,
    /// Strength of the relationship (0.0-1.0).
    pub strength: f64,
    /// When the cross-reference was created.
    pub created_at: DateTime<Utc>,
}

/// Type of cross-reference between branches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossRefType {
    /// This branch supports the target branch's conclusions.
    #[default]
    Supports,
    /// This branch contradicts the target branch's conclusions.
    Contradicts,
    /// This branch extends or builds upon the target branch.
    Extends,
    /// This branch offers an alternative approach to the target.
    Alternative,
    /// This branch depends on the target branch's conclusions.
    Depends,
}

impl std::fmt::Display for CrossRefType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrossRefType::Supports => write!(f, "supports"),
            CrossRefType::Contradicts => write!(f, "contradicts"),
            CrossRefType::Extends => write!(f, "extends"),
            CrossRefType::Alternative => write!(f, "alternative"),
            CrossRefType::Depends => write!(f, "depends"),
        }
    }
}

impl std::str::FromStr for CrossRefType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "supports" => Ok(CrossRefType::Supports),
            "contradicts" => Ok(CrossRefType::Contradicts),
            "extends" => Ok(CrossRefType::Extends),
            "alternative" => Ok(CrossRefType::Alternative),
            "depends" => Ok(CrossRefType::Depends),
            _ => Err(format!("Unknown cross-ref type: {}", s)),
        }
    }
}

/// Checkpoint for state snapshots enabling backtracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Optional branch ID for branch-specific checkpoints.
    pub branch_id: Option<String>,
    /// Human-readable checkpoint name.
    pub name: String,
    /// Optional description of the checkpoint state.
    pub description: Option<String>,
    /// Serialized state snapshot data.
    pub snapshot: serde_json::Value,
    /// When the checkpoint was created.
    pub created_at: DateTime<Utc>,
}

/// Graph node for Graph-of-Thoughts reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique node identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Node content/thought text.
    pub content: String,
    /// Type of node in the reasoning graph.
    pub node_type: NodeType,
    /// Quality score from evaluation (0.0-1.0).
    pub score: Option<f64>,
    /// Depth level in the graph (0 = root).
    pub depth: i32,
    /// Whether this is a terminal/conclusion node.
    pub is_terminal: bool,
    /// Whether this is a root/starting node.
    pub is_root: bool,
    /// Whether this node is active (not pruned).
    pub is_active: bool,
    /// When the node was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Type of graph node in Graph-of-Thoughts reasoning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// A standard reasoning thought.
    #[default]
    Thought,
    /// A hypothesis to be tested.
    Hypothesis,
    /// A conclusion drawn from reasoning.
    Conclusion,
    /// An aggregation of multiple nodes.
    Aggregation,
    /// The root/starting node.
    Root,
    /// A refinement of a previous node.
    Refinement,
    /// A terminal/final node.
    Terminal,
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Thought => write!(f, "thought"),
            NodeType::Hypothesis => write!(f, "hypothesis"),
            NodeType::Conclusion => write!(f, "conclusion"),
            NodeType::Aggregation => write!(f, "aggregation"),
            NodeType::Root => write!(f, "root"),
            NodeType::Refinement => write!(f, "refinement"),
            NodeType::Terminal => write!(f, "terminal"),
        }
    }
}

impl std::str::FromStr for NodeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "thought" => Ok(NodeType::Thought),
            "hypothesis" => Ok(NodeType::Hypothesis),
            "conclusion" => Ok(NodeType::Conclusion),
            "aggregation" => Ok(NodeType::Aggregation),
            "root" => Ok(NodeType::Root),
            "refinement" => Ok(NodeType::Refinement),
            "terminal" => Ok(NodeType::Terminal),
            _ => Err(format!("Unknown node type: {}", s)),
        }
    }
}

/// Graph edge for Graph-of-Thoughts connections between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Unique edge identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Source node ID.
    pub from_node: String,
    /// Target node ID.
    pub to_node: String,
    /// Type of relationship between nodes.
    pub edge_type: EdgeType,
    /// Edge weight/strength (0.0-1.0).
    pub weight: f64,
    /// When the edge was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Type of graph edge in Graph-of-Thoughts reasoning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Source node generates target node.
    #[default]
    Generates,
    /// Source node refines/improves target node.
    Refines,
    /// Source node aggregates target node.
    Aggregates,
    /// Source node supports target node's conclusion.
    Supports,
    /// Source node contradicts target node's conclusion.
    Contradicts,
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeType::Generates => write!(f, "generates"),
            EdgeType::Refines => write!(f, "refines"),
            EdgeType::Aggregates => write!(f, "aggregates"),
            EdgeType::Supports => write!(f, "supports"),
            EdgeType::Contradicts => write!(f, "contradicts"),
        }
    }
}

impl std::str::FromStr for EdgeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "generates" => Ok(EdgeType::Generates),
            "refines" => Ok(EdgeType::Refines),
            "aggregates" => Ok(EdgeType::Aggregates),
            "supports" => Ok(EdgeType::Supports),
            "contradicts" => Ok(EdgeType::Contradicts),
            _ => Err(format!("Unknown edge type: {}", s)),
        }
    }
}

/// State snapshot for backtracking and state restoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Unique snapshot identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Type of snapshot (full, incremental, branch).
    pub snapshot_type: SnapshotType,
    /// Serialized state data.
    pub state_data: serde_json::Value,
    /// Parent snapshot ID for incremental snapshots.
    pub parent_snapshot_id: Option<String>,
    /// When the snapshot was created.
    pub created_at: DateTime<Utc>,
    /// Optional description of the snapshot state.
    pub description: Option<String>,
}

/// Type of state snapshot for backtracking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotType {
    /// Complete state snapshot.
    #[default]
    Full,
    /// Incremental changes since last snapshot.
    Incremental,
    /// Branch-specific snapshot.
    Branch,
}

impl std::fmt::Display for SnapshotType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotType::Full => write!(f, "full"),
            SnapshotType::Incremental => write!(f, "incremental"),
            SnapshotType::Branch => write!(f, "branch"),
        }
    }
}

impl std::str::FromStr for SnapshotType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "full" => Ok(SnapshotType::Full),
            "incremental" => Ok(SnapshotType::Incremental),
            "branch" => Ok(SnapshotType::Branch),
            _ => Err(format!("Unknown snapshot type: {}", s)),
        }
    }
}

/// Detection type for bias and fallacy analysis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionType {
    /// Cognitive bias detection.
    #[default]
    Bias,
    /// Logical fallacy detection.
    Fallacy,
}

impl std::fmt::Display for DetectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectionType::Bias => write!(f, "bias"),
            DetectionType::Fallacy => write!(f, "fallacy"),
        }
    }
}

impl std::str::FromStr for DetectionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bias" => Ok(DetectionType::Bias),
            "fallacy" => Ok(DetectionType::Fallacy),
            _ => Err(format!("Unknown detection type: {}", s)),
        }
    }
}

/// Detection result from bias or fallacy analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Unique detection identifier.
    pub id: String,
    /// Optional parent session ID.
    pub session_id: Option<String>,
    /// Optional thought ID being analyzed.
    pub thought_id: Option<String>,
    /// Type of detection (bias or fallacy).
    pub detection_type: DetectionType,
    /// Name of the detected issue (e.g., "confirmation_bias").
    pub detected_issue: String,
    /// Severity level (1-5, where 5 is most severe).
    pub severity: i32,
    /// Confidence in the detection (0.0-1.0).
    pub confidence: f64,
    /// Explanation of why this was detected.
    pub explanation: String,
    /// Optional remediation suggestion.
    pub remediation: Option<String>,
    /// When the detection was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

impl GraphNode {
    /// Create a new graph node
    pub fn new(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            content: content.into(),
            node_type: NodeType::Thought,
            score: None,
            depth: 0,
            is_terminal: false,
            is_root: false,
            is_active: true,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set node type
    pub fn with_type(mut self, node_type: NodeType) -> Self {
        self.node_type = node_type;
        self
    }

    /// Set score
    pub fn with_score(mut self, score: f64) -> Self {
        self.score = Some(score.clamp(0.0, 1.0));
        self
    }

    /// Set depth
    pub fn with_depth(mut self, depth: i32) -> Self {
        self.depth = depth;
        self
    }

    /// Mark as terminal
    pub fn as_terminal(mut self) -> Self {
        self.is_terminal = true;
        self
    }

    /// Mark as root
    pub fn as_root(mut self) -> Self {
        self.is_root = true;
        self
    }

    /// Mark as active
    pub fn as_active(mut self) -> Self {
        self.is_active = true;
        self
    }

    /// Mark as inactive (pruned)
    pub fn as_inactive(mut self) -> Self {
        self.is_active = false;
        self
    }
}

impl GraphEdge {
    /// Create a new graph edge
    pub fn new(
        session_id: impl Into<String>,
        from_node: impl Into<String>,
        to_node: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            from_node: from_node.into(),
            to_node: to_node.into(),
            edge_type: EdgeType::Generates,
            weight: 1.0,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set edge type
    pub fn with_type(mut self, edge_type: EdgeType) -> Self {
        self.edge_type = edge_type;
        self
    }

    /// Set weight
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight.clamp(0.0, 1.0);
        self
    }
}

impl StateSnapshot {
    /// Create a new state snapshot
    pub fn new(session_id: impl Into<String>, state_data: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            snapshot_type: SnapshotType::Full,
            state_data,
            parent_snapshot_id: None,
            created_at: Utc::now(),
            description: None,
        }
    }

    /// Set snapshot type
    pub fn with_type(mut self, snapshot_type: SnapshotType) -> Self {
        self.snapshot_type = snapshot_type;
        self
    }

    /// Set parent snapshot
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_snapshot_id = Some(parent_id.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Invocation log entry for debugging and tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invocation {
    /// Unique invocation identifier.
    pub id: String,
    /// Optional parent session ID.
    pub session_id: Option<String>,
    /// Name of the MCP tool invoked.
    pub tool_name: String,
    /// Input parameters as JSON.
    pub input: serde_json::Value,
    /// Output result as JSON (if successful).
    pub output: Option<serde_json::Value>,
    /// Name of the Langbase pipe called.
    pub pipe_name: Option<String>,
    /// Latency in milliseconds.
    pub latency_ms: Option<i64>,
    /// Whether the invocation succeeded.
    pub success: bool,
    /// Error message (if failed).
    pub error: Option<String>,
    /// When the invocation occurred.
    pub created_at: DateTime<Utc>,
}

impl Session {
    /// Create a new session with the given mode
    pub fn new(mode: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            mode: mode.into(),
            created_at: now,
            updated_at: now,
            metadata: None,
            active_branch_id: None,
        }
    }

    /// Set the active branch
    pub fn with_active_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.active_branch_id = Some(branch_id.into());
        self
    }
}

impl Thought {
    /// Create a new thought in a session
    pub fn new(
        session_id: impl Into<String>,
        content: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            content: content.into(),
            confidence: 0.8,
            mode: mode.into(),
            parent_id: None,
            branch_id: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set the confidence level
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the parent thought
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set the branch ID for tree mode
    pub fn with_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = Some(branch_id.into());
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl Branch {
    /// Create a new branch in a session
    pub fn new(session_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            name: None,
            parent_branch_id: None,
            priority: 1.0,
            confidence: 0.8,
            state: BranchState::Active,
            created_at: now,
            updated_at: now,
            metadata: None,
        }
    }

    /// Set the branch name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the parent branch
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_branch_id = Some(parent_id.into());
        self
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: f64) -> Self {
        self.priority = priority;
        self
    }

    /// Set the confidence
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the state
    pub fn with_state(mut self, state: BranchState) -> Self {
        self.state = state;
        self
    }
}

impl CrossRef {
    /// Create a new cross-reference between branches
    pub fn new(
        from_branch_id: impl Into<String>,
        to_branch_id: impl Into<String>,
        ref_type: CrossRefType,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            from_branch_id: from_branch_id.into(),
            to_branch_id: to_branch_id.into(),
            ref_type,
            reason: None,
            strength: 1.0,
            created_at: Utc::now(),
        }
    }

    /// Set the reason for the cross-reference
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the strength of the cross-reference
    pub fn with_strength(mut self, strength: f64) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(
        session_id: impl Into<String>,
        name: impl Into<String>,
        snapshot: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            branch_id: None,
            name: name.into(),
            description: None,
            snapshot,
            created_at: Utc::now(),
        }
    }

    /// Set the branch ID
    pub fn with_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = Some(branch_id.into());
        self
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Invocation {
    /// Create a new invocation log entry
    pub fn new(tool_name: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: None,
            tool_name: tool_name.into(),
            input,
            output: None,
            pipe_name: None,
            latency_ms: None,
            success: true,
            error: None,
            created_at: Utc::now(),
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the pipe name
    pub fn with_pipe(mut self, pipe_name: impl Into<String>) -> Self {
        self.pipe_name = Some(pipe_name.into());
        self
    }

    /// Mark as successful with output
    pub fn success(mut self, output: serde_json::Value, latency_ms: i64) -> Self {
        self.success = true;
        self.output = Some(output);
        self.latency_ms = Some(latency_ms);
        self
    }

    /// Mark as failed with error
    pub fn failure(mut self, error: impl Into<String>, latency_ms: i64) -> Self {
        self.success = false;
        self.error = Some(error.into());
        self.latency_ms = Some(latency_ms);
        self
    }
}

impl Detection {
    /// Create a new detection result
    pub fn new(
        detection_type: DetectionType,
        detected_issue: impl Into<String>,
        severity: i32,
        confidence: f64,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: None,
            thought_id: None,
            detection_type,
            detected_issue: detected_issue.into(),
            severity: severity.clamp(1, 5),
            confidence: confidence.clamp(0.0, 1.0),
            explanation: explanation.into(),
            remediation: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the thought ID
    pub fn with_thought(mut self, thought_id: impl Into<String>) -> Self {
        self.thought_id = Some(thought_id.into());
        self
    }

    /// Set the remediation suggestion
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// Decision Framework Storage Types
// ============================================================================

/// Stored decision analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Unique decision identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// The decision question.
    pub question: String,
    /// Available options (JSON array).
    pub options: Vec<String>,
    /// Evaluation criteria with weights (JSON array).
    pub criteria: Option<Vec<StoredCriterion>>,
    /// Decision method ('weighted_sum', 'pairwise', 'topsis').
    pub method: String,
    /// Recommendation (JSON object).
    pub recommendation: serde_json::Value,
    /// Option scores (JSON array).
    pub scores: serde_json::Value,
    /// Sensitivity analysis results.
    pub sensitivity_analysis: Option<serde_json::Value>,
    /// Trade-offs between options.
    pub trade_offs: Option<serde_json::Value>,
    /// Constraint satisfaction per option.
    pub constraints_satisfied: Option<serde_json::Value>,
    /// When the decision was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Stored criterion for decision analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCriterion {
    /// Criterion name.
    pub name: String,
    /// Weight (0.0-1.0).
    pub weight: f64,
    /// Optional description.
    pub description: Option<String>,
}

impl Decision {
    /// Create a new decision.
    pub fn new(
        session_id: impl Into<String>,
        question: impl Into<String>,
        options: Vec<String>,
        method: impl Into<String>,
        recommendation: serde_json::Value,
        scores: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            question: question.into(),
            options,
            criteria: None,
            method: method.into(),
            recommendation,
            scores,
            sensitivity_analysis: None,
            trade_offs: None,
            constraints_satisfied: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set criteria.
    pub fn with_criteria(mut self, criteria: Vec<StoredCriterion>) -> Self {
        self.criteria = Some(criteria);
        self
    }

    /// Set sensitivity analysis.
    pub fn with_sensitivity(mut self, analysis: serde_json::Value) -> Self {
        self.sensitivity_analysis = Some(analysis);
        self
    }

    /// Set trade-offs.
    pub fn with_trade_offs(mut self, trade_offs: serde_json::Value) -> Self {
        self.trade_offs = Some(trade_offs);
        self
    }

    /// Set constraints satisfied.
    pub fn with_constraints(mut self, satisfied: serde_json::Value) -> Self {
        self.constraints_satisfied = Some(satisfied);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Stored perspective analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveAnalysis {
    /// Unique analysis identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// The topic analyzed.
    pub topic: String,
    /// Stakeholder analyses (JSON array).
    pub stakeholders: serde_json::Value,
    /// Power/interest matrix (JSON object).
    pub power_matrix: Option<serde_json::Value>,
    /// Identified conflicts (JSON array).
    pub conflicts: Option<serde_json::Value>,
    /// Identified alignments (JSON array).
    pub alignments: Option<serde_json::Value>,
    /// Synthesis of perspectives (JSON object).
    pub synthesis: serde_json::Value,
    /// Overall confidence (0.0-1.0).
    pub confidence: f64,
    /// When the analysis was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

impl PerspectiveAnalysis {
    /// Create a new perspective analysis.
    pub fn new(
        session_id: impl Into<String>,
        topic: impl Into<String>,
        stakeholders: serde_json::Value,
        synthesis: serde_json::Value,
        confidence: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            topic: topic.into(),
            stakeholders,
            power_matrix: None,
            conflicts: None,
            alignments: None,
            synthesis,
            confidence: confidence.clamp(0.0, 1.0),
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set power matrix.
    pub fn with_power_matrix(mut self, matrix: serde_json::Value) -> Self {
        self.power_matrix = Some(matrix);
        self
    }

    /// Set conflicts.
    pub fn with_conflicts(mut self, conflicts: serde_json::Value) -> Self {
        self.conflicts = Some(conflicts);
        self
    }

    /// Set alignments.
    pub fn with_alignments(mut self, alignments: serde_json::Value) -> Self {
        self.alignments = Some(alignments);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// Evidence Assessment Storage Types
// ============================================================================

/// Stored evidence assessment result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAssessment {
    /// Unique assessment identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// The claim being assessed.
    pub claim: String,
    /// Evidence items (JSON array).
    pub evidence: serde_json::Value,
    /// Overall support level (JSON object).
    pub overall_support: serde_json::Value,
    /// Individual evidence analyses (JSON array).
    pub evidence_analysis: serde_json::Value,
    /// Chain of reasoning analysis (JSON object).
    pub chain_analysis: Option<serde_json::Value>,
    /// Detected contradictions (JSON array).
    pub contradictions: Option<serde_json::Value>,
    /// Identified gaps (JSON array).
    pub gaps: Option<serde_json::Value>,
    /// Recommendations (JSON array).
    pub recommendations: Option<serde_json::Value>,
    /// When the assessment was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

impl EvidenceAssessment {
    /// Create a new evidence assessment.
    pub fn new(
        session_id: impl Into<String>,
        claim: impl Into<String>,
        evidence: serde_json::Value,
        overall_support: serde_json::Value,
        evidence_analysis: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            claim: claim.into(),
            evidence,
            overall_support,
            evidence_analysis,
            chain_analysis: None,
            contradictions: None,
            gaps: None,
            recommendations: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set chain analysis.
    pub fn with_chain_analysis(mut self, analysis: serde_json::Value) -> Self {
        self.chain_analysis = Some(analysis);
        self
    }

    /// Set contradictions.
    pub fn with_contradictions(mut self, contradictions: serde_json::Value) -> Self {
        self.contradictions = Some(contradictions);
        self
    }

    /// Set gaps.
    pub fn with_gaps(mut self, gaps: serde_json::Value) -> Self {
        self.gaps = Some(gaps);
        self
    }

    /// Set recommendations.
    pub fn with_recommendations(mut self, recommendations: serde_json::Value) -> Self {
        self.recommendations = Some(recommendations);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Stored probability update result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilityUpdate {
    /// Unique update identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// The hypothesis evaluated.
    pub hypothesis: String,
    /// Prior probability (0-1).
    pub prior: f64,
    /// Posterior probability (0-1).
    pub posterior: f64,
    /// Lower bound of confidence interval.
    pub confidence_lower: Option<f64>,
    /// Upper bound of confidence interval.
    pub confidence_upper: Option<f64>,
    /// Confidence interval level (e.g., 0.95).
    pub confidence_level: Option<f64>,
    /// Bayesian update steps (JSON array).
    pub update_steps: serde_json::Value,
    /// Uncertainty analysis (JSON object).
    pub uncertainty_analysis: Option<serde_json::Value>,
    /// Sensitivity analysis (JSON object).
    pub sensitivity: Option<serde_json::Value>,
    /// Human interpretation (JSON object).
    pub interpretation: serde_json::Value,
    /// When the update was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

impl ProbabilityUpdate {
    /// Create a new probability update.
    pub fn new(
        session_id: impl Into<String>,
        hypothesis: impl Into<String>,
        prior: f64,
        posterior: f64,
        update_steps: serde_json::Value,
        interpretation: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            hypothesis: hypothesis.into(),
            prior: prior.clamp(0.0, 1.0),
            posterior: posterior.clamp(0.0, 1.0),
            confidence_lower: None,
            confidence_upper: None,
            confidence_level: None,
            update_steps,
            uncertainty_analysis: None,
            sensitivity: None,
            interpretation,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Set confidence interval.
    pub fn with_confidence_interval(
        mut self,
        lower: Option<f64>,
        upper: Option<f64>,
        level: Option<f64>,
    ) -> Self {
        self.confidence_lower = lower.map(|v| v.clamp(0.0, 1.0));
        self.confidence_upper = upper.map(|v| v.clamp(0.0, 1.0));
        self.confidence_level = level.map(|v| v.clamp(0.0, 1.0));
        self
    }

    /// Set uncertainty analysis.
    pub fn with_uncertainty(mut self, analysis: serde_json::Value) -> Self {
        self.uncertainty_analysis = Some(analysis);
        self
    }

    /// Set sensitivity analysis.
    pub fn with_sensitivity(mut self, sensitivity: serde_json::Value) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Storage trait for database operations.
///
/// This trait defines all persistence operations for reasoning sessions,
/// thoughts, branches, checkpoints, graph nodes, and other artifacts.
#[async_trait]
pub trait Storage: Send + Sync {
    // Session operations

    /// Create a new session.
    async fn create_session(&self, session: &Session) -> StorageResult<()>;
    /// Get a session by ID.
    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>>;
    /// Update an existing session.
    async fn update_session(&self, session: &Session) -> StorageResult<()>;
    /// Delete a session by ID.
    async fn delete_session(&self, id: &str) -> StorageResult<()>;

    // Thought operations

    /// Create a new thought.
    async fn create_thought(&self, thought: &Thought) -> StorageResult<()>;
    /// Get a thought by ID.
    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>>;
    /// Get all thoughts in a session.
    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>>;
    /// Get all thoughts in a branch.
    async fn get_branch_thoughts(&self, branch_id: &str) -> StorageResult<Vec<Thought>>;
    /// Get the most recent thought in a session.
    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>>;

    // Branch operations (tree mode)

    /// Create a new branch.
    async fn create_branch(&self, branch: &Branch) -> StorageResult<()>;
    /// Get a branch by ID.
    async fn get_branch(&self, id: &str) -> StorageResult<Option<Branch>>;
    /// Get all branches in a session.
    async fn get_session_branches(&self, session_id: &str) -> StorageResult<Vec<Branch>>;
    /// Get child branches of a parent branch.
    async fn get_child_branches(&self, parent_id: &str) -> StorageResult<Vec<Branch>>;
    /// Update an existing branch.
    async fn update_branch(&self, branch: &Branch) -> StorageResult<()>;
    /// Delete a branch by ID.
    async fn delete_branch(&self, id: &str) -> StorageResult<()>;

    // Cross-reference operations (tree mode)

    /// Create a new cross-reference between branches.
    async fn create_cross_ref(&self, cross_ref: &CrossRef) -> StorageResult<()>;
    /// Get cross-references originating from a branch.
    async fn get_cross_refs_from(&self, branch_id: &str) -> StorageResult<Vec<CrossRef>>;
    /// Get cross-references pointing to a branch.
    async fn get_cross_refs_to(&self, branch_id: &str) -> StorageResult<Vec<CrossRef>>;
    /// Delete a cross-reference by ID.
    async fn delete_cross_ref(&self, id: &str) -> StorageResult<()>;

    // Checkpoint operations (backtracking)

    /// Create a new checkpoint.
    async fn create_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;
    /// Get a checkpoint by ID.
    async fn get_checkpoint(&self, id: &str) -> StorageResult<Option<Checkpoint>>;
    /// Get all checkpoints in a session.
    async fn get_session_checkpoints(&self, session_id: &str) -> StorageResult<Vec<Checkpoint>>;
    /// Get all checkpoints for a branch.
    async fn get_branch_checkpoints(&self, branch_id: &str) -> StorageResult<Vec<Checkpoint>>;
    /// Delete a checkpoint by ID.
    async fn delete_checkpoint(&self, id: &str) -> StorageResult<()>;

    // Invocation logging

    /// Log a tool invocation for debugging.
    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;

    // Graph node operations (GoT mode)

    /// Create a new graph node.
    async fn create_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    /// Get a graph node by ID.
    async fn get_graph_node(&self, id: &str) -> StorageResult<Option<GraphNode>>;
    /// Get all graph nodes in a session.
    async fn get_session_graph_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    /// Get active (non-pruned) graph nodes in a session.
    async fn get_active_graph_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    /// Get root nodes in a session.
    async fn get_root_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    /// Get terminal nodes in a session.
    async fn get_terminal_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    /// Update an existing graph node.
    async fn update_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    /// Delete a graph node by ID.
    async fn delete_graph_node(&self, id: &str) -> StorageResult<()>;

    // Graph edge operations (GoT mode)

    /// Create a new graph edge.
    async fn create_graph_edge(&self, edge: &GraphEdge) -> StorageResult<()>;
    /// Get a graph edge by ID.
    async fn get_graph_edge(&self, id: &str) -> StorageResult<Option<GraphEdge>>;
    /// Get edges originating from a node.
    async fn get_edges_from(&self, node_id: &str) -> StorageResult<Vec<GraphEdge>>;
    /// Get edges pointing to a node.
    async fn get_edges_to(&self, node_id: &str) -> StorageResult<Vec<GraphEdge>>;
    /// Get all edges in a session.
    async fn get_session_edges(&self, session_id: &str) -> StorageResult<Vec<GraphEdge>>;
    /// Delete a graph edge by ID.
    async fn delete_graph_edge(&self, id: &str) -> StorageResult<()>;

    // State snapshot operations (backtracking)

    /// Create a new state snapshot.
    async fn create_snapshot(&self, snapshot: &StateSnapshot) -> StorageResult<()>;
    /// Get a state snapshot by ID.
    async fn get_snapshot(&self, id: &str) -> StorageResult<Option<StateSnapshot>>;
    /// Get all snapshots in a session.
    async fn get_session_snapshots(&self, session_id: &str) -> StorageResult<Vec<StateSnapshot>>;
    /// Get the most recent snapshot in a session.
    async fn get_latest_snapshot(&self, session_id: &str) -> StorageResult<Option<StateSnapshot>>;
    /// Delete a state snapshot by ID.
    async fn delete_snapshot(&self, id: &str) -> StorageResult<()>;

    // Detection operations (bias/fallacy analysis)

    /// Create a new detection result.
    async fn create_detection(&self, detection: &Detection) -> StorageResult<()>;
    /// Get a detection by ID.
    async fn get_detection(&self, id: &str) -> StorageResult<Option<Detection>>;
    /// Get all detections in a session.
    async fn get_session_detections(&self, session_id: &str) -> StorageResult<Vec<Detection>>;
    /// Get all detections for a thought.
    async fn get_thought_detections(&self, thought_id: &str) -> StorageResult<Vec<Detection>>;
    /// Get all detections of a specific type.
    async fn get_detections_by_type(
        &self,
        detection_type: DetectionType,
    ) -> StorageResult<Vec<Detection>>;
    /// Get detections of a specific type in a session.
    async fn get_session_detections_by_type(
        &self,
        session_id: &str,
        detection_type: DetectionType,
    ) -> StorageResult<Vec<Detection>>;
    /// Delete a detection by ID.
    async fn delete_detection(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Decision operations (decision framework)
    // ========================================================================

    /// Create a new decision analysis.
    async fn create_decision(&self, decision: &Decision) -> StorageResult<()>;

    /// Get a decision by ID.
    async fn get_decision(&self, id: &str) -> StorageResult<Option<Decision>>;

    /// Get all decisions in a session.
    async fn get_session_decisions(&self, session_id: &str) -> StorageResult<Vec<Decision>>;

    /// Get decisions by method type.
    async fn get_decisions_by_method(&self, method: &str) -> StorageResult<Vec<Decision>>;

    /// Delete a decision by ID.
    async fn delete_decision(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Perspective analysis operations (decision framework)
    // ========================================================================

    /// Create a new perspective analysis.
    async fn create_perspective(&self, analysis: &PerspectiveAnalysis) -> StorageResult<()>;

    /// Get a perspective analysis by ID.
    async fn get_perspective(&self, id: &str) -> StorageResult<Option<PerspectiveAnalysis>>;

    /// Get all perspective analyses in a session.
    async fn get_session_perspectives(
        &self,
        session_id: &str,
    ) -> StorageResult<Vec<PerspectiveAnalysis>>;

    /// Delete a perspective analysis by ID.
    async fn delete_perspective(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Evidence assessment operations (evidence mode)
    // ========================================================================

    /// Create a new evidence assessment.
    async fn create_evidence_assessment(
        &self,
        assessment: &EvidenceAssessment,
    ) -> StorageResult<()>;

    /// Get an evidence assessment by ID.
    async fn get_evidence_assessment(&self, id: &str) -> StorageResult<Option<EvidenceAssessment>>;

    /// Get all evidence assessments in a session.
    async fn get_session_evidence_assessments(
        &self,
        session_id: &str,
    ) -> StorageResult<Vec<EvidenceAssessment>>;

    /// Delete an evidence assessment by ID.
    async fn delete_evidence_assessment(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Probability update operations (evidence mode)
    // ========================================================================

    /// Create a new probability update.
    async fn create_probability_update(&self, update: &ProbabilityUpdate) -> StorageResult<()>;

    /// Get a probability update by ID.
    async fn get_probability_update(&self, id: &str) -> StorageResult<Option<ProbabilityUpdate>>;

    /// Get all probability updates in a session.
    async fn get_session_probability_updates(
        &self,
        session_id: &str,
    ) -> StorageResult<Vec<ProbabilityUpdate>>;

    /// Get probability updates for a hypothesis in a session.
    async fn get_hypothesis_updates(
        &self,
        session_id: &str,
        hypothesis: &str,
    ) -> StorageResult<Vec<ProbabilityUpdate>>;

    /// Delete a probability update by ID.
    async fn delete_probability_update(&self, id: &str) -> StorageResult<()>;
}
