use async_trait::async_trait;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tracing::{info, warn};

use super::{
    Branch, Checkpoint, CrossRef, Decision, Detection, DetectionType, EvidenceAssessment,
    GraphEdge, GraphNode, Invocation, PerspectiveAnalysis, ProbabilityUpdate, Session,
    StateSnapshot, Storage, StoredCriterion, Thought,
};
#[cfg(test)]
use super::{BranchState, CrossRefType, EdgeType};
use crate::config::DatabaseConfig;
use crate::error::{StorageError, StorageResult};

/// Serialize optional data to JSON string, propagating errors.
fn serialize_json<T: serde::Serialize>(
    data: &Option<T>,
    field_name: &str,
) -> StorageResult<Option<String>> {
    data.as_ref()
        .map(|d| serde_json::to_string(d))
        .transpose()
        .map_err(|e| StorageError::Serialization {
            message: format!("Failed to serialize {}: {}", field_name, e),
        })
}

/// Serialize required data to JSON string, propagating errors.
fn serialize_json_required<T: serde::Serialize>(
    data: &T,
    field_name: &str,
) -> StorageResult<String> {
    serde_json::to_string(data).map_err(|e| StorageError::Serialization {
        message: format!("Failed to serialize {}: {}", field_name, e),
    })
}

/// Static migrator that embeds migrations at compile time
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// SQLite-backed storage implementation
#[derive(Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// Create a new SQLite storage instance
    pub async fn new(config: &DatabaseConfig) -> StorageResult<Self> {
        // Ensure parent directory exists
        if let Some(parent) = config.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StorageError::Connection {
                message: format!("Failed to create database directory: {}", e),
            })?;
        }

        let database_url = format!("sqlite://{}?mode=rwc", config.path.display());

        let options = SqliteConnectOptions::from_str(&database_url)
            .map_err(|e| StorageError::Connection {
                message: format!("Invalid database URL: {}", e),
            })?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .connect_with(options)
            .await
            .map_err(|e| StorageError::Connection {
                message: format!("Failed to connect to database: {}", e),
            })?;

        let storage = Self { pool };
        storage.run_migrations().await?;

        Ok(storage)
    }

    /// Run database migrations using embedded sqlx migrations
    async fn run_migrations(&self) -> StorageResult<()> {
        info!("Running database migrations...");

        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|e| StorageError::Migration {
                message: format!("Failed to run migrations: {}", e),
            })?;

        info!("Database migrations completed successfully");
        Ok(())
    }

    /// Get the underlying pool for advanced queries
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Create an in-memory SQLite storage instance for testing
    pub async fn new_in_memory() -> StorageResult<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").map_err(|e| {
            StorageError::Connection {
                message: format!("Invalid in-memory URL: {}", e),
            }
        })?;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|e| StorageError::Connection {
                message: format!("Failed to create in-memory database: {}", e),
            })?;

        let storage = Self { pool };
        storage.run_migrations().await?;

        Ok(storage)
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn create_session(&self, session: &Session) -> StorageResult<()> {
        let metadata = serialize_json(&session.metadata, "session.metadata")?;

        sqlx::query(
            r#"
            INSERT INTO sessions (id, mode, created_at, updated_at, metadata, active_branch_id)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.mode)
        .bind(session.created_at.to_rfc3339())
        .bind(session.updated_at.to_rfc3339())
        .bind(&metadata)
        .bind(&session.active_branch_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>> {
        let row: Option<SessionRow> = sqlx::query_as(
            r#"
            SELECT id, mode, created_at, updated_at, metadata, active_branch_id
            FROM sessions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn update_session(&self, session: &Session) -> StorageResult<()> {
        let metadata = serialize_json(&session.metadata, "session.metadata")?;

        let result = sqlx::query(
            r#"
            UPDATE sessions
            SET mode = ?, updated_at = ?, metadata = ?, active_branch_id = ?
            WHERE id = ?
            "#,
        )
        .bind(&session.mode)
        .bind(session.updated_at.to_rfc3339())
        .bind(&metadata)
        .bind(&session.active_branch_id)
        .bind(&session.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::SessionNotFound {
                session_id: session.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete_session(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn create_thought(&self, thought: &Thought) -> StorageResult<()> {
        let metadata = serialize_json(&thought.metadata, "thought.metadata")?;

        sqlx::query(
            r#"
            INSERT INTO thoughts (id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&thought.id)
        .bind(&thought.session_id)
        .bind(&thought.content)
        .bind(thought.confidence)
        .bind(&thought.mode)
        .bind(&thought.parent_id)
        .bind(&thought.branch_id)
        .bind(thought.created_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>> {
        let row: Option<ThoughtRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata
            FROM thoughts
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>> {
        let rows: Vec<ThoughtRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata
            FROM thoughts
            WHERE session_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_branch_thoughts(&self, branch_id: &str) -> StorageResult<Vec<Thought>> {
        let rows: Vec<ThoughtRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata
            FROM thoughts
            WHERE branch_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(branch_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>> {
        let row: Option<ThoughtRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata
            FROM thoughts
            WHERE session_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()> {
        let input = serialize_json_required(&invocation.input, "invocation.input")?;
        let output = serialize_json(&invocation.output, "invocation.output")?;

        sqlx::query(
            r#"
            INSERT INTO invocations (id, session_id, tool_name, input, output, pipe_name, latency_ms, success, error, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&invocation.id)
        .bind(&invocation.session_id)
        .bind(&invocation.tool_name)
        .bind(&input)
        .bind(&output)
        .bind(&invocation.pipe_name)
        .bind(invocation.latency_ms)
        .bind(invocation.success)
        .bind(&invocation.error)
        .bind(invocation.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Branch operations
    async fn create_branch(&self, branch: &Branch) -> StorageResult<()> {
        let metadata = serialize_json(&branch.metadata, "branch.metadata")?;

        sqlx::query(
            r#"
            INSERT INTO branches (id, session_id, name, parent_branch_id, priority, confidence, state, created_at, updated_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&branch.id)
        .bind(&branch.session_id)
        .bind(&branch.name)
        .bind(&branch.parent_branch_id)
        .bind(branch.priority)
        .bind(branch.confidence)
        .bind(branch.state.to_string())
        .bind(branch.created_at.to_rfc3339())
        .bind(branch.updated_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_branch(&self, id: &str) -> StorageResult<Option<Branch>> {
        let row: Option<BranchRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, name, parent_branch_id, priority, confidence, state, created_at, updated_at, metadata
            FROM branches
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_branches(&self, session_id: &str) -> StorageResult<Vec<Branch>> {
        let rows: Vec<BranchRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, name, parent_branch_id, priority, confidence, state, created_at, updated_at, metadata
            FROM branches
            WHERE session_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_child_branches(&self, parent_id: &str) -> StorageResult<Vec<Branch>> {
        let rows: Vec<BranchRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, name, parent_branch_id, priority, confidence, state, created_at, updated_at, metadata
            FROM branches
            WHERE parent_branch_id = ?
            ORDER BY priority DESC, created_at ASC
            "#,
        )
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn update_branch(&self, branch: &Branch) -> StorageResult<()> {
        let metadata = serialize_json(&branch.metadata, "branch.metadata")?;

        let result = sqlx::query(
            r#"
            UPDATE branches
            SET name = ?, priority = ?, confidence = ?, state = ?, updated_at = ?, metadata = ?
            WHERE id = ?
            "#,
        )
        .bind(&branch.name)
        .bind(branch.priority)
        .bind(branch.confidence)
        .bind(branch.state.to_string())
        .bind(branch.updated_at.to_rfc3339())
        .bind(&metadata)
        .bind(&branch.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::Query {
                message: format!("Branch not found: {}", branch.id),
            });
        }

        Ok(())
    }

    async fn delete_branch(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM branches WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Cross-reference operations
    async fn create_cross_ref(&self, cross_ref: &CrossRef) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO cross_refs (id, from_branch_id, to_branch_id, ref_type, reason, strength, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&cross_ref.id)
        .bind(&cross_ref.from_branch_id)
        .bind(&cross_ref.to_branch_id)
        .bind(cross_ref.ref_type.to_string())
        .bind(&cross_ref.reason)
        .bind(cross_ref.strength)
        .bind(cross_ref.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_cross_refs_from(&self, branch_id: &str) -> StorageResult<Vec<CrossRef>> {
        let rows: Vec<CrossRefRow> = sqlx::query_as(
            r#"
            SELECT id, from_branch_id, to_branch_id, ref_type, reason, strength, created_at
            FROM cross_refs
            WHERE from_branch_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(branch_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_cross_refs_to(&self, branch_id: &str) -> StorageResult<Vec<CrossRef>> {
        let rows: Vec<CrossRefRow> = sqlx::query_as(
            r#"
            SELECT id, from_branch_id, to_branch_id, ref_type, reason, strength, created_at
            FROM cross_refs
            WHERE to_branch_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(branch_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_cross_ref(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM cross_refs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Checkpoint operations
    async fn create_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()> {
        let snapshot = serialize_json_required(&checkpoint.snapshot, "checkpoint.snapshot")?;

        sqlx::query(
            r#"
            INSERT INTO checkpoints (id, session_id, branch_id, name, description, snapshot, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&checkpoint.id)
        .bind(&checkpoint.session_id)
        .bind(&checkpoint.branch_id)
        .bind(&checkpoint.name)
        .bind(&checkpoint.description)
        .bind(&snapshot)
        .bind(checkpoint.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_checkpoint(&self, id: &str) -> StorageResult<Option<Checkpoint>> {
        let row: Option<CheckpointRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, branch_id, name, description, snapshot, created_at
            FROM checkpoints
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_checkpoints(&self, session_id: &str) -> StorageResult<Vec<Checkpoint>> {
        let rows: Vec<CheckpointRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, branch_id, name, description, snapshot, created_at
            FROM checkpoints
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_branch_checkpoints(&self, branch_id: &str) -> StorageResult<Vec<Checkpoint>> {
        let rows: Vec<CheckpointRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, branch_id, name, description, snapshot, created_at
            FROM checkpoints
            WHERE branch_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(branch_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_checkpoint(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM checkpoints WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Graph node operations (GoT mode)
    async fn create_graph_node(&self, node: &GraphNode) -> StorageResult<()> {
        let metadata = serialize_json(&node.metadata, "graph_node.metadata")?;

        sqlx::query(
            r#"
            INSERT INTO graph_nodes (id, session_id, content, node_type, score, depth, is_terminal, is_root, is_active, created_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&node.id)
        .bind(&node.session_id)
        .bind(&node.content)
        .bind(node.node_type.to_string())
        .bind(node.score)
        .bind(node.depth)
        .bind(node.is_terminal)
        .bind(node.is_root)
        .bind(node.is_active)
        .bind(node.created_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_graph_node(&self, id: &str) -> StorageResult<Option<GraphNode>> {
        let row: Option<GraphNodeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, node_type, score, depth, is_terminal, is_root, is_active, created_at, metadata
            FROM graph_nodes
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_graph_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>> {
        let rows: Vec<GraphNodeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, node_type, score, depth, is_terminal, is_root, is_active, created_at, metadata
            FROM graph_nodes
            WHERE session_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_active_graph_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>> {
        let rows: Vec<GraphNodeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, node_type, score, depth, is_terminal, is_root, is_active, created_at, metadata
            FROM graph_nodes
            WHERE session_id = ? AND is_active = 1
            ORDER BY depth ASC, created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_root_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>> {
        let rows: Vec<GraphNodeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, node_type, score, depth, is_terminal, is_root, is_active, created_at, metadata
            FROM graph_nodes
            WHERE session_id = ? AND is_root = 1
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_terminal_nodes(&self, session_id: &str) -> StorageResult<Vec<GraphNode>> {
        let rows: Vec<GraphNodeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, node_type, score, depth, is_terminal, is_root, is_active, created_at, metadata
            FROM graph_nodes
            WHERE session_id = ? AND is_terminal = 1
            ORDER BY score DESC, created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn update_graph_node(&self, node: &GraphNode) -> StorageResult<()> {
        let metadata = serialize_json(&node.metadata, "graph_node.metadata")?;

        let result = sqlx::query(
            r#"
            UPDATE graph_nodes
            SET content = ?, node_type = ?, score = ?, depth = ?, is_terminal = ?, is_root = ?, is_active = ?, metadata = ?
            WHERE id = ?
            "#,
        )
        .bind(&node.content)
        .bind(node.node_type.to_string())
        .bind(node.score)
        .bind(node.depth)
        .bind(node.is_terminal)
        .bind(node.is_root)
        .bind(node.is_active)
        .bind(&metadata)
        .bind(&node.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::Query {
                message: format!("Graph node not found: {}", node.id),
            });
        }

        Ok(())
    }

    async fn delete_graph_node(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM graph_nodes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Graph edge operations (GoT mode)
    async fn create_graph_edge(&self, edge: &GraphEdge) -> StorageResult<()> {
        let metadata = serialize_json(&edge.metadata, "graph_edge.metadata")?;

        sqlx::query(
            r#"
            INSERT INTO graph_edges (id, session_id, from_node, to_node, edge_type, weight, created_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&edge.id)
        .bind(&edge.session_id)
        .bind(&edge.from_node)
        .bind(&edge.to_node)
        .bind(edge.edge_type.to_string())
        .bind(edge.weight)
        .bind(edge.created_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_graph_edge(&self, id: &str) -> StorageResult<Option<GraphEdge>> {
        let row: Option<GraphEdgeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, from_node, to_node, edge_type, weight, created_at, metadata
            FROM graph_edges
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_edges_from(&self, node_id: &str) -> StorageResult<Vec<GraphEdge>> {
        let rows: Vec<GraphEdgeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, from_node, to_node, edge_type, weight, created_at, metadata
            FROM graph_edges
            WHERE from_node = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_edges_to(&self, node_id: &str) -> StorageResult<Vec<GraphEdge>> {
        let rows: Vec<GraphEdgeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, from_node, to_node, edge_type, weight, created_at, metadata
            FROM graph_edges
            WHERE to_node = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_session_edges(&self, session_id: &str) -> StorageResult<Vec<GraphEdge>> {
        let rows: Vec<GraphEdgeRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, from_node, to_node, edge_type, weight, created_at, metadata
            FROM graph_edges
            WHERE session_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_graph_edge(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM graph_edges WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // State snapshot operations (backtracking)
    async fn create_snapshot(&self, snapshot: &StateSnapshot) -> StorageResult<()> {
        let state_data = serialize_json_required(&snapshot.state_data, "snapshot.state_data")?;

        sqlx::query(
            r#"
            INSERT INTO state_snapshots (id, session_id, snapshot_type, state_data, parent_snapshot_id, created_at, description)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&snapshot.id)
        .bind(&snapshot.session_id)
        .bind(snapshot.snapshot_type.to_string())
        .bind(&state_data)
        .bind(&snapshot.parent_snapshot_id)
        .bind(snapshot.created_at.to_rfc3339())
        .bind(&snapshot.description)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_snapshot(&self, id: &str) -> StorageResult<Option<StateSnapshot>> {
        let row: Option<StateSnapshotRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, snapshot_type, state_data, parent_snapshot_id, created_at, description
            FROM state_snapshots
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_snapshots(&self, session_id: &str) -> StorageResult<Vec<StateSnapshot>> {
        let rows: Vec<StateSnapshotRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, snapshot_type, state_data, parent_snapshot_id, created_at, description
            FROM state_snapshots
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_latest_snapshot(&self, session_id: &str) -> StorageResult<Option<StateSnapshot>> {
        let row: Option<StateSnapshotRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, snapshot_type, state_data, parent_snapshot_id, created_at, description
            FROM state_snapshots
            WHERE session_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn delete_snapshot(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM state_snapshots WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Detection operations (bias/fallacy analysis)
    // ========================================================================

    async fn create_detection(&self, detection: &Detection) -> StorageResult<()> {
        let metadata = serialize_json(&detection.metadata, "detection.metadata")?;

        sqlx::query(
            r#"
            INSERT INTO detections (id, session_id, thought_id, detection_type, detected_issue, severity, confidence, explanation, remediation, created_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&detection.id)
        .bind(&detection.session_id)
        .bind(&detection.thought_id)
        .bind(detection.detection_type.to_string())
        .bind(&detection.detected_issue)
        .bind(detection.severity)
        .bind(detection.confidence)
        .bind(&detection.explanation)
        .bind(&detection.remediation)
        .bind(detection.created_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_detection(&self, id: &str) -> StorageResult<Option<Detection>> {
        let row: Option<DetectionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, thought_id, detection_type, detected_issue, severity, confidence, explanation, remediation, created_at, metadata
            FROM detections
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_detections(&self, session_id: &str) -> StorageResult<Vec<Detection>> {
        let rows: Vec<DetectionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, thought_id, detection_type, detected_issue, severity, confidence, explanation, remediation, created_at, metadata
            FROM detections
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_thought_detections(&self, thought_id: &str) -> StorageResult<Vec<Detection>> {
        let rows: Vec<DetectionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, thought_id, detection_type, detected_issue, severity, confidence, explanation, remediation, created_at, metadata
            FROM detections
            WHERE thought_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(thought_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_detections_by_type(
        &self,
        detection_type: DetectionType,
    ) -> StorageResult<Vec<Detection>> {
        let rows: Vec<DetectionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, thought_id, detection_type, detected_issue, severity, confidence, explanation, remediation, created_at, metadata
            FROM detections
            WHERE detection_type = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(detection_type.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_session_detections_by_type(
        &self,
        session_id: &str,
        detection_type: DetectionType,
    ) -> StorageResult<Vec<Detection>> {
        let rows: Vec<DetectionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, thought_id, detection_type, detected_issue, severity, confidence, explanation, remediation, created_at, metadata
            FROM detections
            WHERE session_id = ? AND detection_type = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .bind(detection_type.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_detection(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM detections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Decision operations (decision framework)
    // ========================================================================

    async fn create_decision(&self, decision: &Decision) -> StorageResult<()> {
        let options_json = serialize_json_required(&decision.options, "decision.options")?;
        let criteria_json = serialize_json(&decision.criteria, "decision.criteria")?;
        let sensitivity_json = decision
            .sensitivity_analysis
            .as_ref()
            .map(|v| v.to_string());
        let trade_offs_json = decision.trade_offs.as_ref().map(|v| v.to_string());
        let constraints_json = decision
            .constraints_satisfied
            .as_ref()
            .map(|v| v.to_string());
        let metadata_json = decision.metadata.as_ref().map(|v| v.to_string());

        sqlx::query(
            r#"
            INSERT INTO decisions (
                id, session_id, question, options, criteria, method,
                recommendation, scores, sensitivity_analysis, trade_offs,
                constraints_satisfied, created_at, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&decision.id)
        .bind(&decision.session_id)
        .bind(&decision.question)
        .bind(&options_json)
        .bind(&criteria_json)
        .bind(&decision.method)
        .bind(decision.recommendation.to_string())
        .bind(decision.scores.to_string())
        .bind(&sensitivity_json)
        .bind(&trade_offs_json)
        .bind(&constraints_json)
        .bind(decision.created_at.to_rfc3339())
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_decision(&self, id: &str) -> StorageResult<Option<Decision>> {
        let row: Option<DecisionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, question, options, criteria, method,
                   recommendation, scores, sensitivity_analysis, trade_offs,
                   constraints_satisfied, created_at, metadata
            FROM decisions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_decisions(&self, session_id: &str) -> StorageResult<Vec<Decision>> {
        let rows: Vec<DecisionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, question, options, criteria, method,
                   recommendation, scores, sensitivity_analysis, trade_offs,
                   constraints_satisfied, created_at, metadata
            FROM decisions
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_decisions_by_method(&self, method: &str) -> StorageResult<Vec<Decision>> {
        let rows: Vec<DecisionRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, question, options, criteria, method,
                   recommendation, scores, sensitivity_analysis, trade_offs,
                   constraints_satisfied, created_at, metadata
            FROM decisions
            WHERE method = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(method)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_decision(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM decisions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Perspective analysis operations (decision framework)
    // ========================================================================

    async fn create_perspective(&self, analysis: &PerspectiveAnalysis) -> StorageResult<()> {
        let power_matrix_json = analysis.power_matrix.as_ref().map(|v| v.to_string());
        let conflicts_json = analysis.conflicts.as_ref().map(|v| v.to_string());
        let alignments_json = analysis.alignments.as_ref().map(|v| v.to_string());
        let metadata_json = analysis.metadata.as_ref().map(|v| v.to_string());

        sqlx::query(
            r#"
            INSERT INTO perspective_analyses (
                id, session_id, topic, stakeholders, power_matrix,
                conflicts, alignments, synthesis, confidence, created_at, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&analysis.id)
        .bind(&analysis.session_id)
        .bind(&analysis.topic)
        .bind(analysis.stakeholders.to_string())
        .bind(&power_matrix_json)
        .bind(&conflicts_json)
        .bind(&alignments_json)
        .bind(analysis.synthesis.to_string())
        .bind(analysis.confidence)
        .bind(analysis.created_at.to_rfc3339())
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_perspective(&self, id: &str) -> StorageResult<Option<PerspectiveAnalysis>> {
        let row: Option<PerspectiveRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, topic, stakeholders, power_matrix,
                   conflicts, alignments, synthesis, confidence, created_at, metadata
            FROM perspective_analyses
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_perspectives(
        &self,
        session_id: &str,
    ) -> StorageResult<Vec<PerspectiveAnalysis>> {
        let rows: Vec<PerspectiveRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, topic, stakeholders, power_matrix,
                   conflicts, alignments, synthesis, confidence, created_at, metadata
            FROM perspective_analyses
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_perspective(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM perspective_analyses WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Evidence assessment operations (evidence mode)
    // ========================================================================

    async fn create_evidence_assessment(
        &self,
        assessment: &EvidenceAssessment,
    ) -> StorageResult<()> {
        let chain_json = assessment.chain_analysis.as_ref().map(|v| v.to_string());
        let contradictions_json = assessment.contradictions.as_ref().map(|v| v.to_string());
        let gaps_json = assessment.gaps.as_ref().map(|v| v.to_string());
        let recommendations_json = assessment.recommendations.as_ref().map(|v| v.to_string());
        let metadata_json = assessment.metadata.as_ref().map(|v| v.to_string());

        sqlx::query(
            r#"
            INSERT INTO evidence_assessments (
                id, session_id, claim, evidence, overall_support, evidence_analysis,
                chain_analysis, contradictions, gaps, recommendations, created_at, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&assessment.id)
        .bind(&assessment.session_id)
        .bind(&assessment.claim)
        .bind(assessment.evidence.to_string())
        .bind(assessment.overall_support.to_string())
        .bind(assessment.evidence_analysis.to_string())
        .bind(&chain_json)
        .bind(&contradictions_json)
        .bind(&gaps_json)
        .bind(&recommendations_json)
        .bind(assessment.created_at.to_rfc3339())
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_evidence_assessment(&self, id: &str) -> StorageResult<Option<EvidenceAssessment>> {
        let row: Option<EvidenceAssessmentRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, claim, evidence, overall_support, evidence_analysis,
                   chain_analysis, contradictions, gaps, recommendations, created_at, metadata
            FROM evidence_assessments
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_evidence_assessments(
        &self,
        session_id: &str,
    ) -> StorageResult<Vec<EvidenceAssessment>> {
        let rows: Vec<EvidenceAssessmentRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, claim, evidence, overall_support, evidence_analysis,
                   chain_analysis, contradictions, gaps, recommendations, created_at, metadata
            FROM evidence_assessments
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_evidence_assessment(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM evidence_assessments WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Probability update operations (evidence mode)
    // ========================================================================

    async fn create_probability_update(&self, update: &ProbabilityUpdate) -> StorageResult<()> {
        let uncertainty_json = update.uncertainty_analysis.as_ref().map(|v| v.to_string());
        let sensitivity_json = update.sensitivity.as_ref().map(|v| v.to_string());
        let metadata_json = update.metadata.as_ref().map(|v| v.to_string());

        sqlx::query(
            r#"
            INSERT INTO probability_updates (
                id, session_id, hypothesis, prior, posterior,
                confidence_lower, confidence_upper, confidence_level,
                update_steps, uncertainty_analysis, sensitivity,
                interpretation, created_at, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&update.id)
        .bind(&update.session_id)
        .bind(&update.hypothesis)
        .bind(update.prior)
        .bind(update.posterior)
        .bind(update.confidence_lower)
        .bind(update.confidence_upper)
        .bind(update.confidence_level)
        .bind(update.update_steps.to_string())
        .bind(&uncertainty_json)
        .bind(&sensitivity_json)
        .bind(update.interpretation.to_string())
        .bind(update.created_at.to_rfc3339())
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_probability_update(&self, id: &str) -> StorageResult<Option<ProbabilityUpdate>> {
        let row: Option<ProbabilityUpdateRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, hypothesis, prior, posterior,
                   confidence_lower, confidence_upper, confidence_level,
                   update_steps, uncertainty_analysis, sensitivity,
                   interpretation, created_at, metadata
            FROM probability_updates
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn get_session_probability_updates(
        &self,
        session_id: &str,
    ) -> StorageResult<Vec<ProbabilityUpdate>> {
        let rows: Vec<ProbabilityUpdateRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, hypothesis, prior, posterior,
                   confidence_lower, confidence_upper, confidence_level,
                   update_steps, uncertainty_analysis, sensitivity,
                   interpretation, created_at, metadata
            FROM probability_updates
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_hypothesis_updates(
        &self,
        session_id: &str,
        hypothesis: &str,
    ) -> StorageResult<Vec<ProbabilityUpdate>> {
        let rows: Vec<ProbabilityUpdateRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, hypothesis, prior, posterior,
                   confidence_lower, confidence_upper, confidence_level,
                   update_steps, uncertainty_analysis, sensitivity,
                   interpretation, created_at, metadata
            FROM probability_updates
            WHERE session_id = ? AND hypothesis = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .bind(hypothesis)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn delete_probability_update(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM probability_updates WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

// ============================================================================
// Helper functions for parsing with logging
// ============================================================================

/// Parse JSON metadata with warning on failure
fn parse_metadata_with_logging(json_str: &str, context: &str) -> Option<serde_json::Value> {
    match serde_json::from_str(json_str) {
        Ok(value) => Some(value),
        Err(e) => {
            warn!(
                error = %e,
                json_preview = %json_str.chars().take(100).collect::<String>(),
                context = context,
                "Failed to parse metadata JSON, returning None"
            );
            None
        }
    }
}

/// Parse timestamp with warning on failure
fn parse_timestamp_with_logging(ts_str: &str, context: &str) -> chrono::DateTime<chrono::Utc> {
    use chrono::DateTime;
    match DateTime::parse_from_rfc3339(ts_str) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(e) => {
            warn!(
                error = %e,
                timestamp = ts_str,
                context = context,
                "Failed to parse timestamp, using current time as fallback"
            );
            chrono::Utc::now()
        }
    }
}

/// Parse enum with warning on failure
fn parse_enum_with_logging<T: std::str::FromStr + Default>(value: &str, context: &str) -> T {
    match value.parse() {
        Ok(parsed) => parsed,
        Err(_) => {
            warn!(
                value = value,
                context = context,
                default = %std::any::type_name::<T>(),
                "Failed to parse enum value, using default"
            );
            T::default()
        }
    }
}

// Internal row types for SQLx mapping
#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    mode: String,
    created_at: String,
    updated_at: String,
    metadata: Option<String>,
    active_branch_id: Option<String>,
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        Self {
            id: row.id.clone(),
            mode: row.mode,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("session {} created_at", row.id),
            ),
            updated_at: parse_timestamp_with_logging(
                &row.updated_at,
                &format!("session {} updated_at", row.id),
            ),
            metadata: row.metadata.and_then(|s| {
                parse_metadata_with_logging(&s, &format!("session {} metadata", row.id))
            }),
            active_branch_id: row.active_branch_id,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ThoughtRow {
    id: String,
    session_id: String,
    content: String,
    confidence: f64,
    mode: String,
    parent_id: Option<String>,
    branch_id: Option<String>,
    created_at: String,
    metadata: Option<String>,
}

impl From<ThoughtRow> for Thought {
    fn from(row: ThoughtRow) -> Self {
        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            content: row.content,
            confidence: row.confidence,
            mode: row.mode,
            parent_id: row.parent_id,
            branch_id: row.branch_id,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("thought {} created_at", row.id),
            ),
            metadata: row.metadata.and_then(|s| {
                parse_metadata_with_logging(&s, &format!("thought {} metadata", row.id))
            }),
        }
    }
}

#[derive(sqlx::FromRow)]
struct BranchRow {
    id: String,
    session_id: String,
    name: Option<String>,
    parent_branch_id: Option<String>,
    priority: f64,
    confidence: f64,
    state: String,
    created_at: String,
    updated_at: String,
    metadata: Option<String>,
}

impl From<BranchRow> for Branch {
    fn from(row: BranchRow) -> Self {
        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            name: row.name,
            parent_branch_id: row.parent_branch_id,
            priority: row.priority,
            confidence: row.confidence,
            state: parse_enum_with_logging(&row.state, &format!("branch {} state", row.id)),
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("branch {} created_at", row.id),
            ),
            updated_at: parse_timestamp_with_logging(
                &row.updated_at,
                &format!("branch {} updated_at", row.id),
            ),
            metadata: row.metadata.and_then(|s| {
                parse_metadata_with_logging(&s, &format!("branch {} metadata", row.id))
            }),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CrossRefRow {
    id: String,
    from_branch_id: String,
    to_branch_id: String,
    ref_type: String,
    reason: Option<String>,
    strength: f64,
    created_at: String,
}

impl From<CrossRefRow> for CrossRef {
    fn from(row: CrossRefRow) -> Self {
        Self {
            id: row.id.clone(),
            from_branch_id: row.from_branch_id,
            to_branch_id: row.to_branch_id,
            ref_type: parse_enum_with_logging(
                &row.ref_type,
                &format!("cross_ref {} ref_type", row.id),
            ),
            reason: row.reason,
            strength: row.strength,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("cross_ref {} created_at", row.id),
            ),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CheckpointRow {
    id: String,
    session_id: String,
    branch_id: Option<String>,
    name: String,
    description: Option<String>,
    snapshot: String,
    created_at: String,
}

impl From<CheckpointRow> for Checkpoint {
    fn from(row: CheckpointRow) -> Self {
        let snapshot = match serde_json::from_str(&row.snapshot) {
            Ok(value) => value,
            Err(e) => {
                warn!(
                    error = %e,
                    checkpoint_id = row.id,
                    snapshot_preview = %row.snapshot.chars().take(100).collect::<String>(),
                    "Failed to parse checkpoint snapshot, using null"
                );
                serde_json::Value::Null
            }
        };

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            branch_id: row.branch_id,
            name: row.name,
            description: row.description,
            snapshot,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("checkpoint {} created_at", row.id),
            ),
        }
    }
}

// Phase 3: Graph row types
#[derive(sqlx::FromRow)]
struct GraphNodeRow {
    id: String,
    session_id: String,
    content: String,
    node_type: String,
    score: Option<f64>,
    depth: i32,
    is_terminal: bool,
    is_root: bool,
    is_active: bool,
    created_at: String,
    metadata: Option<String>,
}

impl From<GraphNodeRow> for GraphNode {
    fn from(row: GraphNodeRow) -> Self {
        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            content: row.content,
            node_type: parse_enum_with_logging(
                &row.node_type,
                &format!("graph_node {} node_type", row.id),
            ),
            score: row.score,
            depth: row.depth,
            is_terminal: row.is_terminal,
            is_root: row.is_root,
            is_active: row.is_active,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("graph_node {} created_at", row.id),
            ),
            metadata: row.metadata.and_then(|s| {
                parse_metadata_with_logging(&s, &format!("graph_node {} metadata", row.id))
            }),
        }
    }
}

#[derive(sqlx::FromRow)]
struct GraphEdgeRow {
    id: String,
    session_id: String,
    from_node: String,
    to_node: String,
    edge_type: String,
    weight: f64,
    created_at: String,
    metadata: Option<String>,
}

impl From<GraphEdgeRow> for GraphEdge {
    fn from(row: GraphEdgeRow) -> Self {
        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            from_node: row.from_node,
            to_node: row.to_node,
            edge_type: parse_enum_with_logging(
                &row.edge_type,
                &format!("graph_edge {} edge_type", row.id),
            ),
            weight: row.weight,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("graph_edge {} created_at", row.id),
            ),
            metadata: row.metadata.and_then(|s| {
                parse_metadata_with_logging(&s, &format!("graph_edge {} metadata", row.id))
            }),
        }
    }
}

#[derive(sqlx::FromRow)]
struct StateSnapshotRow {
    id: String,
    session_id: String,
    snapshot_type: String,
    state_data: String,
    parent_snapshot_id: Option<String>,
    created_at: String,
    description: Option<String>,
}

impl From<StateSnapshotRow> for StateSnapshot {
    fn from(row: StateSnapshotRow) -> Self {
        let state_data = match serde_json::from_str(&row.state_data) {
            Ok(value) => value,
            Err(e) => {
                warn!(
                    error = %e,
                    snapshot_id = row.id,
                    state_data_preview = %row.state_data.chars().take(100).collect::<String>(),
                    "Failed to parse state snapshot data, using null"
                );
                serde_json::Value::Null
            }
        };

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            snapshot_type: parse_enum_with_logging(
                &row.snapshot_type,
                &format!("state_snapshot {} snapshot_type", row.id),
            ),
            state_data,
            parent_snapshot_id: row.parent_snapshot_id,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("state_snapshot {} created_at", row.id),
            ),
            description: row.description,
        }
    }
}

/// Row struct for Detection queries
#[derive(Debug, sqlx::FromRow)]
struct DetectionRow {
    id: String,
    session_id: Option<String>,
    thought_id: Option<String>,
    detection_type: String,
    detected_issue: String,
    severity: i32,
    confidence: f64,
    explanation: String,
    remediation: Option<String>,
    created_at: String,
    metadata: Option<String>,
}

impl From<DetectionRow> for Detection {
    fn from(row: DetectionRow) -> Self {
        let metadata = row.metadata.as_deref().and_then(|s| {
            parse_metadata_with_logging(s, &format!("detection {} metadata", row.id))
        });

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            thought_id: row.thought_id,
            detection_type: parse_enum_with_logging(
                &row.detection_type,
                &format!("detection {} detection_type", row.id),
            ),
            detected_issue: row.detected_issue,
            severity: row.severity,
            confidence: row.confidence,
            explanation: row.explanation,
            remediation: row.remediation,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("detection {} created_at", row.id),
            ),
            metadata,
        }
    }
}

// ============================================================================
// Decision Framework Row Types
// ============================================================================

/// Row struct for Decision queries
#[derive(Debug, sqlx::FromRow)]
struct DecisionRow {
    id: String,
    session_id: String,
    question: String,
    options: String,
    criteria: Option<String>,
    method: String,
    recommendation: String,
    scores: String,
    sensitivity_analysis: Option<String>,
    trade_offs: Option<String>,
    constraints_satisfied: Option<String>,
    created_at: String,
    metadata: Option<String>,
}

impl From<DecisionRow> for Decision {
    fn from(row: DecisionRow) -> Self {
        let options: Vec<String> = serde_json::from_str(&row.options).unwrap_or_else(|e| {
            warn!(
                error = %e,
                decision_id = row.id,
                "Failed to parse decision options, using empty vec"
            );
            Vec::new()
        });

        let criteria: Option<Vec<StoredCriterion>> = row.criteria.as_deref().and_then(|s| {
            serde_json::from_str(s).unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    decision_id = row.id,
                    "Failed to parse decision criteria, using None"
                );
                None
            })
        });

        let recommendation: serde_json::Value = serde_json::from_str(&row.recommendation)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    decision_id = row.id,
                    "Failed to parse recommendation, using null"
                );
                serde_json::Value::Null
            });

        let scores: serde_json::Value = serde_json::from_str(&row.scores).unwrap_or_else(|e| {
            warn!(
                error = %e,
                decision_id = row.id,
                "Failed to parse scores, using null"
            );
            serde_json::Value::Null
        });

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            question: row.question,
            options,
            criteria,
            method: row.method,
            recommendation,
            scores,
            sensitivity_analysis: row.sensitivity_analysis.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("decision {} sensitivity", row.id))
            }),
            trade_offs: row.trade_offs.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("decision {} trade_offs", row.id))
            }),
            constraints_satisfied: row.constraints_satisfied.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("decision {} constraints", row.id))
            }),
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("decision {} created_at", row.id),
            ),
            metadata: row.metadata.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("decision {} metadata", row.id))
            }),
        }
    }
}

/// Row struct for PerspectiveAnalysis queries
#[derive(Debug, sqlx::FromRow)]
struct PerspectiveRow {
    id: String,
    session_id: String,
    topic: String,
    stakeholders: String,
    power_matrix: Option<String>,
    conflicts: Option<String>,
    alignments: Option<String>,
    synthesis: String,
    confidence: f64,
    created_at: String,
    metadata: Option<String>,
}

impl From<PerspectiveRow> for PerspectiveAnalysis {
    fn from(row: PerspectiveRow) -> Self {
        let stakeholders: serde_json::Value = serde_json::from_str(&row.stakeholders)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    perspective_id = row.id,
                    "Failed to parse stakeholders, using null"
                );
                serde_json::Value::Null
            });

        let synthesis: serde_json::Value =
            serde_json::from_str(&row.synthesis).unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    perspective_id = row.id,
                    "Failed to parse synthesis, using null"
                );
                serde_json::Value::Null
            });

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            topic: row.topic,
            stakeholders,
            power_matrix: row.power_matrix.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("perspective {} power_matrix", row.id))
            }),
            conflicts: row.conflicts.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("perspective {} conflicts", row.id))
            }),
            alignments: row.alignments.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("perspective {} alignments", row.id))
            }),
            synthesis,
            confidence: row.confidence,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("perspective {} created_at", row.id),
            ),
            metadata: row.metadata.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("perspective {} metadata", row.id))
            }),
        }
    }
}

// ============================================================================
// Evidence Assessment Row Types
// ============================================================================

/// Row struct for EvidenceAssessment queries
#[derive(Debug, sqlx::FromRow)]
struct EvidenceAssessmentRow {
    id: String,
    session_id: String,
    claim: String,
    evidence: String,
    overall_support: String,
    evidence_analysis: String,
    chain_analysis: Option<String>,
    contradictions: Option<String>,
    gaps: Option<String>,
    recommendations: Option<String>,
    created_at: String,
    metadata: Option<String>,
}

impl From<EvidenceAssessmentRow> for EvidenceAssessment {
    fn from(row: EvidenceAssessmentRow) -> Self {
        let evidence: serde_json::Value = serde_json::from_str(&row.evidence).unwrap_or_else(|e| {
            warn!(
                error = %e,
                assessment_id = row.id,
                "Failed to parse evidence, using null"
            );
            serde_json::Value::Null
        });

        let overall_support: serde_json::Value = serde_json::from_str(&row.overall_support)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    assessment_id = row.id,
                    "Failed to parse overall_support, using null"
                );
                serde_json::Value::Null
            });

        let evidence_analysis: serde_json::Value = serde_json::from_str(&row.evidence_analysis)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    assessment_id = row.id,
                    "Failed to parse evidence_analysis, using null"
                );
                serde_json::Value::Null
            });

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            claim: row.claim,
            evidence,
            overall_support,
            evidence_analysis,
            chain_analysis: row.chain_analysis.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("assessment {} chain_analysis", row.id))
            }),
            contradictions: row.contradictions.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("assessment {} contradictions", row.id))
            }),
            gaps: row.gaps.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("assessment {} gaps", row.id))
            }),
            recommendations: row.recommendations.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("assessment {} recommendations", row.id))
            }),
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("assessment {} created_at", row.id),
            ),
            metadata: row.metadata.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("assessment {} metadata", row.id))
            }),
        }
    }
}

/// Row struct for ProbabilityUpdate queries
#[derive(Debug, sqlx::FromRow)]
struct ProbabilityUpdateRow {
    id: String,
    session_id: String,
    hypothesis: String,
    prior: f64,
    posterior: f64,
    confidence_lower: Option<f64>,
    confidence_upper: Option<f64>,
    confidence_level: Option<f64>,
    update_steps: String,
    uncertainty_analysis: Option<String>,
    sensitivity: Option<String>,
    interpretation: String,
    created_at: String,
    metadata: Option<String>,
}

impl From<ProbabilityUpdateRow> for ProbabilityUpdate {
    fn from(row: ProbabilityUpdateRow) -> Self {
        let update_steps: serde_json::Value = serde_json::from_str(&row.update_steps)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    update_id = row.id,
                    "Failed to parse update_steps, using null"
                );
                serde_json::Value::Null
            });

        let interpretation: serde_json::Value = serde_json::from_str(&row.interpretation)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    update_id = row.id,
                    "Failed to parse interpretation, using null"
                );
                serde_json::Value::Null
            });

        Self {
            id: row.id.clone(),
            session_id: row.session_id,
            hypothesis: row.hypothesis,
            prior: row.prior,
            posterior: row.posterior,
            confidence_lower: row.confidence_lower,
            confidence_upper: row.confidence_upper,
            confidence_level: row.confidence_level,
            update_steps,
            uncertainty_analysis: row.uncertainty_analysis.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("probability {} uncertainty", row.id))
            }),
            sensitivity: row.sensitivity.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("probability {} sensitivity", row.id))
            }),
            interpretation,
            created_at: parse_timestamp_with_logging(
                &row.created_at,
                &format!("probability {} created_at", row.id),
            ),
            metadata: row.metadata.as_deref().and_then(|s| {
                parse_metadata_with_logging(s, &format!("probability {} metadata", row.id))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    // ============================================================================
    // Helper Function Tests
    // ============================================================================

    #[test]
    fn test_parse_timestamp_valid_rfc3339() {
        let valid_ts = "2024-01-15T10:30:00Z";
        let result = parse_timestamp_with_logging(valid_ts, "test");

        assert_eq!(result.year(), 2024);
        assert_eq!(result.month(), 1);
        assert_eq!(result.day(), 15);
        assert_eq!(result.hour(), 10);
        assert_eq!(result.minute(), 30);
    }

    #[test]
    fn test_parse_timestamp_with_offset() {
        let ts_with_offset = "2024-06-20T15:45:30+05:00";
        let result = parse_timestamp_with_logging(ts_with_offset, "test");

        // Should convert to UTC
        assert_eq!(result.hour(), 10); // 15:45 +05:00 = 10:45 UTC
        assert_eq!(result.minute(), 45);
    }

    #[test]
    fn test_parse_timestamp_invalid_returns_now() {
        let invalid_ts = "not-a-timestamp";
        let before = chrono::Utc::now();
        let result = parse_timestamp_with_logging(invalid_ts, "test");
        let after = chrono::Utc::now();

        // Should return current time on invalid input
        assert!(result >= before);
        assert!(result <= after);
    }

    #[test]
    fn test_parse_timestamp_empty_string() {
        let empty = "";
        let before = chrono::Utc::now();
        let result = parse_timestamp_with_logging(empty, "test");
        let after = chrono::Utc::now();

        assert!(result >= before);
        assert!(result <= after);
    }

    #[test]
    fn test_parse_metadata_valid_json() {
        let json = r#"{"key": "value", "number": 42}"#;
        let result = parse_metadata_with_logging(json, "test");

        assert!(result.is_some());
        let value = result.unwrap();
        assert_eq!(value["key"], "value");
        assert_eq!(value["number"], 42);
    }

    #[test]
    fn test_parse_metadata_nested_json() {
        let json = r#"{"outer": {"inner": "deep"}, "array": [1, 2, 3]}"#;
        let result = parse_metadata_with_logging(json, "test");

        assert!(result.is_some());
        let value = result.unwrap();
        assert_eq!(value["outer"]["inner"], "deep");
        assert_eq!(value["array"][0], 1);
    }

    #[test]
    fn test_parse_metadata_invalid_json() {
        let invalid = "{ invalid json }";
        let result = parse_metadata_with_logging(invalid, "test");

        assert!(result.is_none());
    }

    #[test]
    fn test_parse_metadata_empty_string() {
        let empty = "";
        let result = parse_metadata_with_logging(empty, "test");

        assert!(result.is_none());
    }

    #[test]
    fn test_parse_metadata_empty_object() {
        let empty_obj = "{}";
        let result = parse_metadata_with_logging(empty_obj, "test");

        assert!(result.is_some());
        let value = result.unwrap();
        assert!(value.is_object());
    }

    #[test]
    fn test_parse_metadata_null() {
        let null = "null";
        let result = parse_metadata_with_logging(null, "test");

        assert!(result.is_some());
        let value = result.unwrap();
        assert!(value.is_null());
    }

    #[test]
    fn test_parse_enum_valid_branch_state() {
        use super::super::BranchState;

        let result: BranchState = parse_enum_with_logging("active", "test");
        assert_eq!(result, BranchState::Active);

        let result: BranchState = parse_enum_with_logging("completed", "test");
        assert_eq!(result, BranchState::Completed);

        let result: BranchState = parse_enum_with_logging("abandoned", "test");
        assert_eq!(result, BranchState::Abandoned);
    }

    #[test]
    fn test_parse_enum_invalid_returns_default() {
        use super::super::BranchState;

        let result: BranchState = parse_enum_with_logging("invalid_state", "test");
        assert_eq!(result, BranchState::default());
    }

    #[test]
    fn test_parse_enum_empty_string() {
        use super::super::BranchState;

        let result: BranchState = parse_enum_with_logging("", "test");
        assert_eq!(result, BranchState::default());
    }

    #[test]
    fn test_parse_enum_node_type() {
        use super::super::NodeType;

        let result: NodeType = parse_enum_with_logging("root", "test");
        assert_eq!(result, NodeType::Root);

        let result: NodeType = parse_enum_with_logging("terminal", "test");
        assert_eq!(result, NodeType::Terminal);

        let result: NodeType = parse_enum_with_logging("thought", "test");
        assert_eq!(result, NodeType::Thought);
    }

    #[test]
    fn test_parse_enum_edge_type() {
        use super::super::EdgeType;

        let result: EdgeType = parse_enum_with_logging("generates", "test");
        assert_eq!(result, EdgeType::Generates);

        let result: EdgeType = parse_enum_with_logging("refines", "test");
        assert_eq!(result, EdgeType::Refines);

        let result: EdgeType = parse_enum_with_logging("aggregates", "test");
        assert_eq!(result, EdgeType::Aggregates);
    }

    #[test]
    fn test_parse_enum_snapshot_type() {
        use super::super::SnapshotType;

        let result: SnapshotType = parse_enum_with_logging("full", "test");
        assert_eq!(result, SnapshotType::Full);

        let result: SnapshotType = parse_enum_with_logging("branch", "test");
        assert_eq!(result, SnapshotType::Branch);

        let result: SnapshotType = parse_enum_with_logging("incremental", "test");
        assert_eq!(result, SnapshotType::Incremental);
    }

    #[test]
    fn test_parse_enum_cross_ref_type() {
        use super::super::CrossRefType;

        let result: CrossRefType = parse_enum_with_logging("supports", "test");
        assert_eq!(result, CrossRefType::Supports);

        let result: CrossRefType = parse_enum_with_logging("contradicts", "test");
        assert_eq!(result, CrossRefType::Contradicts);

        let result: CrossRefType = parse_enum_with_logging("extends", "test");
        assert_eq!(result, CrossRefType::Extends);
    }

    // ============================================================================
    // Row Conversion Tests
    // ============================================================================

    #[test]
    fn test_session_row_conversion() {
        let row = SessionRow {
            id: "sess-123".to_string(),
            mode: "linear".to_string(),
            created_at: "2024-01-15T10:00:00Z".to_string(),
            updated_at: "2024-01-15T11:00:00Z".to_string(),
            metadata: Some(r#"{"key": "value"}"#.to_string()),
            active_branch_id: Some("branch-1".to_string()),
        };

        let session: Session = row.into();
        assert_eq!(session.id, "sess-123");
        assert_eq!(session.mode, "linear");
        assert_eq!(session.active_branch_id, Some("branch-1".to_string()));
        assert!(session.metadata.is_some());
    }

    #[test]
    fn test_session_row_conversion_no_metadata() {
        let row = SessionRow {
            id: "sess-456".to_string(),
            mode: "tree".to_string(),
            created_at: "2024-01-15T10:00:00Z".to_string(),
            updated_at: "2024-01-15T11:00:00Z".to_string(),
            metadata: None,
            active_branch_id: None,
        };

        let session: Session = row.into();
        assert_eq!(session.id, "sess-456");
        assert!(session.metadata.is_none());
        assert!(session.active_branch_id.is_none());
    }

    #[test]
    fn test_thought_row_conversion() {
        let row = ThoughtRow {
            id: "thought-123".to_string(),
            session_id: "sess-1".to_string(),
            branch_id: Some("branch-1".to_string()),
            parent_id: None,
            content: "Test thought content".to_string(),
            mode: "linear".to_string(),
            confidence: 0.85,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            metadata: None,
        };

        let thought: Thought = row.into();
        assert_eq!(thought.id, "thought-123");
        assert_eq!(thought.content, "Test thought content");
        assert_eq!(thought.confidence, 0.85);
        assert_eq!(thought.branch_id, Some("branch-1".to_string()));
    }

    #[test]
    fn test_branch_row_conversion() {
        let row = BranchRow {
            id: "branch-123".to_string(),
            session_id: "sess-1".to_string(),
            name: Some("Main branch".to_string()),
            parent_branch_id: None,
            state: "active".to_string(),
            confidence: 0.9,
            priority: 1.0,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            updated_at: "2024-01-15T11:00:00Z".to_string(),
            metadata: None,
        };

        let branch: Branch = row.into();
        assert_eq!(branch.id, "branch-123");
        assert_eq!(branch.name, Some("Main branch".to_string()));
        assert_eq!(branch.state, super::super::BranchState::Active);
        assert_eq!(branch.confidence, 0.9);
    }

    #[test]
    fn test_cross_ref_row_conversion() {
        let row = CrossRefRow {
            id: "xref-123".to_string(),
            from_branch_id: "branch-1".to_string(),
            to_branch_id: "branch-2".to_string(),
            ref_type: "supports".to_string(),
            reason: Some("Related concept".to_string()),
            strength: 0.75,
            created_at: "2024-01-15T10:00:00Z".to_string(),
        };

        let cross_ref: CrossRef = row.into();
        assert_eq!(cross_ref.id, "xref-123");
        assert_eq!(cross_ref.from_branch_id, "branch-1");
        assert_eq!(cross_ref.to_branch_id, "branch-2");
        assert_eq!(cross_ref.ref_type, super::super::CrossRefType::Supports);
        assert_eq!(cross_ref.strength, 0.75);
    }

    #[test]
    fn test_checkpoint_row_conversion() {
        let row = CheckpointRow {
            id: "cp-123".to_string(),
            session_id: "sess-1".to_string(),
            branch_id: Some("branch-1".to_string()),
            name: "Checkpoint Alpha".to_string(),
            description: Some("First checkpoint".to_string()),
            snapshot: r#"{"thoughts": []}"#.to_string(),
            created_at: "2024-01-15T10:00:00Z".to_string(),
        };

        let checkpoint: Checkpoint = row.into();
        assert_eq!(checkpoint.id, "cp-123");
        assert_eq!(checkpoint.name, "Checkpoint Alpha");
        assert_eq!(checkpoint.description, Some("First checkpoint".to_string()));
        assert!(checkpoint.snapshot.is_object());
    }

    #[test]
    fn test_checkpoint_row_conversion_invalid_snapshot() {
        let row = CheckpointRow {
            id: "cp-456".to_string(),
            session_id: "sess-1".to_string(),
            branch_id: None,
            name: "Bad checkpoint".to_string(),
            description: None,
            snapshot: "{ invalid json }".to_string(),
            created_at: "2024-01-15T10:00:00Z".to_string(),
        };

        let checkpoint: Checkpoint = row.into();
        // Should fall back to null on invalid JSON
        assert!(checkpoint.snapshot.is_null());
    }

    #[test]
    fn test_graph_node_row_conversion() {
        let row = GraphNodeRow {
            id: "node-123".to_string(),
            session_id: "sess-1".to_string(),
            content: "Node content".to_string(),
            node_type: "root".to_string(),
            score: Some(0.85),
            depth: 0,
            is_terminal: false,
            is_root: true,
            is_active: true,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            metadata: None,
        };

        let node: GraphNode = row.into();
        assert_eq!(node.id, "node-123");
        assert_eq!(node.content, "Node content");
        assert_eq!(node.node_type, super::super::NodeType::Root);
        assert_eq!(node.score, Some(0.85));
        assert!(node.is_root);
    }

    #[test]
    fn test_graph_edge_row_conversion() {
        let row = GraphEdgeRow {
            id: "edge-123".to_string(),
            session_id: "sess-1".to_string(),
            from_node: "node-1".to_string(),
            to_node: "node-2".to_string(),
            edge_type: "generates".to_string(),
            weight: 0.9,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            metadata: Some(r#"{"label": "generates"}"#.to_string()),
        };

        let edge: GraphEdge = row.into();
        assert_eq!(edge.id, "edge-123");
        assert_eq!(edge.from_node, "node-1");
        assert_eq!(edge.to_node, "node-2");
        assert_eq!(edge.edge_type, super::super::EdgeType::Generates);
        assert!(edge.metadata.is_some());
    }

    #[test]
    fn test_state_snapshot_row_conversion() {
        let row = StateSnapshotRow {
            id: "snap-123".to_string(),
            session_id: "sess-1".to_string(),
            snapshot_type: "full".to_string(),
            state_data: r#"{"key": "value"}"#.to_string(),
            parent_snapshot_id: Some("snap-122".to_string()),
            created_at: "2024-01-15T10:00:00Z".to_string(),
            description: Some("State snapshot".to_string()),
        };

        let snapshot: StateSnapshot = row.into();
        assert_eq!(snapshot.id, "snap-123");
        assert_eq!(snapshot.snapshot_type, super::super::SnapshotType::Full);
        assert_eq!(snapshot.parent_snapshot_id, Some("snap-122".to_string()));
        assert!(snapshot.state_data.is_object());
    }

    #[test]
    fn test_state_snapshot_row_invalid_state_data() {
        let row = StateSnapshotRow {
            id: "snap-456".to_string(),
            session_id: "sess-1".to_string(),
            snapshot_type: "incremental".to_string(),
            state_data: "not valid json".to_string(),
            parent_snapshot_id: None,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            description: None,
        };

        let snapshot: StateSnapshot = row.into();
        // Should fall back to null
        assert!(snapshot.state_data.is_null());
    }

    // ============================================================================
    // Async Integration Tests with In-Memory SQLite
    // ============================================================================

    #[tokio::test]
    async fn test_sqlite_storage_new_in_memory() {
        let storage = SqliteStorage::new_in_memory().await;
        assert!(storage.is_ok());
    }

    #[tokio::test]
    async fn test_session_crud_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session
        let session = Session::new("tree");
        let create_result = storage.create_session(&session).await;
        assert!(
            create_result.is_ok(),
            "Failed to create session: {:?}",
            create_result.err()
        );

        // Get session
        let retrieved = storage.get_session(&session.id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.mode, "tree");

        // Create a branch first (active_branch_id has FK constraint)
        let branch = Branch::new(&session.id).with_name("main");
        storage.create_branch(&branch).await.unwrap();

        // Update session with the branch reference
        let mut updated_session = retrieved.clone();
        updated_session.active_branch_id = Some(branch.id.clone());
        updated_session.updated_at = chrono::Utc::now();
        let update_result = storage.update_session(&updated_session).await;
        assert!(
            update_result.is_ok(),
            "Failed to update session: {:?}",
            update_result.err()
        );

        // Verify update
        let after_update = storage.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(after_update.active_branch_id, Some(branch.id));
    }

    #[tokio::test]
    async fn test_thought_crud_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session first
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        // Create thought
        let thought =
            Thought::new(&session.id, "Test thought content", "linear").with_confidence(0.85);
        let create_result = storage.create_thought(&thought).await;
        assert!(create_result.is_ok());

        // Get thought
        let retrieved = storage.get_thought(&thought.id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, "Test thought content");
        assert_eq!(retrieved.confidence, 0.85);

        // Get session thoughts
        let thoughts = storage.get_session_thoughts(&session.id).await.unwrap();
        assert_eq!(thoughts.len(), 1);
        assert_eq!(thoughts[0].id, thought.id);
    }

    #[tokio::test]
    async fn test_branch_crud_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session
        let session = Session::new("tree");
        storage.create_session(&session).await.unwrap();

        // Create branch
        let branch = Branch::new(&session.id)
            .with_name("main-branch")
            .with_confidence(0.9);
        let create_result = storage.create_branch(&branch).await;
        assert!(create_result.is_ok());

        // Get branch
        let retrieved = storage.get_branch(&branch.id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, Some("main-branch".to_string()));

        // List session branches
        let branches = storage.get_session_branches(&session.id).await.unwrap();
        assert_eq!(branches.len(), 1);

        // Update branch
        let mut updated = retrieved.clone();
        updated.state = BranchState::Completed;
        storage.update_branch(&updated).await.unwrap();

        let after_update = storage.get_branch(&branch.id).await.unwrap().unwrap();
        assert_eq!(after_update.state, BranchState::Completed);
    }

    #[tokio::test]
    async fn test_checkpoint_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session
        let session = Session::new("backtracking");
        storage.create_session(&session).await.unwrap();

        // Create checkpoint
        let checkpoint = Checkpoint::new(
            &session.id,
            "test-checkpoint",
            serde_json::json!({"state": "saved"}),
        )
        .with_description("A test checkpoint");
        let create_result = storage.create_checkpoint(&checkpoint).await;
        assert!(create_result.is_ok());

        // Get checkpoint
        let retrieved = storage.get_checkpoint(&checkpoint.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-checkpoint");

        // List checkpoints
        let checkpoints = storage.get_session_checkpoints(&session.id).await.unwrap();
        assert_eq!(checkpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_invocation_logging() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create invocation (session_id is optional, so no need to create session first)
        let invocation =
            Invocation::new("reasoning.linear", serde_json::json!({"content": "test"}))
                .with_pipe("linear-v1");
        let result = storage.log_invocation(&invocation).await;
        assert!(
            result.is_ok(),
            "Failed to log invocation: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_graph_node_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session
        let session = Session::new("got");
        storage.create_session(&session).await.unwrap();

        // Create graph node
        let node = GraphNode::new(&session.id, "Root thought content")
            .as_root()
            .with_score(0.9);
        let create_result = storage.create_graph_node(&node).await;
        assert!(create_result.is_ok());

        // Get node
        let retrieved = storage.get_graph_node(&node.id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!(retrieved.is_root);
        assert_eq!(retrieved.score, Some(0.9));

        // Get session nodes
        let nodes = storage.get_session_graph_nodes(&session.id).await.unwrap();
        assert_eq!(nodes.len(), 1);

        // Get active nodes
        let active = storage.get_active_graph_nodes(&session.id).await.unwrap();
        assert_eq!(active.len(), 1);

        // Update node
        let mut updated = retrieved.clone();
        updated.is_terminal = true;
        updated.is_active = false;
        storage.update_graph_node(&updated).await.unwrap();

        let after_update = storage.get_graph_node(&node.id).await.unwrap().unwrap();
        assert!(after_update.is_terminal);
        assert!(!after_update.is_active);
    }

    #[tokio::test]
    async fn test_graph_edge_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session and nodes
        let session = Session::new("got");
        storage.create_session(&session).await.unwrap();

        let node1 = GraphNode::new(&session.id, "Node 1").as_root();
        let node2 = GraphNode::new(&session.id, "Node 2");
        storage.create_graph_node(&node1).await.unwrap();
        storage.create_graph_node(&node2).await.unwrap();

        // Create edge
        let edge = GraphEdge::new(&session.id, &node1.id, &node2.id)
            .with_weight(0.8)
            .with_type(EdgeType::Generates);
        let create_result = storage.create_graph_edge(&edge).await;
        assert!(create_result.is_ok());

        // Get session edges
        let edges = storage.get_session_edges(&session.id).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_node, node1.id);
        assert_eq!(edges[0].to_node, node2.id);
    }

    #[tokio::test]
    async fn test_cross_ref_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session
        let session = Session::new("tree");
        storage.create_session(&session).await.unwrap();

        // Create branches
        let branch1 = Branch::new(&session.id).with_name("branch-1");
        let branch2 = Branch::new(&session.id).with_name("branch-2");
        storage.create_branch(&branch1).await.unwrap();
        storage.create_branch(&branch2).await.unwrap();

        // Create cross-reference
        let cross_ref = CrossRef::new(&branch1.id, &branch2.id, CrossRefType::Supports)
            .with_reason("Related concepts")
            .with_strength(0.8);
        let create_result = storage.create_cross_ref(&cross_ref).await;
        assert!(create_result.is_ok());

        // Get branch cross-refs
        let refs = storage.get_cross_refs_from(&branch1.id).await.unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_branch_id, branch2.id);
    }

    #[tokio::test]
    async fn test_state_snapshot_operations() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Create session
        let session = Session::new("backtracking");
        storage.create_session(&session).await.unwrap();

        // Create snapshot
        let snapshot = StateSnapshot::new(&session.id, serde_json::json!({"state": "saved"}))
            .with_description("Test snapshot");
        let create_result = storage.create_snapshot(&snapshot).await;
        assert!(create_result.is_ok());

        // Get snapshot
        let retrieved = storage.get_snapshot(&snapshot.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().description,
            Some("Test snapshot".to_string())
        );

        // Get latest
        let latest = storage.get_latest_snapshot(&session.id).await.unwrap();
        assert!(latest.is_some());
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let result = storage.get_session("nonexistent-id").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_thought_not_found() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let result = storage.get_thought("nonexistent-id").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_branch_not_found() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let result = storage.get_branch("nonexistent-id").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_multiple_thoughts_ordering() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        // Create multiple thoughts
        let thought1 = Thought::new(&session.id, "First thought", "linear");
        let thought2 = Thought::new(&session.id, "Second thought", "linear");
        let thought3 = Thought::new(&session.id, "Third thought", "linear");

        storage.create_thought(&thought1).await.unwrap();
        storage.create_thought(&thought2).await.unwrap();
        storage.create_thought(&thought3).await.unwrap();

        let thoughts = storage.get_session_thoughts(&session.id).await.unwrap();
        assert_eq!(thoughts.len(), 3);
    }

    #[tokio::test]
    async fn test_thought_with_parent() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let parent = Thought::new(&session.id, "Parent thought", "linear");
        storage.create_thought(&parent).await.unwrap();

        let child = Thought::new(&session.id, "Child thought", "linear").with_parent(&parent.id);
        storage.create_thought(&child).await.unwrap();

        let retrieved = storage.get_thought(&child.id).await.unwrap().unwrap();
        assert_eq!(retrieved.parent_id, Some(parent.id));
    }

    #[tokio::test]
    async fn test_get_root_graph_nodes() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let session = Session::new("got");
        storage.create_session(&session).await.unwrap();

        let root1 = GraphNode::new(&session.id, "Root 1").as_root();
        let root2 = GraphNode::new(&session.id, "Root 2").as_root();
        let non_root = GraphNode::new(&session.id, "Non-root");

        storage.create_graph_node(&root1).await.unwrap();
        storage.create_graph_node(&root2).await.unwrap();
        storage.create_graph_node(&non_root).await.unwrap();

        let roots = storage.get_root_nodes(&session.id).await.unwrap();
        assert_eq!(roots.len(), 2);
    }

    #[tokio::test]
    async fn test_get_terminal_graph_nodes() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let session = Session::new("got");
        storage.create_session(&session).await.unwrap();

        let node1 = GraphNode::new(&session.id, "Normal node");
        let terminal = GraphNode::new(&session.id, "Terminal node").as_terminal();

        storage.create_graph_node(&node1).await.unwrap();
        storage.create_graph_node(&terminal).await.unwrap();

        let terminals = storage.get_terminal_nodes(&session.id).await.unwrap();
        assert_eq!(terminals.len(), 1);
        assert!(terminals[0].is_terminal);
    }

    // ============================================================================
    // Serialize JSON Helper Tests
    // ============================================================================

    #[test]
    fn test_serialize_json_with_some_value() {
        let data: Option<serde_json::Value> = Some(serde_json::json!({"key": "value"}));
        let result = super::serialize_json(&data, "test.field");

        assert!(result.is_ok());
        let serialized = result.unwrap();
        assert!(serialized.is_some());
        assert_eq!(serialized.unwrap(), r#"{"key":"value"}"#);
    }

    #[test]
    fn test_serialize_json_with_none() {
        let data: Option<serde_json::Value> = None;
        let result = super::serialize_json(&data, "test.field");

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_serialize_json_with_complex_object() {
        let data: Option<serde_json::Value> = Some(serde_json::json!({
            "nested": {"inner": "value"},
            "array": [1, 2, 3],
            "number": 42.5,
            "boolean": true
        }));
        let result = super::serialize_json(&data, "test.complex");

        assert!(result.is_ok());
        let serialized = result.unwrap().unwrap();
        assert!(serialized.contains("nested"));
        assert!(serialized.contains("array"));
    }

    #[test]
    fn test_serialize_json_required_success() {
        let data = serde_json::json!({"required": "data"});
        let result = super::serialize_json_required(&data, "test.required");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), r#"{"required":"data"}"#);
    }

    #[test]
    fn test_serialize_json_required_empty_object() {
        let data = serde_json::json!({});
        let result = super::serialize_json_required(&data, "test.empty");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "{}");
    }

    #[test]
    fn test_serialize_json_required_null() {
        let data = serde_json::Value::Null;
        let result = super::serialize_json_required(&data, "test.null");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "null");
    }
}
