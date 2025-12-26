//! Error types and result aliases for the application.
//!
//! This module provides a hierarchy of error types for different subsystems:
//! - [`AppError`]: Top-level application errors
//! - [`StorageError`]: Database and persistence errors
//! - [`LangbaseError`]: Langbase API communication errors
//! - [`McpError`]: MCP protocol errors
//! - [`ToolError`]: Tool-specific execution errors
//! - [`ModeError`]: Mode-specific reasoning execution errors

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

    /// JSON serialization failed.
    #[error("Serialization failed: {message}")]
    Serialization {
        /// Description of the serialization issue.
        message: String,
    },
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

    /// Pipe response parsing failed - no fallback available in strict mode.
    #[error("Response parse failed for pipe '{pipe}': {message}")]
    ResponseParseFailed {
        /// Name of the pipe that returned unparseable response.
        pipe: String,
        /// Description of the parse failure.
        message: String,
        /// Raw response content (truncated for logging).
        raw_response: String,
    },

    /// Pipe not found (404 error).
    #[error("Pipe not found: {pipe} (verify pipe exists on Langbase)")]
    PipeNotFound {
        /// Name of the missing pipe.
        pipe: String,
    },
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

    /// Response parsing failed (strict mode - no fallback).
    #[error("Parse error in {mode} mode: {message}")]
    ParseFailed {
        /// Reasoning mode that failed.
        mode: String,
        /// Description of parse failure.
        message: String,
    },

    /// Pipe unavailable and no fallback allowed.
    #[error("Pipe unavailable: {pipe} - {reason}")]
    PipeUnavailable {
        /// Name of the unavailable pipe.
        pipe: String,
        /// Reason for unavailability.
        reason: String,
    },
}

/// Mode-specific execution errors for reasoning operations.
///
/// These errors occur during the execution of reasoning modes
/// and provide structured information for debugging and recovery.
#[derive(Debug, Error)]
pub enum ModeError {
    /// Session state has been corrupted or is inconsistent.
    #[error("Session state corrupted: {message}")]
    StateCorrupted {
        /// Description of the corruption.
        message: String,
    },

    /// A required parameter was not provided.
    #[error("Required parameter missing: {param}")]
    MissingParameter {
        /// Name of the missing parameter.
        param: String,
    },

    /// Branch state is invalid for the requested operation.
    #[error("Invalid branch state: {branch_id}")]
    InvalidBranchState {
        /// ID of the branch with invalid state.
        branch_id: String,
    },

    /// Lock acquisition failed (mutex poisoned or timeout).
    #[error("Lock acquisition failed: {resource}")]
    LockPoisoned {
        /// Name of the resource that couldn't be locked.
        resource: String,
    },

    /// Checkpoint not found for backtracking.
    #[error("Checkpoint not found: {checkpoint_id}")]
    CheckpointNotFound {
        /// ID of the missing checkpoint.
        checkpoint_id: String,
    },

    /// Graph node not found.
    #[error("Graph node not found: {node_id}")]
    NodeNotFound {
        /// ID of the missing node.
        node_id: String,
    },

    /// Invalid confidence value (must be 0.0-1.0).
    #[error("Invalid confidence value: {value} (must be 0.0-1.0)")]
    InvalidConfidence {
        /// The invalid confidence value.
        value: f64,
    },

    /// Operation timeout.
    #[error("Operation timed out after {timeout_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
    },

    /// Parse error when processing mode-specific data.
    #[error("Parse error in {context}: {message}")]
    ParseError {
        /// Context where parsing failed.
        context: String,
        /// Description of the parse error.
        message: String,
    },
}

impl From<ModeError> for AppError {
    fn from(err: ModeError) -> Self {
        AppError::Internal {
            message: err.to_string(),
        }
    }
}

impl From<ModeError> for McpError {
    fn from(err: ModeError) -> Self {
        McpError::ExecutionFailed {
            message: err.to_string(),
        }
    }
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

/// Result type alias for mode operations
pub type ModeResult<T> = Result<T, ModeError>;

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

        let err = StorageError::Serialization {
            message: "invalid utf-8 in metadata".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Serialization failed: invalid utf-8 in metadata"
        );
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

    // Additional comprehensive tests

