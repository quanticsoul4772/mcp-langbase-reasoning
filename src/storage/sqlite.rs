use async_trait::async_trait;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tracing::{info, warn};

use super::{
    Branch, Checkpoint, CrossRef, GraphEdge, GraphNode, Invocation, Session, StateSnapshot,
    Storage, Thought,
};
use crate::config::DatabaseConfig;
use crate::error::{StorageError, StorageResult};

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

        MIGRATOR.run(&self.pool).await.map_err(|e| StorageError::Migration {
            message: format!("Failed to run migrations: {}", e),
        })?;

        info!("Database migrations completed successfully");
        Ok(())
    }

    /// Get the underlying pool for advanced queries
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn create_session(&self, session: &Session) -> StorageResult<()> {
        let metadata = session
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let metadata = session
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let metadata = thought
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let input = serde_json::to_string(&invocation.input).unwrap_or_default();
        let output = invocation
            .output
            .as_ref()
            .map(|o| serde_json::to_string(o).unwrap_or_default());

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
        let metadata = branch
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let metadata = branch
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let snapshot = serde_json::to_string(&checkpoint.snapshot).unwrap_or_default();

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
        let metadata = node
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let metadata = node
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let metadata = edge
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

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
        let state_data = serde_json::to_string(&snapshot.state_data).unwrap_or_default();

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
fn parse_enum_with_logging<T: std::str::FromStr + Default>(
    value: &str,
    context: &str,
) -> T {
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
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("session {} created_at", row.id)),
            updated_at: parse_timestamp_with_logging(&row.updated_at, &format!("session {} updated_at", row.id)),
            metadata: row.metadata.and_then(|s| parse_metadata_with_logging(&s, &format!("session {} metadata", row.id))),
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
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("thought {} created_at", row.id)),
            metadata: row.metadata.and_then(|s| parse_metadata_with_logging(&s, &format!("thought {} metadata", row.id))),
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
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("branch {} created_at", row.id)),
            updated_at: parse_timestamp_with_logging(&row.updated_at, &format!("branch {} updated_at", row.id)),
            metadata: row.metadata.and_then(|s| parse_metadata_with_logging(&s, &format!("branch {} metadata", row.id))),
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
            ref_type: parse_enum_with_logging(&row.ref_type, &format!("cross_ref {} ref_type", row.id)),
            reason: row.reason,
            strength: row.strength,
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("cross_ref {} created_at", row.id)),
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
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("checkpoint {} created_at", row.id)),
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
            node_type: parse_enum_with_logging(&row.node_type, &format!("graph_node {} node_type", row.id)),
            score: row.score,
            depth: row.depth,
            is_terminal: row.is_terminal,
            is_root: row.is_root,
            is_active: row.is_active,
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("graph_node {} created_at", row.id)),
            metadata: row.metadata.and_then(|s| parse_metadata_with_logging(&s, &format!("graph_node {} metadata", row.id))),
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
            edge_type: parse_enum_with_logging(&row.edge_type, &format!("graph_edge {} edge_type", row.id)),
            weight: row.weight,
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("graph_edge {} created_at", row.id)),
            metadata: row.metadata.and_then(|s| parse_metadata_with_logging(&s, &format!("graph_edge {} metadata", row.id))),
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
            snapshot_type: parse_enum_with_logging(&row.snapshot_type, &format!("state_snapshot {} snapshot_type", row.id)),
            state_data,
            parent_snapshot_id: row.parent_snapshot_id,
            created_at: parse_timestamp_with_logging(&row.created_at, &format!("state_snapshot {} created_at", row.id)),
            description: row.description,
        }
    }
}
