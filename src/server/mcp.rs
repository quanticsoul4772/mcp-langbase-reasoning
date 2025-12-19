//! MCP protocol implementation for JSON-RPC 2.0 communication.
//!
//! This module provides the core MCP server implementation including:
//! - JSON-RPC 2.0 request/response handling
//! - Tool definitions and schemas
//! - Stdio-based server communication

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

use super::{handle_tool_call, SharedState};

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod mcp_tests;

/// JSON-RPC 2.0 request structure.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (must be "2.0").
    pub jsonrpc: String,
    /// Request identifier (None for notifications).
    pub id: Option<Value>,
    /// The method name to invoke.
    pub method: String,
    /// Optional parameters for the method.
    #[serde(default)]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response structure.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (always "2.0").
    pub jsonrpc: String,
    /// Request identifier (null if notification, must always be present per spec).
    pub id: Value,
    /// The result on success (mutually exclusive with error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error on failure (mutually exclusive with result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    /// Error code (negative for predefined errors).
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP server information returned during initialization.
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    /// The server name identifier.
    pub name: String,
    /// The server version string.
    pub version: String,
}

/// MCP server capabilities advertised to clients.
#[derive(Debug, Serialize)]
pub struct Capabilities {
    /// Tool-related capabilities.
    pub tools: ToolCapabilities,
}

/// Tool-specific capabilities.
#[derive(Debug, Serialize)]
pub struct ToolCapabilities {
    /// Whether the tool list can change dynamically.
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// Result of the MCP initialize handshake.
#[derive(Debug, Serialize)]
pub struct InitializeResult {
    /// The MCP protocol version supported.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: Capabilities,
    /// Server identification information.
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// MCP tool definition with JSON Schema.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    /// Unique tool name (used in tool calls).
    pub name: String,
    /// Human-readable description of the tool.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Parameters for a tools/call request.
#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    /// The name of the tool to invoke.
    pub name: String,
    /// Optional arguments for the tool.
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// Content item within a tool result.
#[derive(Debug, Serialize)]
pub struct ToolResultContent {
    /// The content type (e.g., "text").
    #[serde(rename = "type")]
    pub content_type: String,
    /// The text content of the result.
    pub text: String,
}

/// Result of a tool invocation.
#[derive(Debug, Serialize)]
pub struct ToolCallResult {
    /// The result content items.
    pub content: Vec<ToolResultContent>,
    /// Whether the result represents an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl JsonRpcResponse {
    /// Create a success response
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.unwrap_or(Value::Null),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.unwrap_or(Value::Null),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// MCP Server running over stdio.
///
/// Handles JSON-RPC 2.0 messages over stdin/stdout for MCP protocol
/// communication with clients.
pub struct McpServer {
    /// Shared application state.
    state: SharedState,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Run the server using async stdio
    pub async fn run(&self) -> std::io::Result<()> {
        info!("MCP Langbase Reasoning Server starting...");

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            // EOF reached
            if bytes_read == 0 {
                info!("EOF received, shutting down");
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            debug!(request = %trimmed, "Received request");

            let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                Ok(request) => self.handle_request(request).await,
                Err(e) => {
                    error!(error = %e, "Failed to parse request");
                    Some(JsonRpcResponse::error(
                        None,
                        -32700,
                        format!("Parse error: {}", e),
                    ))
                }
            };

            // Only send response if not a notification (per JSON-RPC 2.0 spec)
            if let Some(response) = response {
                let response_json = serde_json::to_string(&response)?;
                debug!(response = %response_json, "Sending response");

                stdout.write_all(response_json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }

        Ok(())
    }

    /// Handle a single JSON-RPC request
    /// Returns None for notifications (requests without id) per JSON-RPC 2.0 spec
    async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        // Check if this is a notification (no id = no response required)
        let is_notification = request.id.is_none();

        match request.method.as_str() {
            "initialize" => Some(self.handle_initialize(request.id)),
            "initialized" => {
                // Notification - no response per JSON-RPC 2.0
                debug!("Received initialized notification");
                None
            }
            "notifications/cancelled" => {
                // Notification - no response
                debug!("Received cancelled notification");
                None
            }
            "tools/list" => Some(self.handle_tools_list(request.id)),
            "tools/call" => Some(self.handle_tool_call(request.id, request.params).await),
            "ping" => Some(JsonRpcResponse::success(
                request.id,
                Value::Object(Default::default()),
            )),
            method => {
                // For unknown methods, only respond if it's a request (has id)
                if is_notification {
                    debug!(method = %method, "Unknown notification, ignoring");
                    None
                } else {
                    error!(method = %method, "Unknown method");
                    Some(JsonRpcResponse::error(
                        request.id,
                        -32601,
                        format!("Method not found: {}", method),
                    ))
                }
            }
        }
    }

