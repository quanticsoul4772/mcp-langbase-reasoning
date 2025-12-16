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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_display() {
        let err = AppError::Config {
            message: "missing key".to_string(),
        };
        assert_eq!(err.to_string(), "Configuration error: missing key");

        let err = AppError::Internal {
            message: "unexpected".to_string(),
        };
        assert_eq!(err.to_string(), "Internal error: unexpected");
    }

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::Connection {
            message: "failed to connect".to_string(),
        };
        assert_eq!(err.to_string(), "Database connection failed: failed to connect");

        let err = StorageError::SessionNotFound {
            session_id: "sess-123".to_string(),
        };
        assert_eq!(err.to_string(), "Session not found: sess-123");

        let err = StorageError::ThoughtNotFound {
            thought_id: "thought-456".to_string(),
        };
        assert_eq!(err.to_string(), "Thought not found: thought-456");

        let err = StorageError::Query {
            message: "syntax error".to_string(),
        };
        assert_eq!(err.to_string(), "Query failed: syntax error");

        let err = StorageError::Migration {
            message: "version mismatch".to_string(),
        };
        assert_eq!(err.to_string(), "Migration failed: version mismatch");
    }

    #[test]
    fn test_langbase_error_display() {
        let err = LangbaseError::Unavailable {
            message: "server down".to_string(),
            retries: 3,
        };
        assert_eq!(err.to_string(), "Langbase unavailable: server down (retries: 3)");

        let err = LangbaseError::Api {
            status: 401,
            message: "unauthorized".to_string(),
        };
        assert_eq!(err.to_string(), "API error: 401 - unauthorized");

        let err = LangbaseError::InvalidResponse {
            message: "malformed JSON".to_string(),
        };
        assert_eq!(err.to_string(), "Invalid response: malformed JSON");

        let err = LangbaseError::Timeout { timeout_ms: 5000 };
        assert_eq!(err.to_string(), "Request timeout after 5000ms");
    }

    #[test]
    fn test_mcp_error_display() {
        let err = McpError::InvalidRequest {
            message: "bad format".to_string(),
        };
        assert_eq!(err.to_string(), "Invalid request: bad format");

        let err = McpError::UnknownTool {
            tool_name: "nonexistent".to_string(),
        };
        assert_eq!(err.to_string(), "Unknown tool: nonexistent");

        let err = McpError::InvalidParameters {
            tool_name: "reasoning.linear".to_string(),
            message: "missing content".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid parameters for reasoning.linear: missing content"
        );

        let err = McpError::ExecutionFailed {
            message: "pipe failed".to_string(),
        };
        assert_eq!(err.to_string(), "Tool execution failed: pipe failed");
    }

    #[test]
    fn test_tool_error_display() {
        let err = ToolError::Validation {
            field: "content".to_string(),
            reason: "cannot be empty".to_string(),
        };
        assert_eq!(err.to_string(), "Validation failed: content - cannot be empty");

        let err = ToolError::Session("not found".to_string());
        assert_eq!(err.to_string(), "Session error: not found");

        let err = ToolError::Reasoning {
            message: "logic error".to_string(),
        };
        assert_eq!(err.to_string(), "Reasoning failed: logic error");
    }

    #[test]
    fn test_tool_error_conversion_to_app_error() {
        let tool_err = ToolError::Validation {
            field: "test".to_string(),
            reason: "invalid".to_string(),
        };
        let app_err: AppError = tool_err.into();
        assert!(matches!(app_err, AppError::Internal { .. }));
        assert!(app_err.to_string().contains("Validation failed"));
    }

    #[test]
    fn test_app_error_conversion_to_mcp_error() {
        let app_err = AppError::Config {
            message: "test error".to_string(),
        };
        let mcp_err: McpError = app_err.into();
        assert!(matches!(mcp_err, McpError::ExecutionFailed { .. }));
        assert!(mcp_err.to_string().contains("Configuration error"));
    }

    #[test]
    fn test_storage_error_conversion_to_app_error() {
        let storage_err = StorageError::SessionNotFound {
            session_id: "test-123".to_string(),
        };
        let app_err: AppError = storage_err.into();
        assert!(matches!(app_err, AppError::Storage(_)));
    }

    #[test]
    fn test_langbase_error_conversion_to_app_error() {
        let langbase_err = LangbaseError::Timeout { timeout_ms: 1000 };
        let app_err: AppError = langbase_err.into();
        assert!(matches!(app_err, AppError::Langbase(_)));
    }

    #[test]
    fn test_mcp_error_conversion_to_app_error() {
        let mcp_err = McpError::UnknownTool {
            tool_name: "test".to_string(),
        };
        let app_err: AppError = mcp_err.into();
        assert!(matches!(app_err, AppError::Mcp(_)));
    }
}
