//! Storage layer for reasoning session persistence.
//!
//! This module provides SQLite-based storage for sessions, thoughts, branches,
//! checkpoints, graph nodes, and other reasoning artifacts.

mod sqlite;

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;

pub use sqlite::{
    get_record_skip_count, get_timestamp_reconstruction_count, reset_record_skip_count,
    reset_timestamp_reconstruction_count, SqliteStorage,
};

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
    /// Whether a fallback was used for this invocation.
    pub fallback_used: bool,
    /// Type of fallback if used (parse_error, api_unavailable, local_calculation).
    pub fallback_type: Option<String>,
}

// ============================================================================
// Pipe Usage Metrics Types
// ============================================================================

/// Summary of pipe usage statistics.
///
/// Provides aggregated metrics for a single Langbase pipe including
/// call counts, success rates, and latency statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeUsageSummary {
    /// Name of the Langbase pipe.
    pub pipe_name: String,
    /// Total number of invocations.
    pub total_calls: u64,
    /// Number of successful calls.
    pub success_count: u64,
    /// Number of failed calls.
    pub failure_count: u64,
    /// Success rate (0.0-1.0).
    pub success_rate: f64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Minimum latency in milliseconds.
    pub min_latency_ms: Option<i64>,
    /// Maximum latency in milliseconds.
    pub max_latency_ms: Option<i64>,
    /// First invocation timestamp.
    pub first_call: DateTime<Utc>,
    /// Most recent invocation timestamp.
    pub last_call: DateTime<Utc>,
}

/// Summary of fallback usage across invocations.
///
/// Provides metrics for tracking how often fallbacks are used,
/// which is critical for measuring actual pipe reliability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackMetricsSummary {
    /// Total number of fallbacks used.
    pub total_fallbacks: u64,
    /// Breakdown by fallback type.
    pub fallbacks_by_type: std::collections::HashMap<String, u64>,
    /// Breakdown by pipe name.
    pub fallbacks_by_pipe: std::collections::HashMap<String, u64>,
    /// Total invocations analyzed.
    pub total_invocations: u64,
    /// Fallback rate (0.0-1.0).
    pub fallback_rate: f64,
    /// Recommendation based on fallback usage.
    pub recommendation: String,
    /// Number of timestamps that were reconstructed due to parse failures.
    /// This indicates data integrity issues in the database.
    pub timestamp_reconstructions: u64,
    /// Number of database records skipped due to JSON or timestamp parse failures.
    /// This indicates data loss when records fail parsing in query results.
    pub records_skipped: u64,
}