    /// Handle initialize request
    fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        info!("Handling initialize request");

        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: Capabilities {
                tools: ToolCapabilities {
                    list_changed: false,
                },
            },
            server_info: ServerInfo {
                name: "mcp-langbase-reasoning".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        match serde_json::to_value(result) {
            Ok(val) => JsonRpcResponse::success(id, val),
            Err(e) => {
                error!(error = %e, "Failed to serialize initialize result");
                JsonRpcResponse::error(id, -32603, format!("Internal error: {}", e))
            }
        }
    }

    /// Handle tools/list request
    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        info!("Handling tools/list request");

        let tools = vec![
            // Phase 1-2 tools
            get_linear_tool(),
            get_tree_tool(),
            get_tree_focus_tool(),
            get_tree_list_tool(),
            get_tree_complete_tool(),
            get_divergent_tool(),
            get_reflection_tool(),
            get_reflection_evaluate_tool(),
            // Phase 3 tools
            get_backtracking_tool(),
            get_backtracking_checkpoint_tool(),
            get_backtracking_list_tool(),
            get_auto_tool(),
            get_got_init_tool(),
            get_got_generate_tool(),
            get_got_score_tool(),
            get_got_aggregate_tool(),
            get_got_refine_tool(),
            get_got_prune_tool(),
            get_got_finalize_tool(),
            get_got_state_tool(),
            // Phase 4 tools - Bias & Fallacy Detection
            get_detect_biases_tool(),
            get_detect_fallacies_tool(),
            // Phase 5 tools - Workflow Presets
            get_preset_list_tool(),
            get_preset_run_tool(),
            // Phase 6 tools - Decision Framework & Evidence Assessment
            get_make_decision_tool(),
            get_analyze_perspectives_tool(),
            get_assess_evidence_tool(),
            get_probabilistic_tool(),
            // Metrics tools
            get_metrics_summary_tool(),
            get_metrics_by_pipe_tool(),
            get_metrics_invocations_tool(),
            get_fallback_metrics_tool(),
            // Debug tools
            get_debug_config_tool(),
        ];

        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "tools": tools
            }),
        )
    }

    /// Handle tools/call request
    async fn handle_tool_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params: ToolCallParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(id, -32602, format!("Invalid params: {}", e));
                }
            },
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing params");
            }
        };

        info!(tool = %params.name, "Handling tool call");

        let (content, is_error) =
            match handle_tool_call(&self.state, &params.name, params.arguments).await {
                Ok(result) => {
                    let text = serde_json::to_string_pretty(&result).unwrap_or_else(|e| {
                        error!(error = %e, "Failed to serialize tool result");
                        format!("{{\"error\": \"Serialization failed: {}\"}}", e)
                    });
                    (
                        ToolResultContent {
                            content_type: "text".to_string(),
                            text,
                        },
                        None,
                    )
                }
                Err(e) => (
                    ToolResultContent {
                        content_type: "text".to_string(),
                        text: format!("Error: {}", e),
                    },
                    Some(true),
                ),
            };

        let tool_result = ToolCallResult {
            content: vec![content],
            is_error,
        };

        match serde_json::to_value(tool_result) {
            Ok(val) => JsonRpcResponse::success(id, val),
            Err(e) => {
                error!(error = %e, "Failed to serialize tool call result");
                JsonRpcResponse::error(id.clone(), -32603, format!("Internal error: {}", e))
            }
        }
    }
}

