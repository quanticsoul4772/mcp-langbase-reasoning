use thiserror::Error;

/// Application-level errors
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Configuration error: {message}")]
    Config { message: String },

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Langbase error: {0}")]
    Langbase(#[from] LangbaseError),

    #[error("MCP protocol error: {0}")]
    Mcp(#[from] McpError),

    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Storage layer errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database connection failed: {message}")]
    Connection { message: String },

    #[error("Query failed: {message}")]
    Query { message: String },

    #[error("Session not found: {session_id}")]
    SessionNotFound { session_id: String },

    #[error("Thought not found: {thought_id}")]
    ThoughtNotFound { thought_id: String },

    #[error("Migration failed: {message}")]
    Migration { message: String },

    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

/// Langbase API errors
#[derive(Debug, Error)]
pub enum LangbaseError {
    #[error("Langbase unavailable: {message} (retries: {retries})")]
    Unavailable { message: String, retries: u32 },

    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    #[error("Invalid response: {message}")]
    InvalidResponse { message: String },

    #[error("Request timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// MCP protocol errors
#[derive(Debug, Error)]
pub enum McpError {
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("Unknown tool: {tool_name}")]
    UnknownTool { tool_name: String },

    #[error("Invalid parameters for {tool_name}: {message}")]
    InvalidParameters { tool_name: String, message: String },

    #[error("Tool execution failed: {message}")]
    ExecutionFailed { message: String },

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Tool-specific errors with structured details
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Validation failed: {field} - {reason}")]
    Validation { field: String, reason: String },

    #[error("Session error: {0}")]
    Session(String),

    #[error("Reasoning failed: {message}")]
    Reasoning { message: String },
}

impl From<ToolError> for AppError {
    fn from(err: ToolError) -> Self {
        AppError::Internal {
            message: err.to_string(),
        }
    }
}

impl From<AppError> for McpError {
    fn from(err: AppError) -> Self {
        McpError::ExecutionFailed {
            message: err.to_string(),
        }
    }
}

/// Result type alias for application errors
pub type AppResult<T> = Result<T, AppError>;

/// Result type alias for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Result type alias for Langbase operations
pub type LangbaseResult<T> = Result<T, LangbaseError>;

/// Result type alias for MCP operations
pub type McpResult<T> = Result<T, McpError>;
