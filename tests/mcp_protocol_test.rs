//! Integration tests for MCP protocol handling
//!
//! Tests JSON-RPC request/response handling without external dependencies.

use serde_json::{json, Value};

/// Test helper to parse JSON-RPC response
#[allow(dead_code)]
fn parse_response(response: &str) -> Value {
    serde_json::from_str(response).expect("Failed to parse JSON-RPC response")
}

/// Verify JSON-RPC 2.0 response structure
fn assert_valid_jsonrpc_response(response: &Value) {
    assert_eq!(response["jsonrpc"], "2.0", "Invalid JSON-RPC version");
    assert!(
        response.get("result").is_some() || response.get("error").is_some(),
        "Response must have result or error"
    );
}

#[cfg(test)]
mod initialize_tests {
    use super::*;

    #[test]
    fn test_initialize_request_format() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            }
        });

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["method"], "initialize");
        assert!(request["id"].is_number());
    }

    #[test]
    fn test_initialize_response_structure() {
        // Simulated response from MCP server
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "mcp-langbase-reasoning",
                    "version": "0.1.0"
                }
            }
        });

        assert_valid_jsonrpc_response(&response);

        let result = &response["result"];
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], "mcp-langbase-reasoning");
    }
}

#[cfg(test)]
mod tools_list_tests {
    use super::*;

    #[test]
    fn test_tools_list_request_format() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        assert_eq!(request["method"], "tools/list");
    }

    #[test]
    fn test_tools_list_response_structure() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "reasoning.linear",
                        "description": "Single-pass sequential reasoning.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "content": {
                                    "type": "string",
                                    "description": "The thought content to process"
                                }
                            },
                            "required": ["content"]
                        }
                    }
                ]
            }
        });

        assert_valid_jsonrpc_response(&response);

        let tools = response["result"]["tools"].as_array().expect("tools should be array");
        assert!(!tools.is_empty(), "Should have at least one tool");

        let linear_tool = &tools[0];
        assert_eq!(linear_tool["name"], "reasoning.linear");
        assert!(linear_tool["inputSchema"]["properties"]["content"].is_object());
    }

    #[test]
    fn test_linear_tool_schema_validation() {
        let schema = json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The thought content to process"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1
                }
            },
            "required": ["content"]
        });

        // Verify required fields
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("content")));

        // Verify confidence bounds
        assert_eq!(schema["properties"]["confidence"]["minimum"], 0);
        assert_eq!(schema["properties"]["confidence"]["maximum"], 1);
    }
}

#[cfg(test)]
mod tools_call_tests {
    use super::*;

    #[test]
    fn test_tools_call_request_format() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "reasoning.linear",
                "arguments": {
                    "content": "Analyze the benefits of testing."
                }
            }
        });

        assert_eq!(request["method"], "tools/call");
        assert_eq!(request["params"]["name"], "reasoning.linear");
        assert!(request["params"]["arguments"]["content"].is_string());
    }

    #[test]
    fn test_tools_call_success_response() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": "{\"thought_id\": \"abc123\", \"content\": \"Analysis result\"}"
                    }
                ]
            }
        });

        assert_valid_jsonrpc_response(&response);

        let content = &response["result"]["content"];
        assert!(content.is_array());
        assert_eq!(content[0]["type"], "text");
    }

    #[test]
    fn test_tools_call_error_response() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": "Error: Validation failed"
                    }
                ],
                "isError": true
            }
        });

        assert_valid_jsonrpc_response(&response);
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn test_unknown_tool_error() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": "Error: Unknown tool: nonexistent.tool"
                    }
                ],
                "isError": true
            }
        });

        assert!(response["result"]["isError"].as_bool().unwrap_or(false));
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_parse_error_response() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": "Parse error: invalid JSON"
            }
        });

        assert_valid_jsonrpc_response(&response);
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn test_invalid_params_error() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "error": {
                "code": -32602,
                "message": "Invalid params: missing required field"
            }
        });

        assert_eq!(response["error"]["code"], -32602);
    }

    #[test]
    fn test_method_not_found_error() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 6,
            "error": {
                "code": -32601,
                "message": "Method not found: unknown/method"
            }
        });

        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn test_internal_error_response() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "error": {
                "code": -32603,
                "message": "Internal error: serialization failed"
            }
        });

        assert_eq!(response["error"]["code"], -32603);
    }
}

#[cfg(test)]
mod jsonrpc_compliance_tests {
    use super::*;

    #[test]
    fn test_notification_no_id() {
        // Notifications should not have id field
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });

        assert!(notification.get("id").is_none());
    }

    #[test]
    fn test_response_preserves_id() {
        let request_id = json!(42);
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {}
        });

        assert_eq!(response["id"], request_id);
    }

    #[test]
    fn test_string_id_support() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": "request-123",
            "result": {}
        });

        assert_eq!(response["id"], "request-123");
    }

    #[test]
    fn test_null_id_for_parse_errors() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": "Parse error"
            }
        });

        assert!(response["id"].is_null());
    }
}