/// Get the linear reasoning tool definition
fn get_linear_tool() -> Tool {
    Tool {
        name: "reasoning_linear".to_string(),
        description: "Single-pass sequential reasoning. Process a thought and get a logical continuation or analysis.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The thought content to process"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context continuity"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Confidence threshold (0.0-1.0)"
                }
            },
            "required": ["content"],
            "additionalProperties": false
        }),
    }
}

/// Get the tree reasoning tool definition
fn get_tree_tool() -> Tool {
    Tool {
        name: "reasoning_tree".to_string(),
        description: "Branching exploration with multiple reasoning paths. Explores 2-4 distinct approaches and recommends the most promising one.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The thought content to explore"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context continuity"
                },
                "branch_id": {
                    "type": "string",
                    "description": "Optional branch ID to extend (creates root branch if not provided)"
                },
                "num_branches": {
                    "type": "integer",
                    "minimum": 2,
                    "maximum": 4,
                    "description": "Number of branches to explore (default: 3)"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Confidence threshold (0.0-1.0)"
                },
                "cross_refs": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "to_branch": { "type": "string" },
                            "type": { "type": "string", "enum": ["supports", "contradicts", "extends", "alternative", "depends"] },
                            "reason": { "type": "string" },
                            "strength": { "type": "number", "minimum": 0, "maximum": 1 }
                        },
                        "required": ["to_branch", "type"]
                    },
                    "description": "Optional cross-references to other branches"
                }
            },
            "required": ["content"],
            "additionalProperties": false
        }),
    }
}

/// Get the tree focus tool definition
fn get_tree_focus_tool() -> Tool {
    Tool {
        name: "reasoning_tree_focus".to_string(),
        description: "Focus on a specific branch, making it the active branch for the session."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "branch_id": {
                    "type": "string",
                    "description": "The branch ID to focus on"
                }
            },
            "required": ["session_id", "branch_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the tree list tool definition
fn get_tree_list_tool() -> Tool {
    Tool {
        name: "reasoning_tree_list".to_string(),
        description: "List all branches in a session.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the tree complete tool definition
fn get_tree_complete_tool() -> Tool {
    Tool {
        name: "reasoning_tree_complete".to_string(),
        description: "Mark a branch as completed or abandoned.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "branch_id": {
                    "type": "string",
                    "description": "The branch ID to update"
                },
                "completed": {
                    "type": "boolean",
                    "description": "True to mark as completed, false to mark as abandoned (default: true)"
                }
            },
            "required": ["branch_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the divergent reasoning tool definition
fn get_divergent_tool() -> Tool {
    Tool {
        name: "reasoning_divergent".to_string(),
        description: "Creative reasoning that generates novel perspectives and unconventional solutions. Challenges assumptions and synthesizes diverse viewpoints.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The thought content to explore creatively"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context continuity"
                },
                "branch_id": {
                    "type": "string",
                    "description": "Optional branch ID for tree mode integration"
                },
                "num_perspectives": {
                    "type": "integer",
                    "minimum": 2,
                    "maximum": 5,
                    "description": "Number of perspectives to generate (default: 3)"
                },
                "challenge_assumptions": {
                    "type": "boolean",
                    "description": "Whether to explicitly identify and challenge assumptions"
                },
                "force_rebellion": {
                    "type": "boolean",
                    "description": "Enable maximum creativity mode with contrarian viewpoints"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Confidence threshold (0.0-1.0, default: 0.7)"
                }
            },
            "required": ["content"],
            "additionalProperties": false
        }),
    }
}

/// Get the reflection reasoning tool definition
fn get_reflection_tool() -> Tool {
    Tool {
        name: "reasoning_reflection".to_string(),
        description: "Meta-cognitive reasoning that analyzes and improves reasoning quality. Evaluates strengths, weaknesses, and provides recommendations.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "thought_id": {
                    "type": "string",
                    "description": "ID of an existing thought to reflect upon"
                },
                "content": {
                    "type": "string",
                    "description": "Content to reflect upon (used if thought_id not provided)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context continuity"
                },
                "branch_id": {
                    "type": "string",
                    "description": "Optional branch ID for tree mode integration"
                },
                "max_iterations": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 5,
                    "description": "Maximum iterations for iterative refinement (default: 3)"
                },
                "quality_threshold": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Quality threshold to stop iterating (default: 0.8)"
                },
                "include_chain": {
                    "type": "boolean",
                    "description": "Whether to include full reasoning chain in context"
                }
            },
            "additionalProperties": false
        }),
    }
}

