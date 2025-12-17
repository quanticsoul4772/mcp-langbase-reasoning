mod sqlite;

pub use sqlite::SqliteStorage;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::StorageResult;

/// Session represents a reasoning context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
    /// Active branch for tree mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_branch_id: Option<String>,
}

/// Thought represents a single reasoning step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub confidence: f64,
    pub mode: String,
    pub parent_id: Option<String>,
    /// Branch ID for tree mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Branch represents a reasoning branch in tree mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub id: String,
    pub session_id: String,
    pub name: Option<String>,
    pub parent_branch_id: Option<String>,
    pub priority: f64,
    pub confidence: f64,
    pub state: BranchState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Branch state for tree mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchState {
    #[default]
    Active,
    Completed,
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

/// Cross-reference between branches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossRef {
    pub id: String,
    pub from_branch_id: String,
    pub to_branch_id: String,
    pub ref_type: CrossRefType,
    pub reason: Option<String>,
    pub strength: f64,
    pub created_at: DateTime<Utc>,
}

/// Type of cross-reference between branches
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossRefType {
    #[default]
    Supports,
    Contradicts,
    Extends,
    Alternative,
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

/// Checkpoint for state snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub session_id: String,
    pub branch_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub snapshot: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Graph node for Graph-of-Thoughts reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub node_type: NodeType,
    pub score: Option<f64>,
    pub depth: i32,
    pub is_terminal: bool,
    pub is_root: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Type of graph node
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    #[default]
    Thought,
    Hypothesis,
    Conclusion,
    Aggregation,
    Root,
    Refinement,
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

/// Graph edge for Graph-of-Thoughts connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub session_id: String,
    pub from_node: String,
    pub to_node: String,
    pub edge_type: EdgeType,
    pub weight: f64,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Type of graph edge
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    #[default]
    Generates,
    Refines,
    Aggregates,
    Supports,
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

/// State snapshot for backtracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub id: String,
    pub session_id: String,
    pub snapshot_type: SnapshotType,
    pub state_data: serde_json::Value,
    pub parent_snapshot_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,
}

