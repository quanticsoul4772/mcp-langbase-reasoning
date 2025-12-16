use async_trait::async_trait;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tracing::info;

use super::{Invocation, Session, Storage, Thought};
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
            INSERT INTO sessions (id, mode, created_at, updated_at, metadata)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.mode)
        .bind(session.created_at.to_rfc3339())
        .bind(session.updated_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>> {
        let row: Option<SessionRow> = sqlx::query_as(
            r#"
            SELECT id, mode, created_at, updated_at, metadata
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
            SET mode = ?, updated_at = ?, metadata = ?
            WHERE id = ?
            "#,
        )
        .bind(&session.mode)
        .bind(session.updated_at.to_rfc3339())
        .bind(&metadata)
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
            INSERT INTO thoughts (id, session_id, content, confidence, mode, parent_id, created_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&thought.id)
        .bind(&thought.session_id)
        .bind(&thought.content)
        .bind(thought.confidence)
        .bind(&thought.mode)
        .bind(&thought.parent_id)
        .bind(thought.created_at.to_rfc3339())
        .bind(&metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>> {
        let row: Option<ThoughtRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, confidence, mode, parent_id, created_at, metadata
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
            SELECT id, session_id, content, confidence, mode, parent_id, created_at, metadata
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

    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>> {
        let row: Option<ThoughtRow> = sqlx::query_as(
            r#"
            SELECT id, session_id, content, confidence, mode, parent_id, created_at, metadata
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
}

// Internal row types for SQLx mapping
#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    mode: String,
    created_at: String,
    updated_at: String,
    metadata: Option<String>,
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        use chrono::DateTime;

        Self {
            id: row.id,
            mode: row.mode,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            metadata: row.metadata.and_then(|s| serde_json::from_str(&s).ok()),
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
    created_at: String,
    metadata: Option<String>,
}

impl From<ThoughtRow> for Thought {
    fn from(row: ThoughtRow) -> Self {
        use chrono::DateTime;

        Self {
            id: row.id,
            session_id: row.session_id,
            content: row.content,
            confidence: row.confidence,
            mode: row.mode,
            parent_id: row.parent_id,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            metadata: row.metadata.and_then(|s| serde_json::from_str(&s).ok()),
        }
    }
}