/// Get the reflection evaluate tool definition
fn get_reflection_evaluate_tool() -> Tool {
    Tool {
        name: "reasoning_reflection_evaluate".to_string(),
        description: "Evaluate a session's overall reasoning quality, coherence, and provide recommendations.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID to evaluate"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

// ============================================================================
// Phase 3 Tool Definitions
// ============================================================================

/// Get the backtracking tool definition
fn get_backtracking_tool() -> Tool {
    Tool {
        name: "reasoning_backtrack".to_string(),
        description: "Restore from a checkpoint and explore alternative reasoning paths. Enables non-linear exploration with state restoration.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "checkpoint_id": {
                    "type": "string",
                    "description": "ID of the checkpoint to restore from"
                },
                "new_direction": {
                    "type": "string",
                    "description": "Optional new direction or approach to try from the checkpoint"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID (must match checkpoint's session)"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Confidence threshold (0.0-1.0, default: 0.8)"
                }
            },
            "required": ["checkpoint_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the backtracking checkpoint creation tool definition
fn get_backtracking_checkpoint_tool() -> Tool {
    Tool {
        name: "reasoning_checkpoint_create".to_string(),
        description: "Create a checkpoint at the current reasoning state for later backtracking."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID to checkpoint"
                },
                "name": {
                    "type": "string",
                    "description": "Name for the checkpoint"
                },
                "description": {
                    "type": "string",
                    "description": "Optional description of the checkpoint state"
                }
            },
            "required": ["session_id", "name"],
            "additionalProperties": false
        }),
    }
}