/// Type of state snapshot
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotType {
    #[default]
    Full,
    Incremental,
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
    pub fn new(
        session_id: impl Into<String>,
        state_data: serde_json::Value,
    ) -> Self {
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

/// Invocation log entry for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invocation {
    pub id: String,
    pub session_id: Option<String>,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub pipe_name: Option<String>,
    pub latency_ms: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
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

/// Storage trait for database operations
#[async_trait]
pub trait Storage: Send + Sync {
    // Session operations
    async fn create_session(&self, session: &Session) -> StorageResult<()>;
    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>>;
    async fn update_session(&self, session: &Session) -> StorageResult<()>;
    async fn delete_session(&self, id: &str) -> StorageResult<()>;

    // Thought operations
    async fn create_thought(&self, thought: &Thought) -> StorageResult<()>;
    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>>;
    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>>;
    async fn get_branch_thoughts(&self, branch_id: &str) -> StorageResult<Vec<Thought>>;
    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>>;

    // Branch operations (tree mode)
    async fn create_branch(&self, branch: &Branch) -> StorageResult<()>;
    async fn get_branch(&self, id: &str) -> StorageResult<Option<Branch>>;
    async fn get_session_branches(&self, session_id: &str) -> StorageResult<Vec<Branch>>;
    async fn get_child_branches(&self, parent_id: &str) -> StorageResult<Vec<Branch>>;
    async fn update_branch(&self, branch: &Branch) -> StorageResult<()>;
    async fn delete_branch(&self, id: &str) -> StorageResult<()>;

    // Cross-reference operations (tree mode)
    async fn create_cross_ref(&self, cross_ref: &CrossRef) -> StorageResult<()>;
    async fn get_cross_refs_from(&self, branch_id: &str) -> StorageResult<Vec<CrossRef>>;
    async fn get_cross_refs_to(&self, branch_id: &str) -> StorageResult<Vec<CrossRef>>;
    async fn delete_cross_ref(&self, id: &str) -> StorageResult<()>;

    // Checkpoint operations (backtracking)
    async fn create_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;
    async fn get_checkpoint(&self, id: &str) -> StorageResult<Option<Checkpoint>>;
    async fn get_session_checkpoints(&self, session_id: &str) -> StorageResult<Vec<Checkpoint>>;
    async fn get_branch_checkpoints(&self, branch_id: &str) -> StorageResult<Vec<Checkpoint>>;
    async fn delete_checkpoint(&self, id: &str) -> StorageResult<()>;

    // Invocation logging
    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;

    // Graph node operations (GoT mode)
    async fn create_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    async fn get_graph_node(&self, id: &str) -> StorageResult<Option<GraphNode>>;
    async fn get_session_graph_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    async fn get_active_graph_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    async fn get_root_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    async fn get_terminal_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>>;
    async fn update_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    async fn delete_graph_node(&self, id: &str) -> StorageResult<()>;

    // Graph edge operations (GoT mode)
    async fn create_graph_edge(&self, edge: &GraphEdge) -> StorageResult<()>;
    async fn get_graph_edge(&self, id: &str) -> StorageResult<Option<GraphEdge>>;
    async fn get_edges_from(&self, node_id: &str) -> StorageResult<Vec<GraphEdge>>;
    async fn get_edges_to(&self, node_id: &str) -> StorageResult<Vec<GraphEdge>>;
    async fn get_session_edges(&self, session_id: &str) -> StorageResult<Vec<GraphEdge>>;
    async fn delete_graph_edge(&self, id: &str) -> StorageResult<()>;

    // State snapshot operations (backtracking)
    async fn create_snapshot(&self, snapshot: &StateSnapshot) -> StorageResult<()>;
    async fn get_snapshot(&self, id: &str) -> StorageResult<Option<StateSnapshot>>;
    async fn get_session_snapshots(&self, session_id: &str) -> StorageResult<Vec<StateSnapshot>>;
    async fn get_latest_snapshot(&self, session_id: &str) -> StorageResult<Option<StateSnapshot>>;
    async fn delete_snapshot(&self, id: &str) -> StorageResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Session tests
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

    // Thought tests
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

    // Branch tests
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

    // BranchState tests
    #[test]
    fn test_branch_state_display() {
        assert_eq!(BranchState::Active.to_string(), "active");
        assert_eq!(BranchState::Completed.to_string(), "completed");
        assert_eq!(BranchState::Abandoned.to_string(), "abandoned");
    }

    #[test]
    fn test_branch_state_from_str() {
        assert_eq!("active".parse::<BranchState>().unwrap(), BranchState::Active);
        assert_eq!("completed".parse::<BranchState>().unwrap(), BranchState::Completed);
        assert_eq!("abandoned".parse::<BranchState>().unwrap(), BranchState::Abandoned);
        assert_eq!("ACTIVE".parse::<BranchState>().unwrap(), BranchState::Active); // case insensitive
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

    // CrossRef tests
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

    // CrossRefType tests
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
        assert_eq!("supports".parse::<CrossRefType>().unwrap(), CrossRefType::Supports);
        assert_eq!("contradicts".parse::<CrossRefType>().unwrap(), CrossRefType::Contradicts);
        assert_eq!("extends".parse::<CrossRefType>().unwrap(), CrossRefType::Extends);
        assert_eq!("alternative".parse::<CrossRefType>().unwrap(), CrossRefType::Alternative);
        assert_eq!("depends".parse::<CrossRefType>().unwrap(), CrossRefType::Depends);
        assert_eq!("SUPPORTS".parse::<CrossRefType>().unwrap(), CrossRefType::Supports); // case insensitive
    }

    #[test]
    fn test_cross_ref_type_from_str_invalid() {
        let result = "invalid".parse::<CrossRefType>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown cross-ref type"));
    }

    // Checkpoint tests
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
        let checkpoint =
            Checkpoint::new("sess-1", "CP", json!({})).with_branch("branch-123");
        assert_eq!(checkpoint.branch_id, Some("branch-123".to_string()));
    }

    #[test]
    fn test_checkpoint_with_description() {
        let checkpoint = Checkpoint::new("sess-1", "CP", json!({}))
            .with_description("Before major changes");
        assert_eq!(
            checkpoint.description,
            Some("Before major changes".to_string())
        );
    }

    // Invocation tests
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
        let invocation =
            Invocation::new("reasoning.tree", json!({})).with_session("sess-123");
        assert_eq!(invocation.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_invocation_with_pipe() {
        let invocation =
            Invocation::new("reasoning.linear", json!({})).with_pipe("linear-reasoning-v1");
        assert_eq!(invocation.pipe_name, Some("linear-reasoning-v1".to_string()));
    }

    #[test]
    fn test_invocation_success() {
        let output = json!({"result": "success"});
        let invocation =
            Invocation::new("reasoning.linear", json!({})).success(output.clone(), 150);
        assert!(invocation.success);
        assert_eq!(invocation.output, Some(output));
        assert_eq!(invocation.latency_ms, Some(150));
        assert!(invocation.error.is_none());
    }

    #[test]
    fn test_invocation_failure() {
        let invocation =
            Invocation::new("reasoning.linear", json!({})).failure("API timeout", 5000);
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

    // Phase 3: GraphNode tests
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

    // NodeType tests
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
        assert_eq!("hypothesis".parse::<NodeType>().unwrap(), NodeType::Hypothesis);
        assert_eq!("conclusion".parse::<NodeType>().unwrap(), NodeType::Conclusion);
        assert_eq!("aggregation".parse::<NodeType>().unwrap(), NodeType::Aggregation);
        assert_eq!("root".parse::<NodeType>().unwrap(), NodeType::Root);
        assert_eq!("refinement".parse::<NodeType>().unwrap(), NodeType::Refinement);
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

    // GraphEdge tests
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

    // EdgeType tests
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
        assert_eq!("generates".parse::<EdgeType>().unwrap(), EdgeType::Generates);
        assert_eq!("refines".parse::<EdgeType>().unwrap(), EdgeType::Refines);
        assert_eq!("aggregates".parse::<EdgeType>().unwrap(), EdgeType::Aggregates);
        assert_eq!("supports".parse::<EdgeType>().unwrap(), EdgeType::Supports);
        assert_eq!("contradicts".parse::<EdgeType>().unwrap(), EdgeType::Contradicts);
        assert_eq!("GENERATES".parse::<EdgeType>().unwrap(), EdgeType::Generates);
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

    // StateSnapshot tests
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
        let snapshot = StateSnapshot::new("sess-1", json!({}))
            .with_description("Before major refactor");
        assert_eq!(snapshot.description, Some("Before major refactor".to_string()));
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

    // SnapshotType tests
    #[test]
    fn test_snapshot_type_display() {
        assert_eq!(SnapshotType::Full.to_string(), "full");
        assert_eq!(SnapshotType::Incremental.to_string(), "incremental");
        assert_eq!(SnapshotType::Branch.to_string(), "branch");
    }

    #[test]
    fn test_snapshot_type_from_str() {
        assert_eq!("full".parse::<SnapshotType>().unwrap(), SnapshotType::Full);
        assert_eq!("incremental".parse::<SnapshotType>().unwrap(), SnapshotType::Incremental);
        assert_eq!("branch".parse::<SnapshotType>().unwrap(), SnapshotType::Branch);
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
}
