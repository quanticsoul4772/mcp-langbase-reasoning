//! Unit tests for MCP protocol implementation.
//!
//! Tests JSON-RPC 2.0 request/response handling, tool definitions,
//! and MCP type serialization.

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
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("content")));
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
// Phase 4 Tool Definition Tests - Bias & Fallacy Detection
// ============================================================================

#[test]
fn test_detect_biases_tool_definition() {
    let tool = get_detect_biases_tool();

    assert_eq!(tool.name, "reasoning_detect_biases");
    assert!(tool.description.contains("cognitive biases"));
    assert!(tool.description.contains("confirmation bias"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["content"].is_object());
    assert!(schema["properties"]["thought_id"].is_object());
    assert!(schema["properties"]["session_id"].is_object());
    assert!(schema["properties"]["check_types"].is_object());
    assert_eq!(schema["properties"]["check_types"]["type"], "array");
}

#[test]
fn test_detect_fallacies_tool_definition() {
    let tool = get_detect_fallacies_tool();

    assert_eq!(tool.name, "reasoning_detect_fallacies");
    assert!(tool.description.contains("logical fallacies"));
    assert!(tool.description.contains("ad hominem"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["content"].is_object());
    assert!(schema["properties"]["thought_id"].is_object());
    assert!(schema["properties"]["session_id"].is_object());
    assert!(schema["properties"]["check_formal"].is_object());
    assert!(schema["properties"]["check_informal"].is_object());
    assert_eq!(schema["properties"]["check_formal"]["type"], "boolean");
    assert_eq!(schema["properties"]["check_informal"]["type"], "boolean");
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
        // Phase 4 - Detection tools
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
    ];

    assert_eq!(tools.len(), 22, "Should have exactly 22 tools defined");
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
        assert!(
            !tool.description.is_empty(),
            "Tool description should not be empty"
        );
        assert_eq!(
            tool.input_schema["type"], "object",
            "Schema type should be object for {}",
            tool.name
        );
        assert!(
            tool.input_schema["properties"].is_object(),
            "Schema should have properties for {}",
            tool.name
        );
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
        // Phase 4 - Detection tools
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
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
            tools: ToolCapabilities {
                list_changed: false,
            },
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