/// Get the backtracking list checkpoints tool definition
fn get_backtracking_list_tool() -> Tool {
    Tool {
        name: "reasoning_checkpoint_list".to_string(),
        description: "List all checkpoints available for a session.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID to list checkpoints for"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the auto mode router tool definition
fn get_auto_tool() -> Tool {
    Tool {
        name: "reasoning_auto".to_string(),
        description: "Automatically select the most appropriate reasoning mode based on content analysis. Routes to linear, tree, divergent, reflection, or got mode.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to analyze for mode selection"
                },
                "hints": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional hints about the problem type"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context"
                }
            },
            "required": ["content"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT initialization tool definition
fn get_got_init_tool() -> Tool {
    Tool {
        name: "reasoning_got_init".to_string(),
        description: "Initialize a new Graph-of-Thoughts reasoning graph with a root node."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Initial thought content for the root node"
                },
                "problem": {
                    "type": "string",
                    "description": "Optional problem context"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID"
                },
                "config": {
                    "type": "object",
                    "properties": {
                        "max_nodes": { "type": "integer", "minimum": 10, "maximum": 1000 },
                        "max_depth": { "type": "integer", "minimum": 1, "maximum": 20 },
                        "default_k": { "type": "integer", "minimum": 1, "maximum": 10 },
                        "prune_threshold": { "type": "number", "minimum": 0, "maximum": 1 }
                    },
                    "description": "Optional configuration overrides"
                }
            },
            "required": ["content"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT generate tool definition
fn get_got_generate_tool() -> Tool {
    Tool {
        name: "reasoning_got_generate".to_string(),
        description: "Generate k diverse continuations from a node in the reasoning graph."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "node_id": {
                    "type": "string",
                    "description": "Optional node ID to generate from (uses active nodes if not specified)"
                },
                "k": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "description": "Number of continuations to generate (default: 3)"
                },
                "problem": {
                    "type": "string",
                    "description": "Optional problem context"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT score tool definition
fn get_got_score_tool() -> Tool {
    Tool {
        name: "reasoning_got_score".to_string(),
        description: "Score a node's quality based on relevance, validity, depth, and novelty."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "node_id": {
                    "type": "string",
                    "description": "The node ID to score"
                },
                "problem": {
                    "type": "string",
                    "description": "Optional problem context"
                }
            },
            "required": ["session_id", "node_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT aggregate tool definition
fn get_got_aggregate_tool() -> Tool {
    Tool {
        name: "reasoning_got_aggregate".to_string(),
        description: "Merge multiple reasoning nodes into a unified insight.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "node_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 2,
                    "description": "Node IDs to aggregate (minimum 2)"
                },
                "problem": {
                    "type": "string",
                    "description": "Optional problem context"
                }
            },
            "required": ["session_id", "node_ids"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT refine tool definition
fn get_got_refine_tool() -> Tool {
    Tool {
        name: "reasoning_got_refine".to_string(),
        description: "Improve a reasoning node through self-critique and refinement.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "node_id": {
                    "type": "string",
                    "description": "The node ID to refine"
                },
                "problem": {
                    "type": "string",
                    "description": "Optional problem context"
                }
            },
            "required": ["session_id", "node_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT prune tool definition
fn get_got_prune_tool() -> Tool {
    Tool {
        name: "reasoning_got_prune".to_string(),
        description: "Remove low-scoring nodes from the reasoning graph.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "threshold": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Score threshold - nodes below this are pruned (default: 0.3)"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT finalize tool definition
fn get_got_finalize_tool() -> Tool {
    Tool {
        name: "reasoning_got_finalize".to_string(),
        description: "Mark terminal nodes and retrieve final conclusions from the reasoning graph."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                },
                "terminal_node_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional node IDs to mark as terminal (auto-selects best nodes if empty)"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

/// Get the GoT state tool definition
fn get_got_state_tool() -> Tool {
    Tool {
        name: "reasoning_got_state".to_string(),
        description:
            "Get the current state of the reasoning graph including node counts and structure."
                .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session ID"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    }
}

// ============================================================================
// Phase 4 Tool Definitions - Bias & Fallacy Detection
// ============================================================================

/// Get the detect biases tool definition
fn get_detect_biases_tool() -> Tool {
    Tool {
        name: "reasoning_detect_biases".to_string(),
        description: "Analyze content for cognitive biases such as confirmation bias, anchoring, availability heuristic, sunk cost fallacy, and others. Returns detected biases with severity, confidence, explanation, and remediation suggestions.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to analyze for cognitive biases"
                },
                "thought_id": {
                    "type": "string",
                    "description": "ID of an existing thought to analyze (alternative to content)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID for context and persistence"
                },
                "check_types": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Specific bias types to check (optional, checks all if not specified)"
                }
            },
            "additionalProperties": false
        }),
    }
}

/// Get the detect fallacies tool definition
fn get_detect_fallacies_tool() -> Tool {
    Tool {
        name: "reasoning_detect_fallacies".to_string(),
        description: "Analyze content for logical fallacies including ad hominem, straw man, false dichotomy, appeal to authority, circular reasoning, and others. Returns detected fallacies with severity, confidence, explanation, and remediation suggestions.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to analyze for logical fallacies"
                },
                "thought_id": {
                    "type": "string",
                    "description": "ID of an existing thought to analyze (alternative to content)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID for context and persistence"
                },
                "check_formal": {
                    "type": "boolean",
                    "description": "Check for formal logical fallacies (default: true)"
                },
                "check_informal": {
                    "type": "boolean",
                    "description": "Check for informal logical fallacies (default: true)"
                }
            },
            "additionalProperties": false
        }),
    }
}

// ============================================================================
// Phase 5 Tool Definitions - Workflow Presets
// ============================================================================

/// Get the preset list tool definition
fn get_preset_list_tool() -> Tool {
    Tool {
        name: "reasoning_preset_list".to_string(),
        description: "List available workflow presets. Presets are pre-defined multi-step reasoning workflows that compose existing modes into higher-level operations like code review, debug analysis, and architecture decisions.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Filter by category (e.g., 'code', 'architecture', 'research')"
                }
            },
            "additionalProperties": false
        }),
    }
}