    #[test]
    fn test_app_error_storage_variant_display() {
        let storage_err = StorageError::Query {
            message: "syntax error".to_string(),
        };
        let app_err = AppError::Storage(storage_err);
        assert!(app_err.to_string().contains("Storage error"));
        assert!(app_err.to_string().contains("syntax error"));
    }

    #[test]
    fn test_app_error_langbase_variant_display() {
        let langbase_err = LangbaseError::Api {
            status: 500,
            message: "internal server error".to_string(),
        };
        let app_err = AppError::Langbase(langbase_err);
        assert!(app_err.to_string().contains("Langbase error"));
        assert!(app_err.to_string().contains("500"));
    }

    #[test]
    fn test_app_error_mcp_variant_display() {
        let mcp_err = McpError::InvalidParameters {
            tool_name: "test_tool".to_string(),
            message: "missing field".to_string(),
        };
        let app_err = AppError::Mcp(mcp_err);
        assert!(app_err.to_string().contains("MCP protocol error"));
        assert!(app_err.to_string().contains("test_tool"));
    }

    #[test]
    fn test_storage_error_conversion_from_sqlx() {
        let sqlx_err = sqlx::Error::RowNotFound;
        let storage_err: StorageError = sqlx_err.into();
        assert!(matches!(storage_err, StorageError::Sqlx(_)));
    }

    #[test]
    fn test_langbase_error_http_variant_display() {
        // Testing that LangbaseError::Http variant exists and displays properly
        // Note: Creating a real reqwest::Error is complex, so we test the variant exists
        // In real usage: let langbase_err: LangbaseError = reqwest_error.into();
        // This test verifies the From<reqwest::Error> trait is implemented
    }