/// Filter options for metrics queries.
///
/// Allows filtering invocations by various criteria for targeted analysis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsFilter {
    /// Filter by pipe name (exact match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipe_name: Option<String>,
    /// Filter by session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Filter by tool name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Filter calls after this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<DateTime<Utc>>,
    /// Filter calls before this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<DateTime<Utc>>,
    /// Only include successful (true) or failed (false) calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_only: Option<bool>,
    /// Limit number of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl MetricsFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by pipe name.
    pub fn with_pipe(mut self, pipe_name: impl Into<String>) -> Self {
        self.pipe_name = Some(pipe_name.into());
        self
    }

    /// Filter by session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Filter by tool name.
    pub fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    /// Filter calls after this time.
    pub fn after(mut self, time: DateTime<Utc>) -> Self {
        self.after = Some(time);
        self
    }

    /// Filter calls before this time.
    pub fn before(mut self, time: DateTime<Utc>) -> Self {
        self.before = Some(time);
        self
    }

    /// Only include successful calls.
    pub fn successful_only(mut self) -> Self {
        self.success_only = Some(true);
        self
    }

    /// Only include failed calls.
    pub fn failed_only(mut self) -> Self {
        self.success_only = Some(false);
        self
    }

    /// Limit number of results.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
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
            fallback_used: false,
            fallback_type: None,
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

    /// Set latency separately
    pub fn with_latency(mut self, latency_ms: i64) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }

    /// Mark as successful (simple version without output)
    pub fn mark_success(mut self) -> Self {
        self.success = true;
        self
    }

    /// Mark as failed (simple version)
    pub fn mark_failed(mut self, error: impl Into<String>) -> Self {
        self.success = false;
        self.error = Some(error.into());
        self
    }

    /// Mark that a fallback was used
    pub fn with_fallback(mut self, fallback_type: impl Into<String>) -> Self {
        self.fallback_used = true;
        self.fallback_type = Some(fallback_type.into());
        self
    }

    /// Mark fallback with specific type constants
    pub fn with_parse_error_fallback(self) -> Self {
        self.with_fallback("parse_error")
    }

    /// Mark fallback due to API unavailability
    pub fn with_api_unavailable_fallback(self) -> Self {
        self.with_fallback("api_unavailable")
    }

    /// Mark fallback due to local calculation
    pub fn with_local_calculation_fallback(self) -> Self {
        self.with_fallback("local_calculation")
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

    /// Get an existing session or create a new one.
    ///
    /// If `session_id` is `Some`, looks up the session:
    /// - If found, returns it
    /// - If not found, creates a new session with that ID
    ///
    /// If `session_id` is `None`, creates a new session with a generated ID.
    ///
    /// This is a provided method with a default implementation that uses
    /// `get_session` and `create_session`.
    async fn get_or_create_session(
        &self,
        session_id: &Option<String>,
        mode: &str,
    ) -> StorageResult<Session>
    where
        Self: Sized,
    {
        match session_id {
            Some(id) => match self.get_session(id).await? {
                Some(session) => Ok(session),
                None => {
                    let mut new_session = Session::new(mode);
                    new_session.id = id.clone();
                    self.create_session(&new_session).await?;
                    Ok(new_session)
                }
            },
            None => {
                let session = Session::new(mode);
                self.create_session(&session).await?;
                Ok(session)
            }
        }
    }

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

    // Invocation logging and metrics

    /// Log a tool invocation for debugging.
    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;

    /// Get aggregated usage summary for all pipes.
    ///
    /// Returns metrics for each pipe that has been invoked, ordered by total calls descending.
    async fn get_pipe_usage_summary(&self) -> StorageResult<Vec<PipeUsageSummary>>;

    /// Get usage summary for a specific pipe.
    ///
    /// Returns None if the pipe has never been invoked.
    async fn get_pipe_summary(&self, pipe_name: &str) -> StorageResult<Option<PipeUsageSummary>>;

    /// Get invocations with optional filtering.
    ///
    /// Supports filtering by pipe name, session, tool, time range, and success status.
    /// Results are ordered by created_at descending (most recent first).
    async fn get_invocations(&self, filter: MetricsFilter) -> StorageResult<Vec<Invocation>>;

    /// Get total invocation count.
    ///
    /// Optionally filter by pipe name.
    async fn get_invocation_count(&self, pipe_name: Option<&str>) -> StorageResult<u64>;

    /// Get fallback usage metrics.
    ///
    /// Returns aggregated statistics about fallback usage across all invocations,
    /// including breakdown by type and pipe.
    async fn get_fallback_metrics(&self) -> StorageResult<FallbackMetricsSummary>;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ========================================================================
    // BranchState tests
    // ========================================================================

    #[test]
    fn test_branch_state_display() {
        assert_eq!(BranchState::Active.to_string(), "active");
        assert_eq!(BranchState::Completed.to_string(), "completed");
        assert_eq!(BranchState::Abandoned.to_string(), "abandoned");
    }

    #[test]
    fn test_branch_state_from_str() {
        assert_eq!(
            BranchState::from_str("active").unwrap(),
            BranchState::Active
        );
        assert_eq!(
            BranchState::from_str("completed").unwrap(),
            BranchState::Completed
        );
        assert_eq!(
            BranchState::from_str("abandoned").unwrap(),
            BranchState::Abandoned
        );
    }

    #[test]
    fn test_branch_state_from_str_case_insensitive() {
        assert_eq!(
            BranchState::from_str("ACTIVE").unwrap(),
            BranchState::Active
        );
        assert_eq!(
            BranchState::from_str("Completed").unwrap(),
            BranchState::Completed
        );
        assert_eq!(
            BranchState::from_str("ABANDONED").unwrap(),
            BranchState::Abandoned
        );
    }

    #[test]
    fn test_branch_state_from_str_invalid() {
        assert!(BranchState::from_str("invalid").is_err());
        assert!(BranchState::from_str("").is_err());
        assert_eq!(
            BranchState::from_str("unknown").unwrap_err(),
            "Unknown branch state: unknown"
        );
    }

    #[test]
    fn test_branch_state_default() {
        assert_eq!(BranchState::default(), BranchState::Active);
    }

    #[test]
    fn test_branch_state_round_trip() {
        for state in [
            BranchState::Active,
            BranchState::Completed,
            BranchState::Abandoned,
        ] {
            let str_val = state.to_string();
            let parsed = BranchState::from_str(&str_val).unwrap();
            assert_eq!(parsed, state);
        }
    }

    // ========================================================================
    // CrossRefType tests
    // ========================================================================

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
            CrossRefType::from_str("supports").unwrap(),
            CrossRefType::Supports
        );
        assert_eq!(
            CrossRefType::from_str("contradicts").unwrap(),
            CrossRefType::Contradicts
        );
        assert_eq!(
            CrossRefType::from_str("extends").unwrap(),
            CrossRefType::Extends
        );
        assert_eq!(
            CrossRefType::from_str("alternative").unwrap(),
            CrossRefType::Alternative
        );
        assert_eq!(
            CrossRefType::from_str("depends").unwrap(),
            CrossRefType::Depends
        );
    }

    #[test]
    fn test_cross_ref_type_from_str_case_insensitive() {
        assert_eq!(
            CrossRefType::from_str("SUPPORTS").unwrap(),
            CrossRefType::Supports
        );
        assert_eq!(
            CrossRefType::from_str("Contradicts").unwrap(),
            CrossRefType::Contradicts
        );
        assert_eq!(
            CrossRefType::from_str("EXTENDS").unwrap(),
            CrossRefType::Extends
        );
    }

    #[test]
    fn test_cross_ref_type_from_str_invalid() {
        assert!(CrossRefType::from_str("invalid").is_err());
        assert!(CrossRefType::from_str("").is_err());
        assert_eq!(
            CrossRefType::from_str("unknown").unwrap_err(),
            "Unknown cross-ref type: unknown"
        );
    }

    #[test]
    fn test_cross_ref_type_default() {
        assert_eq!(CrossRefType::default(), CrossRefType::Supports);
    }

    #[test]
    fn test_cross_ref_type_round_trip() {
        for ref_type in [
            CrossRefType::Supports,
            CrossRefType::Contradicts,
            CrossRefType::Extends,
            CrossRefType::Alternative,
            CrossRefType::Depends,
        ] {
            let str_val = ref_type.to_string();
            let parsed = CrossRefType::from_str(&str_val).unwrap();
            assert_eq!(parsed, ref_type);
        }
    }

    // ========================================================================
    // NodeType tests
    // ========================================================================

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
        assert_eq!(NodeType::from_str("thought").unwrap(), NodeType::Thought);
        assert_eq!(
            NodeType::from_str("hypothesis").unwrap(),
            NodeType::Hypothesis
        );
        assert_eq!(
            NodeType::from_str("conclusion").unwrap(),
            NodeType::Conclusion
        );
        assert_eq!(
            NodeType::from_str("aggregation").unwrap(),
            NodeType::Aggregation
        );
        assert_eq!(NodeType::from_str("root").unwrap(), NodeType::Root);
        assert_eq!(
            NodeType::from_str("refinement").unwrap(),
            NodeType::Refinement
        );
        assert_eq!(NodeType::from_str("terminal").unwrap(), NodeType::Terminal);
    }

    #[test]
    fn test_node_type_from_str_case_insensitive() {
        assert_eq!(NodeType::from_str("THOUGHT").unwrap(), NodeType::Thought);
        assert_eq!(
            NodeType::from_str("Hypothesis").unwrap(),
            NodeType::Hypothesis
        );
        assert_eq!(
            NodeType::from_str("CONCLUSION").unwrap(),
            NodeType::Conclusion
        );
    }

    #[test]
    fn test_node_type_from_str_invalid() {
        assert!(NodeType::from_str("invalid").is_err());
        assert!(NodeType::from_str("").is_err());
        assert_eq!(
            NodeType::from_str("unknown").unwrap_err(),
            "Unknown node type: unknown"
        );
    }

    #[test]
    fn test_node_type_default() {
        assert_eq!(NodeType::default(), NodeType::Thought);
    }

    #[test]
    fn test_node_type_round_trip() {
        for node_type in [
            NodeType::Thought,
            NodeType::Hypothesis,
            NodeType::Conclusion,
            NodeType::Aggregation,
            NodeType::Root,
            NodeType::Refinement,
            NodeType::Terminal,
        ] {
            let str_val = node_type.to_string();
            let parsed = NodeType::from_str(&str_val).unwrap();
            assert_eq!(parsed, node_type);
        }
    }

    // ========================================================================
    // EdgeType tests
    // ========================================================================

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
            EdgeType::from_str("generates").unwrap(),
            EdgeType::Generates
        );
        assert_eq!(EdgeType::from_str("refines").unwrap(), EdgeType::Refines);
        assert_eq!(
            EdgeType::from_str("aggregates").unwrap(),
            EdgeType::Aggregates
        );
        assert_eq!(EdgeType::from_str("supports").unwrap(), EdgeType::Supports);
        assert_eq!(
            EdgeType::from_str("contradicts").unwrap(),
            EdgeType::Contradicts
        );
    }

    #[test]
    fn test_edge_type_from_str_case_insensitive() {
        assert_eq!(
            EdgeType::from_str("GENERATES").unwrap(),
            EdgeType::Generates
        );
        assert_eq!(EdgeType::from_str("Refines").unwrap(), EdgeType::Refines);
        assert_eq!(
            EdgeType::from_str("AGGREGATES").unwrap(),
            EdgeType::Aggregates
        );
    }

    #[test]
    fn test_edge_type_from_str_invalid() {
        assert!(EdgeType::from_str("invalid").is_err());
        assert!(EdgeType::from_str("").is_err());
        assert_eq!(
            EdgeType::from_str("unknown").unwrap_err(),
            "Unknown edge type: unknown"
        );
    }

    #[test]
    fn test_edge_type_default() {
        assert_eq!(EdgeType::default(), EdgeType::Generates);
    }

    #[test]
    fn test_edge_type_round_trip() {
        for edge_type in [
            EdgeType::Generates,
            EdgeType::Refines,
            EdgeType::Aggregates,
            EdgeType::Supports,
            EdgeType::Contradicts,
        ] {
            let str_val = edge_type.to_string();
            let parsed = EdgeType::from_str(&str_val).unwrap();
            assert_eq!(parsed, edge_type);
        }
    }

    // ========================================================================
    // SnapshotType tests
    // ========================================================================

    #[test]
    fn test_snapshot_type_display() {
        assert_eq!(SnapshotType::Full.to_string(), "full");
        assert_eq!(SnapshotType::Incremental.to_string(), "incremental");
        assert_eq!(SnapshotType::Branch.to_string(), "branch");
    }

    #[test]
    fn test_snapshot_type_from_str() {
        assert_eq!(SnapshotType::from_str("full").unwrap(), SnapshotType::Full);
        assert_eq!(
            SnapshotType::from_str("incremental").unwrap(),
            SnapshotType::Incremental
        );
        assert_eq!(
            SnapshotType::from_str("branch").unwrap(),
            SnapshotType::Branch
        );
    }

    #[test]
    fn test_snapshot_type_from_str_case_insensitive() {
        assert_eq!(SnapshotType::from_str("FULL").unwrap(), SnapshotType::Full);
        assert_eq!(
            SnapshotType::from_str("Incremental").unwrap(),
            SnapshotType::Incremental
        );
        assert_eq!(
            SnapshotType::from_str("BRANCH").unwrap(),
            SnapshotType::Branch
        );
    }

    #[test]
    fn test_snapshot_type_from_str_invalid() {
        assert!(SnapshotType::from_str("invalid").is_err());
        assert!(SnapshotType::from_str("").is_err());
        assert_eq!(
            SnapshotType::from_str("unknown").unwrap_err(),
            "Unknown snapshot type: unknown"
        );
    }

    #[test]
    fn test_snapshot_type_default() {
        assert_eq!(SnapshotType::default(), SnapshotType::Full);
    }

    #[test]
    fn test_snapshot_type_round_trip() {
        for snapshot_type in [
            SnapshotType::Full,
            SnapshotType::Incremental,
            SnapshotType::Branch,
        ] {
            let str_val = snapshot_type.to_string();
            let parsed = SnapshotType::from_str(&str_val).unwrap();
            assert_eq!(parsed, snapshot_type);
        }
    }

    // ========================================================================
    // DetectionType tests
    // ========================================================================

    #[test]
    fn test_detection_type_display() {
        assert_eq!(DetectionType::Bias.to_string(), "bias");
        assert_eq!(DetectionType::Fallacy.to_string(), "fallacy");
    }

    #[test]
    fn test_detection_type_from_str() {
        assert_eq!(
            DetectionType::from_str("bias").unwrap(),
            DetectionType::Bias
        );
        assert_eq!(
            DetectionType::from_str("fallacy").unwrap(),
            DetectionType::Fallacy
        );
    }

    #[test]
    fn test_detection_type_from_str_case_insensitive() {
        assert_eq!(
            DetectionType::from_str("BIAS").unwrap(),
            DetectionType::Bias
        );
        assert_eq!(
            DetectionType::from_str("Fallacy").unwrap(),
            DetectionType::Fallacy
        );
    }

    #[test]
    fn test_detection_type_from_str_invalid() {
        assert!(DetectionType::from_str("invalid").is_err());
        assert!(DetectionType::from_str("").is_err());
        assert_eq!(
            DetectionType::from_str("unknown").unwrap_err(),
            "Unknown detection type: unknown"
        );
    }

    #[test]
    fn test_detection_type_default() {
        assert_eq!(DetectionType::default(), DetectionType::Bias);
    }

    #[test]
    fn test_detection_type_round_trip() {
        for detection_type in [DetectionType::Bias, DetectionType::Fallacy] {
            let str_val = detection_type.to_string();
            let parsed = DetectionType::from_str(&str_val).unwrap();
            assert_eq!(parsed, detection_type);
        }
    }

    // ========================================================================
    // Builder method tests
    // ========================================================================

    #[test]
    fn test_session_new() {
        let session = Session::new("linear");
        assert_eq!(session.mode, "linear");
        assert!(session.metadata.is_none());
        assert!(session.active_branch_id.is_none());
        assert!(!session.id.is_empty());
    }

    #[test]
    fn test_session_with_active_branch() {
        let session = Session::new("tree").with_active_branch("branch-123");
        assert_eq!(session.active_branch_id, Some("branch-123".to_string()));
    }

    #[test]
    fn test_thought_new() {
        let thought = Thought::new("session-123", "test content", "linear");
        assert_eq!(thought.session_id, "session-123");
        assert_eq!(thought.content, "test content");
        assert_eq!(thought.mode, "linear");
        assert_eq!(thought.confidence, 0.8);
        assert!(thought.parent_id.is_none());
        assert!(thought.branch_id.is_none());
        assert!(!thought.id.is_empty());
    }

    #[test]
    fn test_thought_with_confidence() {
        let thought = Thought::new("s1", "content", "linear").with_confidence(0.95);
        assert_eq!(thought.confidence, 0.95);
    }

    #[test]
    fn test_thought_with_confidence_clamping() {
        let thought1 = Thought::new("s1", "content", "linear").with_confidence(1.5);
        assert_eq!(thought1.confidence, 1.0);

        let thought2 = Thought::new("s1", "content", "linear").with_confidence(-0.5);
        assert_eq!(thought2.confidence, 0.0);
    }

    #[test]
    fn test_thought_with_parent() {
        let thought = Thought::new("s1", "content", "linear").with_parent("parent-123");
        assert_eq!(thought.parent_id, Some("parent-123".to_string()));
    }

    #[test]
    fn test_thought_with_branch() {
        let thought = Thought::new("s1", "content", "tree").with_branch("branch-123");
        assert_eq!(thought.branch_id, Some("branch-123".to_string()));
    }

    #[test]
    fn test_thought_builder_chain() {
        let thought = Thought::new("s1", "content", "tree")
            .with_confidence(0.9)
            .with_parent("p1")
            .with_branch("b1")
            .with_metadata(serde_json::json!({"key": "value"}));

        assert_eq!(thought.confidence, 0.9);
        assert_eq!(thought.parent_id, Some("p1".to_string()));
        assert_eq!(thought.branch_id, Some("b1".to_string()));
        assert!(thought.metadata.is_some());
    }

    #[test]
    fn test_branch_new() {
        let branch = Branch::new("session-123");
        assert_eq!(branch.session_id, "session-123");
        assert!(branch.name.is_none());
        assert!(branch.parent_branch_id.is_none());
        assert_eq!(branch.priority, 1.0);
        assert_eq!(branch.confidence, 0.8);
        assert_eq!(branch.state, BranchState::Active);
        assert!(!branch.id.is_empty());
    }

    #[test]
    fn test_branch_with_name() {
        let branch = Branch::new("s1").with_name("main branch");
        assert_eq!(branch.name, Some("main branch".to_string()));
    }

    #[test]
    fn test_branch_with_parent() {
        let branch = Branch::new("s1").with_parent("parent-branch");
        assert_eq!(branch.parent_branch_id, Some("parent-branch".to_string()));
    }

    #[test]
    fn test_branch_with_priority() {
        let branch = Branch::new("s1").with_priority(0.5);
        assert_eq!(branch.priority, 0.5);
    }

    #[test]
    fn test_branch_with_confidence() {
        let branch = Branch::new("s1").with_confidence(0.95);
        assert_eq!(branch.confidence, 0.95);
    }

    #[test]
    fn test_branch_with_confidence_clamping() {
        let branch1 = Branch::new("s1").with_confidence(1.5);
        assert_eq!(branch1.confidence, 1.0);

        let branch2 = Branch::new("s1").with_confidence(-0.5);
        assert_eq!(branch2.confidence, 0.0);
    }

    #[test]
    fn test_branch_with_state() {
        let branch = Branch::new("s1").with_state(BranchState::Completed);
        assert_eq!(branch.state, BranchState::Completed);
    }

    #[test]
    fn test_cross_ref_new() {
        let cross_ref = CrossRef::new("branch1", "branch2", CrossRefType::Supports);
        assert_eq!(cross_ref.from_branch_id, "branch1");
        assert_eq!(cross_ref.to_branch_id, "branch2");
        assert_eq!(cross_ref.ref_type, CrossRefType::Supports);
        assert!(cross_ref.reason.is_none());
        assert_eq!(cross_ref.strength, 1.0);
        assert!(!cross_ref.id.is_empty());
    }

    #[test]
    fn test_cross_ref_with_reason() {
        let cross_ref = CrossRef::new("b1", "b2", CrossRefType::Extends)
            .with_reason("builds on previous analysis");
        assert_eq!(
            cross_ref.reason,
            Some("builds on previous analysis".to_string())
        );
    }

    #[test]
    fn test_cross_ref_with_strength() {
        let cross_ref = CrossRef::new("b1", "b2", CrossRefType::Contradicts).with_strength(0.75);
        assert_eq!(cross_ref.strength, 0.75);
    }

    #[test]
    fn test_cross_ref_strength_clamping() {
        let cross_ref1 = CrossRef::new("b1", "b2", CrossRefType::Supports).with_strength(1.5);
        assert_eq!(cross_ref1.strength, 1.0);

        let cross_ref2 = CrossRef::new("b1", "b2", CrossRefType::Supports).with_strength(-0.5);
        assert_eq!(cross_ref2.strength, 0.0);
    }

    #[test]
    fn test_checkpoint_new() {
        let snapshot = serde_json::json!({"state": "test"});
        let checkpoint = Checkpoint::new("session-123", "checkpoint1", snapshot.clone());
        assert_eq!(checkpoint.session_id, "session-123");
        assert_eq!(checkpoint.name, "checkpoint1");
        assert_eq!(checkpoint.snapshot, snapshot);
        assert!(checkpoint.branch_id.is_none());
        assert!(checkpoint.description.is_none());
        assert!(!checkpoint.id.is_empty());
    }

    #[test]
    fn test_checkpoint_with_branch() {
        let snapshot = serde_json::json!({"state": "test"});
        let checkpoint = Checkpoint::new("s1", "cp1", snapshot).with_branch("branch-123");
        assert_eq!(checkpoint.branch_id, Some("branch-123".to_string()));
    }

    #[test]
    fn test_checkpoint_with_description() {
        let snapshot = serde_json::json!({"state": "test"});
        let checkpoint =
            Checkpoint::new("s1", "cp1", snapshot).with_description("before major decision");
        assert_eq!(
            checkpoint.description,
            Some("before major decision".to_string())
        );
    }

    #[test]
    fn test_graph_node_new() {
        let node = GraphNode::new("session-123", "node content");
        assert_eq!(node.session_id, "session-123");
        assert_eq!(node.content, "node content");
        assert_eq!(node.node_type, NodeType::Thought);
        assert!(node.score.is_none());
        assert_eq!(node.depth, 0);
        assert!(!node.is_terminal);
        assert!(!node.is_root);
        assert!(node.is_active);
        assert!(!node.id.is_empty());
    }

    #[test]
    fn test_graph_node_with_type() {
        let node = GraphNode::new("s1", "content").with_type(NodeType::Hypothesis);
        assert_eq!(node.node_type, NodeType::Hypothesis);
    }

    #[test]
    fn test_graph_node_with_score() {
        let node = GraphNode::new("s1", "content").with_score(0.85);
        assert_eq!(node.score, Some(0.85));
    }

    #[test]
    fn test_graph_node_with_score_clamping() {
        let node1 = GraphNode::new("s1", "content").with_score(1.5);
        assert_eq!(node1.score, Some(1.0));

        let node2 = GraphNode::new("s1", "content").with_score(-0.5);
        assert_eq!(node2.score, Some(0.0));
    }

    #[test]
    fn test_graph_node_with_depth() {
        let node = GraphNode::new("s1", "content").with_depth(3);
        assert_eq!(node.depth, 3);
    }

    #[test]
    fn test_graph_node_as_terminal() {
        let node = GraphNode::new("s1", "content").as_terminal();
        assert!(node.is_terminal);
    }

    #[test]
    fn test_graph_node_as_root() {
        let node = GraphNode::new("s1", "content").as_root();
        assert!(node.is_root);
    }

    #[test]
    fn test_graph_node_as_active() {
        let node = GraphNode::new("s1", "content").as_inactive().as_active();
        assert!(node.is_active);
    }

    #[test]
    fn test_graph_node_as_inactive() {
        let node = GraphNode::new("s1", "content").as_inactive();
        assert!(!node.is_active);
    }

    #[test]
    fn test_graph_edge_new() {
        let edge = GraphEdge::new("session-123", "node1", "node2");
        assert_eq!(edge.session_id, "session-123");
        assert_eq!(edge.from_node, "node1");
        assert_eq!(edge.to_node, "node2");
        assert_eq!(edge.edge_type, EdgeType::Generates);
        assert_eq!(edge.weight, 1.0);
        assert!(!edge.id.is_empty());
    }

    #[test]
    fn test_graph_edge_with_type() {
        let edge = GraphEdge::new("s1", "n1", "n2").with_type(EdgeType::Refines);
        assert_eq!(edge.edge_type, EdgeType::Refines);
    }

    #[test]
    fn test_graph_edge_with_weight() {
        let edge = GraphEdge::new("s1", "n1", "n2").with_weight(0.75);
        assert_eq!(edge.weight, 0.75);
    }

    #[test]
    fn test_graph_edge_with_weight_clamping() {
        let edge1 = GraphEdge::new("s1", "n1", "n2").with_weight(1.5);
        assert_eq!(edge1.weight, 1.0);

        let edge2 = GraphEdge::new("s1", "n1", "n2").with_weight(-0.5);
        assert_eq!(edge2.weight, 0.0);
    }

    #[test]
    fn test_state_snapshot_new() {
        let data = serde_json::json!({"key": "value"});
        let snapshot = StateSnapshot::new("session-123", data.clone());
        assert_eq!(snapshot.session_id, "session-123");
        assert_eq!(snapshot.state_data, data);
        assert_eq!(snapshot.snapshot_type, SnapshotType::Full);
        assert!(snapshot.parent_snapshot_id.is_none());
        assert!(snapshot.description.is_none());
        assert!(!snapshot.id.is_empty());
    }

    #[test]
    fn test_state_snapshot_with_type() {
        let data = serde_json::json!({"key": "value"});
        let snapshot = StateSnapshot::new("s1", data).with_type(SnapshotType::Incremental);
        assert_eq!(snapshot.snapshot_type, SnapshotType::Incremental);
    }

    #[test]
    fn test_state_snapshot_with_parent() {
        let data = serde_json::json!({"key": "value"});
        let snapshot = StateSnapshot::new("s1", data).with_parent("parent-123");
        assert_eq!(snapshot.parent_snapshot_id, Some("parent-123".to_string()));
    }

    #[test]
    fn test_state_snapshot_with_description() {
        let data = serde_json::json!({"key": "value"});
        let snapshot = StateSnapshot::new("s1", data).with_description("after step 5");
        assert_eq!(snapshot.description, Some("after step 5".to_string()));
    }

    #[test]
    fn test_invocation_new() {
        let input = serde_json::json!({"param": "value"});
        let invocation = Invocation::new("linear_reasoning", input.clone());
        assert_eq!(invocation.tool_name, "linear_reasoning");
        assert_eq!(invocation.input, input);
        assert!(invocation.session_id.is_none());
        assert!(invocation.output.is_none());
        assert!(invocation.pipe_name.is_none());
        assert!(invocation.latency_ms.is_none());
        assert!(invocation.success);
        assert!(invocation.error.is_none());
        assert!(!invocation.id.is_empty());
    }

    #[test]
    fn test_invocation_with_session() {
        let input = serde_json::json!({"param": "value"});
        let invocation = Invocation::new("tool", input).with_session("session-123");
        assert_eq!(invocation.session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_invocation_with_pipe() {
        let input = serde_json::json!({"param": "value"});
        let invocation = Invocation::new("tool", input).with_pipe("linear-v1");
        assert_eq!(invocation.pipe_name, Some("linear-v1".to_string()));
    }

    #[test]
    fn test_invocation_success() {
        let input = serde_json::json!({"param": "value"});
        let output = serde_json::json!({"result": "ok"});
        let invocation = Invocation::new("tool", input).success(output.clone(), 150);
        assert!(invocation.success);
        assert_eq!(invocation.output, Some(output));
        assert_eq!(invocation.latency_ms, Some(150));
        assert!(invocation.error.is_none());
    }

    #[test]
    fn test_invocation_failure() {
        let input = serde_json::json!({"param": "value"});
        let invocation = Invocation::new("tool", input).failure("connection timeout", 3000);
        assert!(!invocation.success);
        assert_eq!(invocation.error, Some("connection timeout".to_string()));
        assert_eq!(invocation.latency_ms, Some(3000));
        assert!(invocation.output.is_none());
    }

    #[test]
    fn test_detection_new() {
        let detection = Detection::new(
            DetectionType::Bias,
            "confirmation_bias",
            4,
            0.85,
            "Only seeking confirming evidence",
        );
        assert_eq!(detection.detection_type, DetectionType::Bias);
        assert_eq!(detection.detected_issue, "confirmation_bias");
        assert_eq!(detection.severity, 4);
        assert_eq!(detection.confidence, 0.85);
        assert_eq!(detection.explanation, "Only seeking confirming evidence");
        assert!(detection.session_id.is_none());
        assert!(detection.thought_id.is_none());
        assert!(detection.remediation.is_none());
        assert!(!detection.id.is_empty());
    }

    #[test]
    fn test_detection_severity_clamping() {
        let detection1 = Detection::new(
            DetectionType::Fallacy,
            "ad_hominem",
            10,
            0.9,
            "Attacking person not argument",
        );
        assert_eq!(detection1.severity, 5);

        let detection2 = Detection::new(
            DetectionType::Fallacy,
            "ad_hominem",
            0,
            0.9,
            "Attacking person not argument",
        );
        assert_eq!(detection2.severity, 1);
    }

    #[test]
    fn test_detection_confidence_clamping() {
        let detection1 = Detection::new(
            DetectionType::Bias,
            "anchoring",
            3,
            1.5,
            "Over-reliance on initial info",
        );
        assert_eq!(detection1.confidence, 1.0);

        let detection2 = Detection::new(
            DetectionType::Bias,
            "anchoring",
            3,
            -0.5,
            "Over-reliance on initial info",
        );
        assert_eq!(detection2.confidence, 0.0);
    }

    #[test]
    fn test_detection_with_session() {
        let detection = Detection::new(DetectionType::Bias, "sunk_cost", 3, 0.75, "explanation")
            .with_session("session-123");
        assert_eq!(detection.session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_detection_with_thought() {
        let detection = Detection::new(DetectionType::Fallacy, "strawman", 4, 0.8, "explanation")
            .with_thought("thought-123");
        assert_eq!(detection.thought_id, Some("thought-123".to_string()));
    }

    #[test]
    fn test_detection_with_remediation() {
        let detection = Detection::new(DetectionType::Bias, "availability", 2, 0.7, "explanation")
            .with_remediation("Consider base rates");
        assert_eq!(
            detection.remediation,
            Some("Consider base rates".to_string())
        );
    }

    #[test]
    fn test_detection_with_metadata() {
        let metadata = serde_json::json!({"source": "automatic"});
        let detection = Detection::new(DetectionType::Bias, "hindsight", 2, 0.65, "explanation")
            .with_metadata(metadata.clone());
        assert_eq!(detection.metadata, Some(metadata));
    }

    #[test]
    fn test_decision_new() {
        let options = vec!["option1".to_string(), "option2".to_string()];
        let recommendation = serde_json::json!({"choice": "option1"});
        let scores = serde_json::json!([{"option": "option1", "score": 0.8}]);

        let decision = Decision::new(
            "session-123",
            "Which option to choose?",
            options.clone(),
            "weighted_sum",
            recommendation.clone(),
            scores.clone(),
        );

        assert_eq!(decision.session_id, "session-123");
        assert_eq!(decision.question, "Which option to choose?");
        assert_eq!(decision.options, options);
        assert_eq!(decision.method, "weighted_sum");
        assert_eq!(decision.recommendation, recommendation);
        assert_eq!(decision.scores, scores);
        assert!(decision.criteria.is_none());
        assert!(!decision.id.is_empty());
    }

    #[test]
    fn test_decision_with_criteria() {
        let options = vec!["a".to_string()];
        let criteria = vec![StoredCriterion {
            name: "cost".to_string(),
            weight: 0.5,
            description: Some("Total cost".to_string()),
        }];

        let decision = Decision::new(
            "s1",
            "question",
            options,
            "method",
            serde_json::json!({}),
            serde_json::json!([]),
        )
        .with_criteria(criteria.clone());

        assert_eq!(decision.criteria.unwrap().len(), 1);
    }

    #[test]
    fn test_perspective_analysis_new() {
        let stakeholders = serde_json::json!([{"name": "users"}]);
        let synthesis = serde_json::json!({"summary": "analysis"});

        let analysis = PerspectiveAnalysis::new(
            "session-123",
            "new feature",
            stakeholders.clone(),
            synthesis.clone(),
            0.85,
        );

        assert_eq!(analysis.session_id, "session-123");
        assert_eq!(analysis.topic, "new feature");
        assert_eq!(analysis.stakeholders, stakeholders);
        assert_eq!(analysis.synthesis, synthesis);
        assert_eq!(analysis.confidence, 0.85);
        assert!(analysis.power_matrix.is_none());
        assert!(!analysis.id.is_empty());
    }

    #[test]
    fn test_perspective_analysis_confidence_clamping() {
        let stakeholders = serde_json::json!([]);
        let synthesis = serde_json::json!({});

        let analysis1 =
            PerspectiveAnalysis::new("s1", "topic", stakeholders.clone(), synthesis.clone(), 1.5);
        assert_eq!(analysis1.confidence, 1.0);

        let analysis2 = PerspectiveAnalysis::new("s1", "topic", stakeholders, synthesis, -0.5);
        assert_eq!(analysis2.confidence, 0.0);
    }

    #[test]
    fn test_evidence_assessment_new() {
        let evidence = serde_json::json!([{"type": "empirical"}]);
        let support = serde_json::json!({"level": "strong"});
        let analysis = serde_json::json!([{"id": 1}]);

        let assessment = EvidenceAssessment::new(
            "session-123",
            "climate change is real",
            evidence.clone(),
            support.clone(),
            analysis.clone(),
        );

        assert_eq!(assessment.session_id, "session-123");
        assert_eq!(assessment.claim, "climate change is real");
        assert_eq!(assessment.evidence, evidence);
        assert_eq!(assessment.overall_support, support);
        assert_eq!(assessment.evidence_analysis, analysis);
        assert!(assessment.chain_analysis.is_none());
        assert!(!assessment.id.is_empty());
    }

    #[test]
    fn test_probability_update_new() {
        let steps = serde_json::json!([{"step": 1}]);
        let interpretation = serde_json::json!({"result": "increased"});

        let update = ProbabilityUpdate::new(
            "session-123",
            "hypothesis X is true",
            0.5,
            0.75,
            steps.clone(),
            interpretation.clone(),
        );

        assert_eq!(update.session_id, "session-123");
        assert_eq!(update.hypothesis, "hypothesis X is true");
        assert_eq!(update.prior, 0.5);
        assert_eq!(update.posterior, 0.75);
        assert_eq!(update.update_steps, steps);
        assert_eq!(update.interpretation, interpretation);
        assert!(update.confidence_lower.is_none());
        assert!(!update.id.is_empty());
    }

    #[test]
    fn test_probability_update_prior_posterior_clamping() {
        let steps = serde_json::json!([]);
        let interpretation = serde_json::json!({});

        let update1 =
            ProbabilityUpdate::new("s1", "h1", 1.5, -0.5, steps.clone(), interpretation.clone());
        assert_eq!(update1.prior, 1.0);
        assert_eq!(update1.posterior, 0.0);

        let update2 = ProbabilityUpdate::new("s1", "h1", -0.2, 1.2, steps, interpretation);
        assert_eq!(update2.prior, 0.0);
        assert_eq!(update2.posterior, 1.0);
    }

    #[test]
    fn test_probability_update_with_confidence_interval() {
        let steps = serde_json::json!([]);
        let interpretation = serde_json::json!({});

        let update = ProbabilityUpdate::new("s1", "h1", 0.5, 0.7, steps, interpretation)
            .with_confidence_interval(Some(0.6), Some(0.8), Some(0.95));

        assert_eq!(update.confidence_lower, Some(0.6));
        assert_eq!(update.confidence_upper, Some(0.8));
        assert_eq!(update.confidence_level, Some(0.95));
    }

    #[test]
    fn test_probability_update_confidence_interval_clamping() {
        let steps = serde_json::json!([]);
        let interpretation = serde_json::json!({});

        let update = ProbabilityUpdate::new("s1", "h1", 0.5, 0.7, steps, interpretation)
            .with_confidence_interval(Some(1.5), Some(-0.5), Some(2.0));

        assert_eq!(update.confidence_lower, Some(1.0));
        assert_eq!(update.confidence_upper, Some(0.0));
        assert_eq!(update.confidence_level, Some(1.0));
    }

    // ========================================================================
    // Invocation fallback tracking tests
    // ========================================================================

    #[test]
    fn test_invocation_new_defaults_no_fallback() {
        let inv = Invocation::new("test_tool", serde_json::json!({"key": "value"}));
        assert!(!inv.fallback_used);
        assert!(inv.fallback_type.is_none());
    }

    #[test]
    fn test_invocation_with_fallback() {
        let inv = Invocation::new("test_tool", serde_json::json!({})).with_fallback("parse_error");
        assert!(inv.fallback_used);
        assert_eq!(inv.fallback_type, Some("parse_error".to_string()));
    }

    #[test]
    fn test_invocation_with_parse_error_fallback() {
        let inv = Invocation::new("test_tool", serde_json::json!({})).with_parse_error_fallback();
        assert!(inv.fallback_used);
        assert_eq!(inv.fallback_type, Some("parse_error".to_string()));
    }

    #[test]
    fn test_invocation_with_api_unavailable_fallback() {
        let inv =
            Invocation::new("test_tool", serde_json::json!({})).with_api_unavailable_fallback();
        assert!(inv.fallback_used);
        assert_eq!(inv.fallback_type, Some("api_unavailable".to_string()));
    }

    #[test]
    fn test_invocation_with_local_calculation_fallback() {
        let inv =
            Invocation::new("test_tool", serde_json::json!({})).with_local_calculation_fallback();
        assert!(inv.fallback_used);
        assert_eq!(inv.fallback_type, Some("local_calculation".to_string()));
    }

    #[test]
    fn test_invocation_builder_chain_with_fallback() {
        let inv = Invocation::new("test_tool", serde_json::json!({"test": true}))
            .with_session("session-123")
            .with_pipe("test-pipe-v1")
            .with_fallback("api_unavailable")
            .success(serde_json::json!({"result": "ok"}), 150);

        assert_eq!(inv.session_id, Some("session-123".to_string()));
        assert_eq!(inv.pipe_name, Some("test-pipe-v1".to_string()));
        assert!(inv.fallback_used);
        assert_eq!(inv.fallback_type, Some("api_unavailable".to_string()));
        assert!(inv.success);
        assert_eq!(inv.latency_ms, Some(150));
    }

    // ========================================================================
    // FallbackMetricsSummary tests
    // ========================================================================

    #[test]
    fn test_fallback_metrics_summary_serialization() {
        use std::collections::HashMap;

        let mut by_type = HashMap::new();
        by_type.insert("parse_error".to_string(), 5);
        by_type.insert("api_unavailable".to_string(), 3);

        let mut by_pipe = HashMap::new();
        by_pipe.insert("linear-reasoning-v1".to_string(), 4);
        by_pipe.insert("tree-reasoning-v1".to_string(), 4);

        let summary = FallbackMetricsSummary {
            total_fallbacks: 8,
            fallbacks_by_type: by_type,
            fallbacks_by_pipe: by_pipe,
            total_invocations: 100,
            fallback_rate: 0.08,
            recommendation: "Test recommendation".to_string(),
            timestamp_reconstructions: 0,
            records_skipped: 0,
        };

        // Test serialization
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"total_fallbacks\":8"));
        assert!(json.contains("\"fallback_rate\":0.08"));
        assert!(json.contains("\"recommendation\":\"Test recommendation\""));

        // Test deserialization
        let deserialized: FallbackMetricsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_fallbacks, 8);
        assert_eq!(deserialized.total_invocations, 100);
        assert_eq!(deserialized.fallback_rate, 0.08);
    }
}