/// Get the preset run tool definition
fn get_preset_run_tool() -> Tool {
    Tool {
        name: "reasoning_preset_run".to_string(),
        description: "Execute a workflow preset. Runs a multi-step reasoning workflow with automatic step sequencing, dependency management, and result aggregation.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "ID of the preset to run (e.g., 'code-review', 'debug-analysis', 'architecture-decision')"
                },
                "inputs": {
                    "type": "object",
                    "description": "Input parameters for the preset workflow",
                    "additionalProperties": true
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context persistence"
                }
            },
            "required": ["preset_id"],
            "additionalProperties": false
        }),
    }
}

// ============================================================================
// Phase 6 Tool Definitions - Decision Framework & Evidence Assessment
// ============================================================================

/// Get the make decision tool definition
fn get_make_decision_tool() -> Tool {
    Tool {
        name: "reasoning_make_decision".to_string(),
        description: "Multi-criteria decision analysis using weighted scoring, pairwise comparison, or TOPSIS methods. Evaluates alternatives against criteria with optional weights and provides ranked recommendations.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The decision question to analyze"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 2,
                    "description": "Options to evaluate (minimum 2)"
                },
                "criteria": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Criterion name" },
                            "weight": { "type": "number", "minimum": 0, "maximum": 1, "description": "Importance weight (0-1)" },
                            "description": { "type": "string", "description": "Optional criterion description" }
                        },
                        "required": ["name"]
                    },
                    "description": "Evaluation criteria with optional weights"
                },
                "method": {
                    "type": "string",
                    "enum": ["weighted_sum", "pairwise", "topsis"],
                    "description": "Analysis method (default: weighted_sum)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context persistence"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the decision"
                }
            },
            "required": ["question", "options"],
            "additionalProperties": false
        }),
    }
}

/// Get the analyze perspectives tool definition
fn get_analyze_perspectives_tool() -> Tool {
    Tool {
        name: "reasoning_analyze_perspectives".to_string(),
        description: "Stakeholder power/interest matrix analysis. Maps stakeholders to quadrants (KeyPlayer, KeepSatisfied, KeepInformed, MinimalEffort) and identifies conflicts, alignments, and strategic engagement recommendations.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "The topic or decision to analyze from multiple perspectives"
                },
                "stakeholders": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Stakeholder name" },
                            "role": { "type": "string", "description": "Stakeholder role" },
                            "interests": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Key interests"
                            },
                            "power_level": {
                                "type": "number",
                                "minimum": 0,
                                "maximum": 1,
                                "description": "Power/influence level (0-1)"
                            },
                            "interest_level": {
                                "type": "number",
                                "minimum": 0,
                                "maximum": 1,
                                "description": "Interest/stake level (0-1)"
                            }
                        },
                        "required": ["name"]
                    },
                    "description": "Stakeholders to consider (optional - will infer if not provided)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context persistence"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the analysis"
                }
            },
            "required": ["topic"],
            "additionalProperties": false
        }),
    }
}

