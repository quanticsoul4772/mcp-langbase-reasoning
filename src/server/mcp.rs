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
                    JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e))
                }
            };

            let response_json = serde_json::to_string(&response)?;
            debug!(response = %response_json, "Sending response");

            stdout.write_all(response_json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }

        Ok(())
    }

    /// Handle a single JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "initialized" => JsonRpcResponse::success(request.id, Value::Null),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tool_call(request.id, request.params).await,
            "ping" => JsonRpcResponse::success(request.id, Value::Object(Default::default())),
            method => {
                error!(method = %method, "Unknown method");
                JsonRpcResponse::error(request.id, -32601, format!("Method not found: {}", method))
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

        let tools = vec![get_linear_tool()];

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
        name: "reasoning.linear".to_string(),
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
            "required": ["content"]
        }),
    }
}
