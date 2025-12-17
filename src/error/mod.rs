//! Error types and result aliases for the application.
//!
//! This module provides a hierarchy of error types for different subsystems:
//! - [`AppError`]: Top-level application errors
//! - [`StorageError`]: Database and persistence errors
//! - [`LangbaseError`]: Langbase API communication errors
//! - [`McpError`]: MCP protocol errors
//! - [`ToolError`]: Tool-specific execution errors

use thiserror::Error;

/// Application-level errors encompassing all subsystem errors.
#[derive(Debug, Error)]
pub enum AppError {
    /// Configuration-related error.
    #[error("Configuration error: {message}")]
    Config {
        /// Error message describing the configuration issue.
        message: String,
    },

    /// Storage layer error.
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Langbase API error.
    #[error("Langbase error: {0}")]
    Langbase(#[from] LangbaseError),

    /// MCP protocol error.
    #[error("MCP protocol error: {0}")]
    Mcp(#[from] McpError),

    /// Internal application error.
    #[error("Internal error: {message}")]
    Internal {
        /// Error message describing the internal issue.
        message: String,
    },
}

/// Storage layer errors for database operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Database connection failed.
    #[error("Database connection failed: {message}")]
    Connection {
        /// Error message describing the connection issue.
        message: String,
    },

    /// Database query failed.
    #[error("Query failed: {message}")]
    Query {
        /// Error message describing the query issue.
        message: String,
    },

    /// Session not found in storage.
    #[error("Session not found: {session_id}")]
    SessionNotFound {
        /// ID of the missing session.
        session_id: String,
    },

    /// Thought not found in storage.
    #[error("Thought not found: {thought_id}")]
    ThoughtNotFound {
        /// ID of the missing thought.
        thought_id: String,
    },

    /// Database migration failed.
    #[error("Migration failed: {message}")]
    Migration {
        /// Error message describing the migration issue.
        message: String,
    },

    /// Underlying SQLx error.
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

/// Langbase API errors for pipe communication.
#[derive(Debug, Error)]
pub enum LangbaseError {
    /// Langbase service unavailable after retries.
    #[error("Langbase unavailable: {message} (retries: {retries})")]
    Unavailable {
        /// Error message from the service.
        message: String,
        /// Number of retry attempts made.
        retries: u32,
    },

    /// API returned an error status.
    #[error("API error: {status} - {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message from the API.
        message: String,
    },

    /// Invalid response from the API.
    #[error("Invalid response: {message}")]
    InvalidResponse {
        /// Description of the response issue.
        message: String,
    },

    /// Request timed out.
    #[error("Request timeout after {timeout_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
    },

    /// Underlying HTTP error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// MCP protocol errors for request handling.
#[derive(Debug, Error)]
pub enum McpError {
    /// Invalid MCP request format.
    #[error("Invalid request: {message}")]
    InvalidRequest {
        /// Description of the request issue.
        message: String,
    },

    /// Requested tool not found.
    #[error("Unknown tool: {tool_name}")]
    UnknownTool {
        /// Name of the unknown tool.
        tool_name: String,
    },

    /// Invalid parameters for a tool.
    #[error("Invalid parameters for {tool_name}: {message}")]
    InvalidParameters {
        /// Name of the tool with invalid parameters.
        tool_name: String,
        /// Description of the parameter issue.
        message: String,
    },

    /// Tool execution failed.
    #[error("Tool execution failed: {message}")]
    ExecutionFailed {
        /// Description of the execution failure.
        message: String,
    },

    /// JSON serialization/deserialization error.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Tool-specific errors with structured details.
#[derive(Debug, Error)]
pub enum ToolError {
    /// Input validation failed.
    #[error("Validation failed: {field} - {reason}")]
    Validation {
        /// Name of the invalid field.
        field: String,
        /// Reason for validation failure.
        reason: String,
    },

    /// Session-related error.
    #[error("Session error: {0}")]
    Session(String),

    /// Reasoning operation failed.
    #[error("Reasoning failed: {message}")]
    Reasoning {
        /// Description of the reasoning failure.
        message: String,
    },
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
        assert_eq!(
            err.to_string(),
            "Database connection failed: failed to connect"
        );

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
        assert_eq!(
            err.to_string(),
            "Langbase unavailable: server down (retries: 3)"
        );

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
        assert_eq!(
            err.to_string(),
            "Validation failed: content - cannot be empty"
        );

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