/// Get the assess evidence tool definition
fn get_assess_evidence_tool() -> Tool {
    Tool {
        name: "reasoning_assess_evidence".to_string(),
        description: "Evidence quality assessment with source credibility analysis, corroboration tracking, and chain of custody evaluation. Returns credibility scores, confidence assessments, and evidence synthesis.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "claim": {
                    "type": "string",
                    "description": "The claim to assess evidence for"
                },
                "evidence": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "Evidence content or description" },
                            "source": { "type": "string", "description": "Source of the evidence" },
                            "source_type": {
                                "type": "string",
                                "enum": ["primary", "secondary", "tertiary", "expert", "anecdotal"],
                                "description": "Type of source"
                            },
                            "date": { "type": "string", "description": "Date of evidence (ISO format)" }
                        },
                        "required": ["content"]
                    },
                    "minItems": 1,
                    "description": "Evidence items to assess"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context persistence"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the assessment"
                }
            },
            "required": ["claim", "evidence"],
            "additionalProperties": false
        }),
    }
}

/// Get the probabilistic reasoning tool definition
fn get_probabilistic_tool() -> Tool {
    Tool {
        name: "reasoning_probabilistic".to_string(),
        description: "Bayesian probability updates for belief revision. Takes prior probabilities and new evidence to compute posterior probabilities with entropy and uncertainty metrics.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "hypothesis": {
                    "type": "string",
                    "description": "The hypothesis to evaluate"
                },
                "prior": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Prior probability (0-1)"
                },
                "evidence": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string", "description": "Evidence description" },
                            "likelihood_if_true": {
                                "type": "number",
                                "minimum": 0,
                                "maximum": 1,
                                "description": "P(evidence|hypothesis true)"
                            },
                            "likelihood_if_false": {
                                "type": "number",
                                "minimum": 0,
                                "maximum": 1,
                                "description": "P(evidence|hypothesis false)"
                            }
                        },
                        "required": ["description"]
                    },
                    "minItems": 1,
                    "description": "Evidence items with likelihood ratios"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID for context persistence"
                }
            },
            "required": ["hypothesis", "prior", "evidence"],
            "additionalProperties": false
        }),
    }
}

// ============================================================================
// Metrics Tools
// ============================================================================

fn get_metrics_summary_tool() -> Tool {
    Tool {
        name: "reasoning_metrics_summary".to_string(),
        description: "Get aggregated usage statistics for all Langbase pipes. Returns call counts, success rates, and latency statistics for each pipe that has been invoked.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}

fn get_metrics_by_pipe_tool() -> Tool {
    Tool {
        name: "reasoning_metrics_by_pipe".to_string(),
        description: "Get detailed usage statistics for a specific Langbase pipe. Returns call counts, success/failure counts, success rate, and latency statistics (avg, min, max).".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pipe_name": {
                    "type": "string",
                    "description": "Name of the pipe to get metrics for"
                }
            },
            "required": ["pipe_name"],
            "additionalProperties": false
        }),
    }
}

fn get_metrics_invocations_tool() -> Tool {
    Tool {
        name: "reasoning_metrics_invocations".to_string(),
        description: "Query invocation history with optional filtering. Returns detailed logs of pipe calls including inputs, outputs, latency, and success status.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pipe_name": {
                    "type": "string",
                    "description": "Filter by pipe name"
                },
                "tool_name": {
                    "type": "string",
                    "description": "Filter by MCP tool name"
                },
                "session_id": {
                    "type": "string",
                    "description": "Filter by session ID"
                },
                "success_only": {
                    "type": "boolean",
                    "description": "If true, only successful calls; if false, only failed calls"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "default": 100,
                    "description": "Maximum number of results to return"
                }
            },
            "additionalProperties": false
        }),
    }
}

fn get_debug_config_tool() -> Tool {
    Tool {
        name: "reasoning_debug_config".to_string(),
        description: "Debug tool to inspect the current pipe configuration. Returns the actual pipe names being used by the server.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}

fn get_fallback_metrics_tool() -> Tool {
    Tool {
        name: "reasoning_fallback_metrics".to_string(),
        description: "Get metrics about fallback usage across invocations. Returns total fallbacks, breakdown by type (parse_error, api_unavailable, local_calculation) and by pipe, plus recommendations for enabling strict mode.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}
