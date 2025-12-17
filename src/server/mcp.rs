use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

use super::{handle_tool_call, SharedState};

/// JSON-RPC request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    /// ID must always be present in responses (null if notification)
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP server information
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// MCP capabilities
#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub tools: ToolCapabilities,
}

/// Tool capabilities
#[derive(Debug, Serialize)]
pub struct ToolCapabilities {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// Initialize result
#[derive(Debug, Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: Capabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// Tool definition
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Tool call parameters
#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// Tool result content
#[derive(Debug, Serialize)]
pub struct ToolResultContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Tool call result
#[derive(Debug, Serialize)]
pub struct ToolCallResult {
    pub content: Vec<ToolResultContent>,
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

/// MCP Server running over stdio
pub struct McpServer {
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
                    Some(JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e)))
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
            "ping" => Some(JsonRpcResponse::success(request.id, Value::Object(Default::default()))),
            method => {
                // For unknown methods, only respond if it's a request (has id)
                if is_notification {
                    debug!(method = %method, "Unknown notification, ignoring");
                    None
                } else {
                    error!(method = %method, "Unknown method");
                    Some(JsonRpcResponse::error(request.id, -32601, format!("Method not found: {}", method)))
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

        let (content, is_error) = match handle_tool_call(&self.state, &params.name, params.arguments).await {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result).unwrap_or_else(|e| {
                    error!(error = %e, "Failed to serialize tool result");
                    format!("{{\"error\": \"Serialization failed: {}\"}}", e)
                });
                (ToolResultContent { content_type: "text".to_string(), text }, None)
            }
            Err(e) => {
                (ToolResultContent { content_type: "text".to_string(), text: format!("Error: {}", e) }, Some(true))
            }
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
        description: "Focus on a specific branch, making it the active branch for the session.".to_string(),
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
        description: "Create a checkpoint at the current reasoning state for later backtracking.".to_string(),
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
        description: "Initialize a new Graph-of-Thoughts reasoning graph with a root node.".to_string(),
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
        description: "Generate k diverse continuations from a node in the reasoning graph.".to_string(),
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
        description: "Score a node's quality based on relevance, validity, depth, and novelty.".to_string(),
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
        description: "Mark terminal nodes and retrieve final conclusions from the reasoning graph.".to_string(),
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
        description: "Get the current state of the reasoning graph including node counts and structure.".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============================================================================
    // JsonRpcResponse tests
    // ============================================================================

    #[test]
    fn test_jsonrpc_response_success_with_id() {
        let response = JsonRpcResponse::success(Some(json!(1)), json!({"result": "ok"}));

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, json!(1));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap()["result"], "ok");
    }

    #[test]
    fn test_jsonrpc_response_success_with_string_id() {
        let response = JsonRpcResponse::success(Some(json!("req-123")), json!({}));

        assert_eq!(response.id, json!("req-123"));
    }

    #[test]
    fn test_jsonrpc_response_success_without_id() {
        let response = JsonRpcResponse::success(None, json!({"data": "value"}));

        assert_eq!(response.id, Value::Null);
        assert!(response.result.is_some());
    }

    #[test]
    fn test_jsonrpc_response_error_with_id() {
        let response = JsonRpcResponse::error(Some(json!(42)), -32600, "Invalid request");

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, json!(42));
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid request");
    }

    #[test]
    fn test_jsonrpc_response_error_without_id() {
        let response = JsonRpcResponse::error(None, -32700, "Parse error");

        assert_eq!(response.id, Value::Null);
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32700);
    }

    #[test]
    fn test_jsonrpc_response_serialization() {
        let response = JsonRpcResponse::success(Some(json!(1)), json!({"test": true}));
        let serialized = serde_json::to_string(&response).unwrap();

        assert!(serialized.contains("\"jsonrpc\":\"2.0\""));
        assert!(serialized.contains("\"id\":1"));
        assert!(serialized.contains("\"result\""));
        // Error should be omitted when None
        assert!(!serialized.contains("\"error\""));
    }

    #[test]
    fn test_jsonrpc_error_serialization() {
        let response = JsonRpcResponse::error(Some(json!(1)), -32601, "Method not found");
        let serialized = serde_json::to_string(&response).unwrap();

        assert!(serialized.contains("\"error\""));
        assert!(serialized.contains("-32601"));
        // Result should be omitted when None
        assert!(!serialized.contains("\"result\""));
    }

    // ============================================================================
    // JsonRpcRequest deserialization tests
    // ============================================================================

    #[test]
    fn test_jsonrpc_request_deserialization() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(json!(1)));
        assert_eq!(request.method, "initialize");
        assert!(request.params.is_some());
    }

    #[test]
    fn test_jsonrpc_request_without_params() {
        let json_str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();

        assert_eq!(request.method, "tools/list");
        assert!(request.params.is_none());
    }

    #[test]
    fn test_jsonrpc_notification_no_id() {
        let json_str = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();

        assert!(request.id.is_none());
        assert_eq!(request.method, "initialized");
    }

    #[test]
    fn test_jsonrpc_request_with_string_id() {
        let json_str = r#"{"jsonrpc":"2.0","id":"uuid-123","method":"ping"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();

        assert_eq!(request.id, Some(json!("uuid-123")));
    }

    // ============================================================================
    // ToolCallParams deserialization tests
    // ============================================================================

    #[test]
    fn test_tool_call_params_deserialization() {
        let json_str = r#"{"name":"reasoning_linear","arguments":{"content":"test"}}"#;
        let params: ToolCallParams = serde_json::from_str(json_str).unwrap();

        assert_eq!(params.name, "reasoning_linear");
        assert!(params.arguments.is_some());
        assert_eq!(params.arguments.unwrap()["content"], "test");
    }

    #[test]
    fn test_tool_call_params_without_arguments() {
        let json_str = r#"{"name":"reasoning_got_state"}"#;
        let params: ToolCallParams = serde_json::from_str(json_str).unwrap();

        assert_eq!(params.name, "reasoning_got_state");
        assert!(params.arguments.is_none());
    }

    // ============================================================================
    // Tool definition tests
    // ============================================================================

    #[test]
    fn test_linear_tool_definition() {
        let tool = get_linear_tool();

        assert_eq!(tool.name, "reasoning_linear");
        assert!(tool.description.contains("sequential"));

        let schema = &tool.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["content"].is_object());
        assert!(schema["required"].as_array().unwrap().contains(&json!("content")));
    }

    #[test]
    fn test_tree_tool_definition() {
        let tool = get_tree_tool();

        assert_eq!(tool.name, "reasoning_tree");
        assert!(tool.description.contains("Branching"));

        let schema = &tool.input_schema;
        assert!(schema["properties"]["num_branches"].is_object());
        assert_eq!(schema["properties"]["num_branches"]["minimum"], 2);
        assert_eq!(schema["properties"]["num_branches"]["maximum"], 4);
    }

    #[test]
    fn test_tree_focus_tool_definition() {
        let tool = get_tree_focus_tool();

        assert_eq!(tool.name, "reasoning_tree_focus");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
        assert!(required.contains(&json!("branch_id")));
    }

    #[test]
    fn test_tree_list_tool_definition() {
        let tool = get_tree_list_tool();

        assert_eq!(tool.name, "reasoning_tree_list");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
    }

    #[test]
    fn test_tree_complete_tool_definition() {
        let tool = get_tree_complete_tool();

        assert_eq!(tool.name, "reasoning_tree_complete");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("branch_id")));
    }

    #[test]
    fn test_divergent_tool_definition() {
        let tool = get_divergent_tool();

        assert_eq!(tool.name, "reasoning_divergent");
        assert!(tool.description.contains("Creative"));

        let schema = &tool.input_schema;
        assert!(schema["properties"]["num_perspectives"].is_object());
    }

    #[test]
    fn test_reflection_tool_definition() {
        let tool = get_reflection_tool();

        assert_eq!(tool.name, "reasoning_reflection");
        assert!(tool.description.contains("Meta-cognitive"));
    }

    #[test]
    fn test_reflection_evaluate_tool_definition() {
        let tool = get_reflection_evaluate_tool();

        assert_eq!(tool.name, "reasoning_reflection_evaluate");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
    }

    #[test]
    fn test_backtracking_tool_definition() {
        let tool = get_backtracking_tool();

        assert_eq!(tool.name, "reasoning_backtrack");
        assert!(tool.description.contains("checkpoint"));
    }

    #[test]
    fn test_checkpoint_create_tool_definition() {
        let tool = get_backtracking_checkpoint_tool();

        assert_eq!(tool.name, "reasoning_checkpoint_create");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
        assert!(required.contains(&json!("name")));
    }

    #[test]
    fn test_checkpoint_list_tool_definition() {
        let tool = get_backtracking_list_tool();

        assert_eq!(tool.name, "reasoning_checkpoint_list");
    }

    #[test]
    fn test_auto_tool_definition() {
        let tool = get_auto_tool();

        assert_eq!(tool.name, "reasoning_auto");
        assert!(tool.description.contains("Automatic"));
    }

    #[test]
    fn test_got_init_tool_definition() {
        let tool = get_got_init_tool();

        assert_eq!(tool.name, "reasoning_got_init");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("content")));
    }

    #[test]
    fn test_got_generate_tool_definition() {
        let tool = get_got_generate_tool();

        assert_eq!(tool.name, "reasoning_got_generate");

        let schema = &tool.input_schema;
        assert!(schema["properties"]["k"].is_object());
        assert_eq!(schema["properties"]["k"]["minimum"], 1);
        assert_eq!(schema["properties"]["k"]["maximum"], 10);
    }

    #[test]
    fn test_got_score_tool_definition() {
        let tool = get_got_score_tool();

        assert_eq!(tool.name, "reasoning_got_score");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
        assert!(required.contains(&json!("node_id")));
    }

    #[test]
    fn test_got_aggregate_tool_definition() {
        let tool = get_got_aggregate_tool();

        assert_eq!(tool.name, "reasoning_got_aggregate");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
        assert!(required.contains(&json!("node_ids")));
    }

    #[test]
    fn test_got_refine_tool_definition() {
        let tool = get_got_refine_tool();

        assert_eq!(tool.name, "reasoning_got_refine");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
        assert!(required.contains(&json!("node_id")));
    }

    #[test]
    fn test_got_prune_tool_definition() {
        let tool = get_got_prune_tool();

        assert_eq!(tool.name, "reasoning_got_prune");

        let schema = &tool.input_schema;
        assert!(schema["properties"]["threshold"].is_object());
    }

    #[test]
    fn test_got_finalize_tool_definition() {
        let tool = get_got_finalize_tool();

        assert_eq!(tool.name, "reasoning_got_finalize");
    }

    #[test]
    fn test_got_state_tool_definition() {
        let tool = get_got_state_tool();

        assert_eq!(tool.name, "reasoning_got_state");

        let required = tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("session_id")));
    }

    // ============================================================================
    // Tool count and completeness tests
    // ============================================================================

    #[test]
    fn test_all_tools_count() {
        let tools = vec![
            get_linear_tool(),
            get_tree_tool(),
            get_tree_focus_tool(),
            get_tree_list_tool(),
            get_tree_complete_tool(),
            get_divergent_tool(),
            get_reflection_tool(),
            get_reflection_evaluate_tool(),
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
        ];

        assert_eq!(tools.len(), 20, "Should have exactly 20 tools defined");
    }

    #[test]
    fn test_all_tools_have_valid_schemas() {
        let tools = vec![
            get_linear_tool(),
            get_tree_tool(),
            get_divergent_tool(),
            get_reflection_tool(),
            get_auto_tool(),
            get_got_init_tool(),
        ];

        for tool in tools {
            assert!(!tool.name.is_empty(), "Tool name should not be empty");
            assert!(!tool.description.is_empty(), "Tool description should not be empty");
            assert_eq!(tool.input_schema["type"], "object", "Schema type should be object for {}", tool.name);
            assert!(tool.input_schema["properties"].is_object(), "Schema should have properties for {}", tool.name);
        }
    }

    #[test]
    fn test_tool_names_are_unique() {
        let tools = vec![
            get_linear_tool(),
            get_tree_tool(),
            get_tree_focus_tool(),
            get_tree_list_tool(),
            get_tree_complete_tool(),
            get_divergent_tool(),
            get_reflection_tool(),
            get_reflection_evaluate_tool(),
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
        ];

        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        names.sort();
        let original_len = names.len();
        names.dedup();

        assert_eq!(names.len(), original_len, "All tool names should be unique");
    }

    // ============================================================================
    // MCP type serialization tests
    // ============================================================================

    #[test]
    fn test_initialize_result_serialization() {
        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: Capabilities {
                tools: ToolCapabilities { list_changed: false },
            },
            server_info: ServerInfo {
                name: "test-server".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["protocolVersion"], "2024-11-05");
        assert_eq!(json["capabilities"]["tools"]["listChanged"], false);
        assert_eq!(json["serverInfo"]["name"], "test-server");
    }

    #[test]
    fn test_tool_serialization() {
        let tool = Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({"type": "object"}),
        };

        let json = serde_json::to_value(&tool).unwrap();

        assert_eq!(json["name"], "test_tool");
        assert_eq!(json["inputSchema"]["type"], "object");
    }

    #[test]
    fn test_tool_call_result_serialization() {
        let result = ToolCallResult {
            content: vec![ToolResultContent {
                content_type: "text".to_string(),
                text: "Hello, world!".to_string(),
            }],
            is_error: None,
        };

        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "Hello, world!");
        // is_error should be omitted when None
        assert!(json.get("isError").is_none());
    }

    #[test]
    fn test_tool_call_result_with_error() {
        let result = ToolCallResult {
            content: vec![ToolResultContent {
                content_type: "text".to_string(),
                text: "Error occurred".to_string(),
            }],
            is_error: Some(true),
        };

        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["isError"], true);
    }
}