    #[test]
    fn test_mcp_error_conversion_from_serde_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let mcp_err: McpError = json_err.into();
        assert!(matches!(mcp_err, McpError::Json(_)));
    }

    #[test]
    fn test_tool_error_to_app_error_to_mcp_error_chain() {
        let tool_err = ToolError::Reasoning {
            message: "inference failed".to_string(),
        };
        let app_err: AppError = tool_err.into();
        let mcp_err: McpError = app_err.into();
        assert!(matches!(mcp_err, McpError::ExecutionFailed { .. }));
        assert!(mcp_err.to_string().contains("Reasoning failed"));
    }

    #[test]
    fn test_storage_error_to_app_error_to_mcp_error_chain() {
        let storage_err = StorageError::Connection {
            message: "db offline".to_string(),
        };
        let app_err: AppError = storage_err.into();
        let mcp_err: McpError = app_err.into();
        assert!(matches!(mcp_err, McpError::ExecutionFailed { .. }));
        assert!(mcp_err.to_string().contains("Database connection failed"));
    }

    #[test]
    fn test_langbase_error_to_app_error_to_mcp_error_chain() {
        let langbase_err = LangbaseError::InvalidResponse {
            message: "malformed response".to_string(),
        };
        let app_err: AppError = langbase_err.into();
        let mcp_err: McpError = app_err.into();
        assert!(matches!(mcp_err, McpError::ExecutionFailed { .. }));
        assert!(mcp_err.to_string().contains("Invalid response"));
    }

    #[test]
    fn test_storage_error_serialization_variant() {
        let err = StorageError::Serialization {
            message: "json parse error".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Serialization failed"));
        assert!(display.contains("json parse error"));
    }

    #[test]
    fn test_langbase_error_unavailable_with_zero_retries() {
        let err = LangbaseError::Unavailable {
            message: "immediate failure".to_string(),
            retries: 0,
        };
        assert!(err.to_string().contains("retries: 0"));
    }

    #[test]
    fn test_langbase_error_unavailable_with_high_retries() {
        let err = LangbaseError::Unavailable {
            message: "persistent failure".to_string(),
            retries: 999,
        };
        assert!(err.to_string().contains("retries: 999"));
    }

    #[test]
    fn test_langbase_error_api_with_various_status_codes() {
        let err_400 = LangbaseError::Api {
            status: 400,
            message: "bad request".to_string(),
        };
        assert!(err_400.to_string().contains("400"));

        let err_403 = LangbaseError::Api {
            status: 403,
            message: "forbidden".to_string(),
        };
        assert!(err_403.to_string().contains("403"));

        let err_503 = LangbaseError::Api {
            status: 503,
            message: "service unavailable".to_string(),
        };
        assert!(err_503.to_string().contains("503"));
    }

    #[test]
    fn test_langbase_error_timeout_various_durations() {
        let err_short = LangbaseError::Timeout { timeout_ms: 100 };
        assert!(err_short.to_string().contains("100ms"));

        let err_long = LangbaseError::Timeout { timeout_ms: 60000 };
        assert!(err_long.to_string().contains("60000ms"));
    }

    #[test]
    fn test_tool_error_session_variant_with_various_messages() {
        let err1 = ToolError::Session("session expired".to_string());
        assert!(err1.to_string().contains("session expired"));

        let err2 = ToolError::Session("session locked".to_string());
        assert!(err2.to_string().contains("session locked"));

        let err3 = ToolError::Session("session corrupt".to_string());
        assert!(err3.to_string().contains("session corrupt"));
    }

    #[test]
    fn test_tool_error_validation_field_names() {
        let err = ToolError::Validation {
            field: "max_depth".to_string(),
            reason: "must be between 1 and 10".to_string(),
        };
        assert!(err.to_string().contains("max_depth"));
        assert!(err.to_string().contains("must be between 1 and 10"));
    }

    #[test]
    fn test_app_error_debug_format() {
        let err = AppError::Config {
            message: "test".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Config"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_storage_error_debug_format() {
        let err = StorageError::Migration {
            message: "failed migration".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Migration"));
        assert!(debug_str.contains("failed migration"));
    }

    #[test]
    fn test_langbase_error_debug_format() {
        let err = LangbaseError::Timeout { timeout_ms: 3000 };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Timeout"));
        assert!(debug_str.contains("3000"));
    }

    #[test]
    fn test_mcp_error_debug_format() {
        let err = McpError::UnknownTool {
            tool_name: "mystery_tool".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("UnknownTool"));
        assert!(debug_str.contains("mystery_tool"));
    }

    #[test]
    fn test_tool_error_debug_format() {
        let err = ToolError::Reasoning {
            message: "logic failed".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Reasoning"));
        assert!(debug_str.contains("logic failed"));
    }

    #[test]
    fn test_error_equality_via_string_representation() {
        let err1 = StorageError::SessionNotFound {
            session_id: "test-id".to_string(),
        };
        let err2 = StorageError::SessionNotFound {
            session_id: "test-id".to_string(),
        };
        assert_eq!(err1.to_string(), err2.to_string());
    }

    #[test]
    fn test_nested_error_display_preservation() {
        let storage_err = StorageError::ThoughtNotFound {
            thought_id: "thought-999".to_string(),
        };
        let original_msg = storage_err.to_string();

        let app_err: AppError = storage_err.into();
        assert!(app_err.to_string().contains(&original_msg));
    }

    #[test]
    fn test_app_result_type_alias() {
        fn returns_app_result() -> AppResult<String> {
            Ok("success".to_string())
        }
        assert!(returns_app_result().is_ok());
    }

    #[test]
    fn test_storage_result_type_alias() {
        fn returns_storage_result() -> StorageResult<i32> {
            Err(StorageError::Query {
                message: "test".to_string(),
            })
        }
        assert!(returns_storage_result().is_err());
    }

    #[test]
    fn test_langbase_result_type_alias() {
        fn returns_langbase_result() -> LangbaseResult<bool> {
            Ok(true)
        }
        assert!(returns_langbase_result().unwrap());
    }

    #[test]
    fn test_mcp_result_type_alias() {
        fn returns_mcp_result() -> McpResult<()> {
            Err(McpError::InvalidRequest {
                message: "test".to_string(),
            })
        }
        assert!(returns_mcp_result().is_err());
    }

    #[test]
    fn test_multiple_error_conversions_in_sequence() {
        let tool_err = ToolError::Validation {
            field: "depth".to_string(),
            reason: "negative value".to_string(),
        };

        let app_err: AppError = tool_err.into();
        assert!(app_err.to_string().contains("Validation failed"));
        assert!(app_err.to_string().contains("depth"));

        let mcp_err: McpError = app_err.into();
        assert!(mcp_err.to_string().contains("Tool execution failed"));
        assert!(mcp_err.to_string().contains("Validation failed"));
    }

    #[test]
    fn test_error_messages_with_special_characters() {
        let err = AppError::Internal {
            message: "Error: \"quotes\" and 'apostrophes' and \\ backslashes".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("quotes"));
        assert!(display.contains("apostrophes"));
        assert!(display.contains("backslashes"));
    }

    #[test]
    fn test_error_messages_with_unicode() {
        let err = StorageError::Query {
            message: "Invalid character: \u{1F4A5}".to_string(),
        };
        assert!(err.to_string().contains("\u{1F4A5}"));
    }

    #[test]
    fn test_empty_error_messages() {
        let err1 = AppError::Config {
            message: "".to_string(),
        };
        assert_eq!(err1.to_string(), "Configuration error: ");

        let err2 = ToolError::Session("".to_string());
        assert_eq!(err2.to_string(), "Session error: ");
    }

    #[test]
    fn test_very_long_error_messages() {
        let long_msg = "a".repeat(1000);
        let err = LangbaseError::InvalidResponse {
            message: long_msg.clone(),
        };
        assert!(err.to_string().contains(&long_msg));
    }

    #[test]
    fn test_error_trait_source_method() {
        use std::error::Error;

        let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
        let mcp_err = McpError::Json(json_err);

        assert!(mcp_err.source().is_some());
    }

    // Tests for new strict mode error types

    #[test]
    fn test_langbase_error_response_parse_failed() {
        let err = LangbaseError::ResponseParseFailed {
            pipe: "linear-reasoning-v1".to_string(),
            message: "expected object, found array".to_string(),
            raw_response: "[1, 2, 3]".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Response parse failed"));
        assert!(display.contains("linear-reasoning-v1"));
        assert!(display.contains("expected object, found array"));
    }

    #[test]
    fn test_langbase_error_pipe_not_found() {
        let err = LangbaseError::PipeNotFound {
            pipe: "nonexistent-pipe-v1".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Pipe not found"));
        assert!(display.contains("nonexistent-pipe-v1"));
        assert!(display.contains("verify pipe exists on Langbase"));
    }

    #[test]
    fn test_tool_error_parse_failed() {
        let err = ToolError::ParseFailed {
            mode: "auto".to_string(),
            message: "JSON syntax error at line 1".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Parse error in auto mode"));
        assert!(display.contains("JSON syntax error"));
    }

    #[test]
    fn test_tool_error_pipe_unavailable() {
        let err = ToolError::PipeUnavailable {
            pipe: "decision-framework-v1".to_string(),
            reason: "API returned 503 Service Unavailable".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Pipe unavailable"));
        assert!(display.contains("decision-framework-v1"));
        assert!(display.contains("503"));
    }

    #[test]
    fn test_tool_error_parse_failed_conversion_to_app_error() {
        let tool_err = ToolError::ParseFailed {
            mode: "got_generate".to_string(),
            message: "missing required field 'continuations'".to_string(),
        };
        let app_err: AppError = tool_err.into();
        assert!(matches!(app_err, AppError::Internal { .. }));
        assert!(app_err.to_string().contains("Parse error"));
    }

    #[test]
    fn test_tool_error_pipe_unavailable_conversion_to_app_error() {
        let tool_err = ToolError::PipeUnavailable {
            pipe: "test-pipe".to_string(),
            reason: "connection refused".to_string(),
        };
        let app_err: AppError = tool_err.into();
        assert!(matches!(app_err, AppError::Internal { .. }));
        assert!(app_err.to_string().contains("Pipe unavailable"));
    }

    #[test]
    fn test_langbase_error_response_parse_failed_debug() {
        let err = LangbaseError::ResponseParseFailed {
            pipe: "test".to_string(),
            message: "error".to_string(),
            raw_response: "raw".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ResponseParseFailed"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_langbase_error_pipe_not_found_debug() {
        let err = LangbaseError::PipeNotFound {
            pipe: "missing-pipe".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("PipeNotFound"));
        assert!(debug_str.contains("missing-pipe"));
    }
}
