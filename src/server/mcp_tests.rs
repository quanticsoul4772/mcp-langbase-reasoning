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
        // Phase 5 - Preset tools
        get_preset_list_tool(),
        get_preset_run_tool(),
    ];

    assert_eq!(tools.len(), 24, "Should have exactly 24 tools defined");
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
        // Phase 5 - Preset tools
        get_preset_list_tool(),
        get_preset_run_tool(),
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

// ============================================================================
// Phase 5 - Preset tool tests
// ============================================================================

#[test]
fn test_preset_list_tool_definition() {
    let tool = get_preset_list_tool();

    assert_eq!(tool.name, "reasoning_preset_list");
    assert!(tool.description.contains("workflow presets"));
    assert!(tool.description.contains("code review"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["category"].is_object());
    assert_eq!(schema["properties"]["category"]["type"], "string");
}

#[test]
fn test_preset_run_tool_definition() {
    let tool = get_preset_run_tool();

    assert_eq!(tool.name, "reasoning_preset_run");
    assert!(tool.description.contains("Execute a workflow preset"));
    assert!(tool.description.contains("multi-step"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["preset_id"].is_object());
    assert_eq!(schema["properties"]["preset_id"]["type"], "string");
    assert!(schema["properties"]["inputs"].is_object());
    assert_eq!(schema["properties"]["inputs"]["type"], "object");
    assert!(schema["properties"]["session_id"].is_object());

    // Check required fields
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"preset_id"));
    assert!(!required.contains(&"inputs"));
    assert!(!required.contains(&"session_id"));
}

// ============================================================================
// Phase 6 - Decision Framework & Evidence Assessment tool tests
// ============================================================================

#[test]
fn test_make_decision_tool_definition() {
    let tool = get_make_decision_tool();

    assert_eq!(tool.name, "reasoning_make_decision");
    assert!(tool.description.contains("Multi-criteria decision"));
    assert!(tool.description.contains("TOPSIS"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["question"].is_object());
    assert_eq!(schema["properties"]["question"]["type"], "string");
    assert!(schema["properties"]["alternatives"].is_object());
    assert_eq!(schema["properties"]["alternatives"]["type"], "array");
    assert_eq!(schema["properties"]["alternatives"]["minItems"], 2);
    assert!(schema["properties"]["criteria"].is_object());
    assert_eq!(schema["properties"]["criteria"]["type"], "array");
    assert!(schema["properties"]["method"].is_object());
    assert_eq!(schema["properties"]["method"]["type"], "string");

    // Check method enum values
    let method_enum = schema["properties"]["method"]["enum"].as_array().unwrap();
    assert!(method_enum.contains(&json!("weighted_sum")));
    assert!(method_enum.contains(&json!("pairwise")));
    assert!(method_enum.contains(&json!("topsis")));

    // Check criteria properties
    let criteria_items = &schema["properties"]["criteria"]["items"];
    assert!(criteria_items["properties"]["name"].is_object());
    assert!(criteria_items["properties"]["weight"].is_object());
    assert_eq!(criteria_items["properties"]["weight"]["minimum"], 0);
    assert_eq!(criteria_items["properties"]["weight"]["maximum"], 1);

    // Check required fields
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"question"));
    assert!(required.contains(&"alternatives"));
}

#[test]
fn test_analyze_perspectives_tool_definition() {
    let tool = get_analyze_perspectives_tool();

    assert_eq!(tool.name, "reasoning_analyze_perspectives");
    assert!(tool.description.contains("Stakeholder"));
    assert!(tool.description.contains("power/interest matrix"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["topic"].is_object());
    assert!(schema["properties"]["stakeholders"].is_object());
    assert_eq!(schema["properties"]["stakeholders"]["type"], "array");

    // Check stakeholder properties
    let stakeholder_items = &schema["properties"]["stakeholders"]["items"];
    assert!(stakeholder_items["properties"]["name"].is_object());
    assert!(stakeholder_items["properties"]["role"].is_object());
    assert!(stakeholder_items["properties"]["interests"].is_object());
    assert!(stakeholder_items["properties"]["power_level"].is_object());
    assert_eq!(stakeholder_items["properties"]["power_level"]["minimum"], 0);
    assert_eq!(stakeholder_items["properties"]["power_level"]["maximum"], 1);
    assert!(stakeholder_items["properties"]["interest_level"].is_object());
    assert_eq!(
        stakeholder_items["properties"]["interest_level"]["minimum"],
        0
    );
    assert_eq!(
        stakeholder_items["properties"]["interest_level"]["maximum"],
        1
    );

    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"topic"));
}

#[test]
fn test_assess_evidence_tool_definition() {
    let tool = get_assess_evidence_tool();

    assert_eq!(tool.name, "reasoning_assess_evidence");
    assert!(tool.description.contains("Evidence quality assessment"));
    assert!(tool.description.contains("credibility"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["claim"].is_object());
    assert!(schema["properties"]["evidence"].is_object());
    assert_eq!(schema["properties"]["evidence"]["type"], "array");
    assert_eq!(schema["properties"]["evidence"]["minItems"], 1);

    // Check evidence item properties
    let evidence_items = &schema["properties"]["evidence"]["items"];
    assert!(evidence_items["properties"]["content"].is_object());
    assert!(evidence_items["properties"]["source"].is_object());
    assert!(evidence_items["properties"]["source_type"].is_object());

    // Check source_type enum
    let source_type_enum = evidence_items["properties"]["source_type"]["enum"]
        .as_array()
        .unwrap();
    assert!(source_type_enum.contains(&json!("primary")));
    assert!(source_type_enum.contains(&json!("secondary")));
    assert!(source_type_enum.contains(&json!("tertiary")));
    assert!(source_type_enum.contains(&json!("expert")));
    assert!(source_type_enum.contains(&json!("anecdotal")));

    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"claim"));
    assert!(required.contains(&"evidence"));
}

#[test]
fn test_probabilistic_tool_definition() {
    let tool = get_probabilistic_tool();

    assert_eq!(tool.name, "reasoning_probabilistic");
    assert!(tool.description.contains("Bayesian"));
    assert!(tool.description.contains("probability"));

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["hypothesis"].is_object());
    assert!(schema["properties"]["prior"].is_object());
    assert_eq!(schema["properties"]["prior"]["minimum"], 0);
    assert_eq!(schema["properties"]["prior"]["maximum"], 1);
    assert!(schema["properties"]["evidence"].is_object());
    assert_eq!(schema["properties"]["evidence"]["type"], "array");
    assert_eq!(schema["properties"]["evidence"]["minItems"], 1);

    // Check evidence item properties
    let evidence_items = &schema["properties"]["evidence"]["items"];
    assert!(evidence_items["properties"]["description"].is_object());
    assert!(evidence_items["properties"]["likelihood_if_true"].is_object());
    assert_eq!(
        evidence_items["properties"]["likelihood_if_true"]["minimum"],
        0
    );
    assert_eq!(
        evidence_items["properties"]["likelihood_if_true"]["maximum"],
        1
    );
    assert!(evidence_items["properties"]["likelihood_if_false"].is_object());
    assert_eq!(
        evidence_items["properties"]["likelihood_if_false"]["minimum"],
        0
    );
    assert_eq!(
        evidence_items["properties"]["likelihood_if_false"]["maximum"],
        1
    );

    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"hypothesis"));
    assert!(required.contains(&"prior"));
    assert!(required.contains(&"evidence"));
}

// ============================================================================
// Additional JSON Schema validation tests
// ============================================================================

#[test]
fn test_linear_tool_confidence_bounds() {
    let tool = get_linear_tool();
    let schema = &tool.input_schema;

    assert_eq!(schema["properties"]["confidence"]["minimum"], 0);
    assert_eq!(schema["properties"]["confidence"]["maximum"], 1);
    assert_eq!(schema["properties"]["confidence"]["type"], "number");
}

#[test]
fn test_tree_tool_cross_refs_schema() {
    let tool = get_tree_tool();
    let schema = &tool.input_schema;

    assert!(schema["properties"]["cross_refs"].is_object());
    assert_eq!(schema["properties"]["cross_refs"]["type"], "array");

    let cross_ref_items = &schema["properties"]["cross_refs"]["items"];
    assert!(cross_ref_items["properties"]["to_branch"].is_object());
    assert!(cross_ref_items["properties"]["type"].is_object());
    assert!(cross_ref_items["properties"]["reason"].is_object());
    assert!(cross_ref_items["properties"]["strength"].is_object());

    // Check type enum
    let type_enum = cross_ref_items["properties"]["type"]["enum"]
        .as_array()
        .unwrap();
    assert!(type_enum.contains(&json!("supports")));
    assert!(type_enum.contains(&json!("contradicts")));
    assert!(type_enum.contains(&json!("extends")));
    assert!(type_enum.contains(&json!("alternative")));
    assert!(type_enum.contains(&json!("depends")));

    // Check strength bounds
    assert_eq!(cross_ref_items["properties"]["strength"]["minimum"], 0);
    assert_eq!(cross_ref_items["properties"]["strength"]["maximum"], 1);

    // Check required fields for cross_refs
    let required: Vec<&str> = cross_ref_items["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"to_branch"));
    assert!(required.contains(&"type"));
}

#[test]
fn test_divergent_tool_num_perspectives_bounds() {
    let tool = get_divergent_tool();
    let schema = &tool.input_schema;

    assert_eq!(schema["properties"]["num_perspectives"]["minimum"], 2);
    assert_eq!(schema["properties"]["num_perspectives"]["maximum"], 5);
    assert_eq!(schema["properties"]["num_perspectives"]["type"], "integer");
}

#[test]
fn test_reflection_tool_iteration_bounds() {
    let tool = get_reflection_tool();
    let schema = &tool.input_schema;

    assert!(schema["properties"]["max_iterations"].is_object());
    assert_eq!(schema["properties"]["max_iterations"]["minimum"], 1);
    assert_eq!(schema["properties"]["max_iterations"]["maximum"], 5);

    assert!(schema["properties"]["quality_threshold"].is_object());
    assert_eq!(schema["properties"]["quality_threshold"]["minimum"], 0);
    assert_eq!(schema["properties"]["quality_threshold"]["maximum"], 1);
}

#[test]
fn test_got_init_config_schema() {
    let tool = get_got_init_tool();
    let schema = &tool.input_schema;

    assert!(schema["properties"]["config"].is_object());
    let config = &schema["properties"]["config"];

    assert!(config["properties"]["max_nodes"].is_object());
    assert_eq!(config["properties"]["max_nodes"]["minimum"], 10);
    assert_eq!(config["properties"]["max_nodes"]["maximum"], 1000);

    assert!(config["properties"]["max_depth"].is_object());
    assert_eq!(config["properties"]["max_depth"]["minimum"], 1);
    assert_eq!(config["properties"]["max_depth"]["maximum"], 20);

    assert!(config["properties"]["default_k"].is_object());
    assert_eq!(config["properties"]["default_k"]["minimum"], 1);
    assert_eq!(config["properties"]["default_k"]["maximum"], 10);

    assert!(config["properties"]["prune_threshold"].is_object());
    assert_eq!(config["properties"]["prune_threshold"]["minimum"], 0);
    assert_eq!(config["properties"]["prune_threshold"]["maximum"], 1);
}

#[test]
fn test_got_aggregate_min_items() {
    let tool = get_got_aggregate_tool();
    let schema = &tool.input_schema;

    assert_eq!(schema["properties"]["node_ids"]["minItems"], 2);
}

#[test]
fn test_all_tools_have_additional_properties_false() {
    let tools = vec![
        get_linear_tool(),
        get_tree_tool(),
        get_divergent_tool(),
        get_reflection_tool(),
        get_backtracking_tool(),
        get_auto_tool(),
        get_got_init_tool(),
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
        get_preset_list_tool(),
        get_preset_run_tool(),
        get_make_decision_tool(),
        get_analyze_perspectives_tool(),
        get_assess_evidence_tool(),
        get_probabilistic_tool(),
    ];

    for tool in tools {
        assert_eq!(
            tool.input_schema["additionalProperties"], false,
            "Tool {} should have additionalProperties: false",
            tool.name
        );
    }
}

// ============================================================================
// Tool serialization and JSON schema compliance tests
// ============================================================================

#[test]
fn test_tool_serialization_format() {
    let tool = get_linear_tool();
    let json = serde_json::to_string(&tool).unwrap();

    // Verify JSON contains expected fields
    assert!(json.contains("\"name\":\"reasoning_linear\""));
    assert!(json.contains("\"description\""));
    assert!(json.contains("\"inputSchema\""));

    // Verify it can be parsed as a Value
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["name"], "reasoning_linear");
    assert!(value["inputSchema"].is_object());
}

#[test]
fn test_all_tools_serialize_to_valid_json() {
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
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
        get_preset_list_tool(),
        get_preset_run_tool(),
        get_make_decision_tool(),
        get_analyze_perspectives_tool(),
        get_assess_evidence_tool(),
        get_probabilistic_tool(),
    ];

    for tool in tools {
        let result = serde_json::to_value(&tool);
        assert!(
            result.is_ok(),
            "Tool {} should serialize to JSON",
            tool.name
        );

        let json = result.unwrap();
        assert_eq!(json["name"], tool.name);
        assert!(json["inputSchema"].is_object());
    }
}

#[test]
fn test_tool_input_schema_has_type_object() {
    let tools = vec![
        get_linear_tool(),
        get_tree_tool(),
        get_divergent_tool(),
        get_reflection_tool(),
        get_backtracking_tool(),
        get_auto_tool(),
        get_got_init_tool(),
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
        get_make_decision_tool(),
        get_analyze_perspectives_tool(),
        get_assess_evidence_tool(),
        get_probabilistic_tool(),
    ];

    for tool in tools {
        assert_eq!(
            tool.input_schema["type"], "object",
            "Tool {} input_schema should have type: object",
            tool.name
        );
    }
}

// ============================================================================
// Complete tool count test with all phases
// ============================================================================

#[test]
fn test_complete_tools_count() {
    let tools = vec![
        // Phase 1-2
        get_linear_tool(),
        get_tree_tool(),
        get_tree_focus_tool(),
        get_tree_list_tool(),
        get_tree_complete_tool(),
        get_divergent_tool(),
        get_reflection_tool(),
        get_reflection_evaluate_tool(),
        // Phase 3
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
        // Phase 4
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
        // Phase 5
        get_preset_list_tool(),
        get_preset_run_tool(),
        // Phase 6
        get_make_decision_tool(),
        get_analyze_perspectives_tool(),
        get_assess_evidence_tool(),
        get_probabilistic_tool(),
    ];

    assert_eq!(
        tools.len(),
        28,
        "Should have exactly 28 tools defined across all phases"
    );
}

// ============================================================================
// MCP type field name validation tests
// ============================================================================

#[test]
fn test_initialize_result_field_names() {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: Capabilities {
            tools: ToolCapabilities {
                list_changed: false,
            },
        },
        server_info: ServerInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    let json = serde_json::to_string(&result).unwrap();

    // Check camelCase serialization
    assert!(json.contains("protocolVersion"));
    assert!(json.contains("serverInfo"));
    assert!(json.contains("listChanged"));
    assert!(!json.contains("protocol_version"));
    assert!(!json.contains("server_info"));
}

#[test]
fn test_tool_input_schema_field_name() {
    let tool = get_linear_tool();
    let json = serde_json::to_value(&tool).unwrap();

    // Should use camelCase
    assert!(json.get("inputSchema").is_some());
    assert!(json.get("input_schema").is_none());
}

#[test]
fn test_tool_result_content_field_name() {
    let result = ToolResultContent {
        content_type: "text".to_string(),
        text: "test".to_string(),
    };

    let json = serde_json::to_value(&result).unwrap();

    // Should serialize as "type"
    assert!(json.get("type").is_some());
    assert_eq!(json["type"], "text");
}

// ============================================================================
// Edge cases and error handling tests
// ============================================================================

#[test]
fn test_jsonrpc_response_with_null_id() {
    let response = JsonRpcResponse::success(Some(Value::Null), json!({}));
    assert_eq!(response.id, Value::Null);
}

#[test]
fn test_jsonrpc_error_with_data() {
    let mut error = JsonRpcError {
        code: -32603,
        message: "Internal error".to_string(),
        data: Some(json!({"details": "stack trace"})),
    };

    let json = serde_json::to_value(&error).unwrap();
    assert!(json.get("data").is_some());
    assert_eq!(json["data"]["details"], "stack trace");

    // Test None data is omitted
    error.data = None;
    let json = serde_json::to_value(&error).unwrap();
    assert!(json.get("data").is_none());
}

#[test]
fn test_tool_clone() {
    let tool = get_linear_tool();
    let cloned = tool.clone();

    assert_eq!(tool.name, cloned.name);
    assert_eq!(tool.description, cloned.description);
    assert_eq!(tool.input_schema, cloned.input_schema);
}

#[test]
fn test_empty_tool_result_content_vec() {
    let result = ToolCallResult {
        content: vec![],
        is_error: None,
    };

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["content"].as_array().unwrap().len(), 0);
}

#[test]
fn test_multiple_tool_result_content_items() {
    let result = ToolCallResult {
        content: vec![
            ToolResultContent {
                content_type: "text".to_string(),
                text: "First".to_string(),
            },
            ToolResultContent {
                content_type: "text".to_string(),
                text: "Second".to_string(),
            },
        ],
        is_error: None,
    };

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["content"].as_array().unwrap().len(), 2);
    assert_eq!(json["content"][0]["text"], "First");
    assert_eq!(json["content"][1]["text"], "Second");
}

// ============================================================================
// Additional Edge Cases - JsonRpcResponse
// ============================================================================

#[test]
fn test_jsonrpc_response_success_with_array_id() {
    let response = JsonRpcResponse::success(Some(json!([1, 2, 3])), json!({"result": "ok"}));
    assert_eq!(response.id, json!([1, 2, 3]));
}

#[test]
fn test_jsonrpc_response_success_with_object_id() {
    let response = JsonRpcResponse::success(Some(json!({"req": "123"})), json!({}));
    assert_eq!(response.id, json!({"req": "123"}));
}

#[test]
fn test_jsonrpc_response_error_with_empty_message() {
    let response = JsonRpcResponse::error(Some(json!(1)), -32600, "");
    assert_eq!(response.error.unwrap().message, "");
}

#[test]
fn test_jsonrpc_response_error_with_long_message() {
    let long_message = "Error: ".to_string() + &"x".repeat(1000);
    let response = JsonRpcResponse::error(Some(json!(1)), -32600, &long_message);
    assert_eq!(response.error.unwrap().message.len(), 1007);
}

#[test]
fn test_jsonrpc_response_success_with_nested_result() {
    let nested_result = json!({
        "level1": {
            "level2": {
                "level3": {
                    "value": 42
                }
            }
        }
    });
    let response = JsonRpcResponse::success(Some(json!(1)), nested_result);
    assert_eq!(
        response.result.unwrap()["level1"]["level2"]["level3"]["value"],
        42
    );
}

#[test]
fn test_jsonrpc_response_error_with_special_characters() {
    let response = JsonRpcResponse::error(Some(json!(1)), -32600, "Error: \n\t\"quoted\"");
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(serialized.contains("\\n"));
    assert!(serialized.contains("\\t"));
}

#[test]
fn test_jsonrpc_response_success_with_unicode() {
    let response = JsonRpcResponse::success(Some(json!(1)), json!({"message": "Hello 世界 🌍"}));
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(serialized.contains("世界"));
}

#[test]
fn test_jsonrpc_response_error_code_ranges() {
    // Test standard error codes
    let codes = vec![-32700, -32600, -32601, -32602, -32603, -32000, 0, 1, 100];
    for code in codes {
        let response = JsonRpcResponse::error(Some(json!(1)), code, "test");
        assert_eq!(response.error.unwrap().code, code);
    }
}

#[test]
fn test_jsonrpc_response_success_with_empty_object() {
    let response = JsonRpcResponse::success(Some(json!(1)), json!({}));
    assert!(response.result.unwrap().as_object().unwrap().is_empty());
}

#[test]
fn test_jsonrpc_response_success_with_array_result() {
    let response = JsonRpcResponse::success(Some(json!(1)), json!([1, 2, 3, 4, 5]));
    assert_eq!(response.result.unwrap().as_array().unwrap().len(), 5);
}

// ============================================================================
// Additional Edge Cases - JsonRpcRequest
// ============================================================================

#[test]
fn test_jsonrpc_request_with_nested_params() {
    let json_str =
        r#"{"jsonrpc":"2.0","id":1,"method":"test","params":{"nested":{"deep":{"value":42}}}}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.params.unwrap()["nested"]["deep"]["value"], 42);
}

#[test]
fn test_jsonrpc_request_with_array_params() {
    let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":[1,2,3]}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.params.unwrap().as_array().unwrap().len(), 3);
}

#[test]
fn test_jsonrpc_request_with_null_params() {
    // In serde_json, null deserializes to None for Option<Value>
    let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":null}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert!(request.params.is_none());
}

#[test]
fn test_jsonrpc_request_with_empty_method() {
    let json_str = r#"{"jsonrpc":"2.0","id":1,"method":""}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.method, "");
}

#[test]
fn test_jsonrpc_request_with_numeric_id_zero() {
    let json_str = r#"{"jsonrpc":"2.0","id":0,"method":"test"}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.id, Some(json!(0)));
}

#[test]
fn test_jsonrpc_request_with_negative_id() {
    let json_str = r#"{"jsonrpc":"2.0","id":-1,"method":"test"}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.id, Some(json!(-1)));
}

#[test]
fn test_jsonrpc_request_with_float_id() {
    let json_str = r#"{"jsonrpc":"2.0","id":3.14,"method":"test"}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.id, Some(json!(3.14)));
}

// ============================================================================
// Invalid JSON Parsing Tests
// ============================================================================

#[test]
fn test_jsonrpc_request_missing_jsonrpc_field() {
    let json_str = r#"{"id":1,"method":"test"}"#;
    let result: Result<JsonRpcRequest, _> = serde_json::from_str(json_str);
    assert!(result.is_err());
}

#[test]
fn test_jsonrpc_request_missing_method_field() {
    let json_str = r#"{"jsonrpc":"2.0","id":1}"#;
    let result: Result<JsonRpcRequest, _> = serde_json::from_str(json_str);
    assert!(result.is_err());
}

#[test]
fn test_jsonrpc_request_invalid_json() {
    let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"test""#;
    let result: Result<JsonRpcRequest, _> = serde_json::from_str(json_str);
    assert!(result.is_err());
}

#[test]
fn test_jsonrpc_request_wrong_jsonrpc_version() {
    let json_str = r#"{"jsonrpc":"1.0","id":1,"method":"test"}"#;
    let request: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(request.jsonrpc, "1.0");
}

#[test]
fn test_jsonrpc_request_empty_string() {
    let result: Result<JsonRpcRequest, _> = serde_json::from_str("");
    assert!(result.is_err());
}

#[test]
fn test_jsonrpc_request_not_an_object() {
    let json_str = r#"["jsonrpc","2.0"]"#;
    let result: Result<JsonRpcRequest, _> = serde_json::from_str(json_str);
    assert!(result.is_err());
}

// ============================================================================
// ToolCallParams Edge Cases
// ============================================================================

#[test]
fn test_tool_call_params_with_null_arguments() {
    // In serde_json, null deserializes to None for Option<Value>
    let json_str = r#"{"name":"test_tool","arguments":null}"#;
    let params: ToolCallParams = serde_json::from_str(json_str).unwrap();
    assert_eq!(params.name, "test_tool");
    assert!(params.arguments.is_none());
}

#[test]
fn test_tool_call_params_with_empty_name() {
    let json_str = r#"{"name":""}"#;
    let params: ToolCallParams = serde_json::from_str(json_str).unwrap();
    assert_eq!(params.name, "");
}

#[test]
fn test_tool_call_params_with_complex_arguments() {
    let json_str =
        r#"{"name":"test","arguments":{"nested":{"array":[1,2,3],"obj":{"key":"value"}}}}"#;
    let params: ToolCallParams = serde_json::from_str(json_str).unwrap();
    let args = params.arguments.unwrap();
    assert_eq!(args["nested"]["array"][0], 1);
    assert_eq!(args["nested"]["obj"]["key"], "value");
}

#[test]
fn test_tool_call_params_with_array_arguments() {
    let json_str = r#"{"name":"test","arguments":[1,2,3,4,5]}"#;
    let params: ToolCallParams = serde_json::from_str(json_str).unwrap();
    assert_eq!(params.arguments.unwrap().as_array().unwrap().len(), 5);
}

#[test]
fn test_tool_call_params_missing_name() {
    let json_str = r#"{"arguments":{}}"#;
    let result: Result<ToolCallParams, _> = serde_json::from_str(json_str);
    assert!(result.is_err());
}

// ============================================================================
// Tool Definition Schema Validation
// ============================================================================

#[test]
fn test_all_phase_tools_have_descriptions() {
    let tools = vec![
        get_linear_tool(),
        get_tree_tool(),
        get_divergent_tool(),
        get_reflection_tool(),
        get_backtracking_tool(),
        get_auto_tool(),
        get_got_init_tool(),
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
        get_preset_list_tool(),
        get_preset_run_tool(),
        get_make_decision_tool(),
        get_analyze_perspectives_tool(),
        get_assess_evidence_tool(),
        get_probabilistic_tool(),
    ];

    for tool in tools {
        assert!(
            !tool.description.is_empty(),
            "Tool {} missing description",
            tool.name
        );
        assert!(
            tool.description.len() > 10,
            "Tool {} description too short",
            tool.name
        );
    }
}

#[test]
fn test_all_tools_start_with_reasoning_prefix() {
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
        get_detect_biases_tool(),
        get_detect_fallacies_tool(),
        get_preset_list_tool(),
        get_preset_run_tool(),
        get_make_decision_tool(),
        get_analyze_perspectives_tool(),
        get_assess_evidence_tool(),
        get_probabilistic_tool(),
    ];

    for tool in tools {
        assert!(
            tool.name.starts_with("reasoning_"),
            "Tool {} should start with 'reasoning_'",
            tool.name
        );
    }
}

#[test]
fn test_all_tools_have_properties_object() {
    let tools = vec![
        get_linear_tool(),
        get_tree_tool(),
        get_divergent_tool(),
        get_reflection_tool(),
        get_backtracking_tool(),
        get_auto_tool(),
        get_got_init_tool(),
    ];

    for tool in tools {
        let schema = &tool.input_schema;
        assert!(
            schema["properties"].is_object(),
            "Tool {} should have properties object",
            tool.name
        );
        assert!(
            !schema["properties"].as_object().unwrap().is_empty(),
            "Tool {} should have at least one property",
            tool.name
        );
    }
}

// ============================================================================
// ToolCallResult Edge Cases
// ============================================================================

#[test]
fn test_tool_call_result_with_error_false() {
    let result = ToolCallResult {
        content: vec![ToolResultContent {
            content_type: "text".to_string(),
            text: "Success".to_string(),
        }],
        is_error: Some(false),
    };

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["isError"], false);
}

#[test]
fn test_tool_call_result_with_multiline_text() {
    let result = ToolCallResult {
        content: vec![ToolResultContent {
            content_type: "text".to_string(),
            text: "Line 1\nLine 2\nLine 3".to_string(),
        }],
        is_error: None,
    };

    let json = serde_json::to_value(&result).unwrap();
    assert!(json["content"][0]["text"].as_str().unwrap().contains('\n'));
}

#[test]
fn test_tool_call_result_with_empty_text() {
    let result = ToolCallResult {
        content: vec![ToolResultContent {
            content_type: "text".to_string(),
            text: "".to_string(),
        }],
        is_error: None,
    };

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["content"][0]["text"], "");
}

#[test]
fn test_tool_call_result_with_different_content_types() {
    let result = ToolCallResult {
        content: vec![
            ToolResultContent {
                content_type: "text".to_string(),
                text: "Text content".to_string(),
            },
            ToolResultContent {
                content_type: "json".to_string(),
                text: r#"{"key":"value"}"#.to_string(),
            },
        ],
        is_error: None,
    };

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["content"][0]["type"], "text");
    assert_eq!(json["content"][1]["type"], "json");
}

// ============================================================================
// Tool-specific Schema Validation
// ============================================================================

#[test]
fn test_backtracking_tool_confidence_default() {
    let tool = get_backtracking_tool();
    let schema = &tool.input_schema;

    assert!(schema["properties"]["confidence"].is_object());
    assert_eq!(schema["properties"]["confidence"]["minimum"], 0);
    assert_eq!(schema["properties"]["confidence"]["maximum"], 1);
}

#[test]
fn test_got_prune_threshold_default() {
    let tool = get_got_prune_tool();
    let schema = &tool.input_schema;

    assert!(schema["properties"]["threshold"].is_object());
    assert_eq!(schema["properties"]["threshold"]["type"], "number");
    assert_eq!(schema["properties"]["threshold"]["minimum"], 0);
    assert_eq!(schema["properties"]["threshold"]["maximum"], 1);
}

#[test]
fn test_preset_run_inputs_additional_properties() {
    let tool = get_preset_run_tool();
    let schema = &tool.input_schema;

    assert!(schema["properties"]["inputs"].is_object());
    assert_eq!(schema["properties"]["inputs"]["additionalProperties"], true);
}

#[test]
fn test_detect_biases_check_types_is_array() {
    let tool = get_detect_biases_tool();
    let schema = &tool.input_schema;

    assert_eq!(schema["properties"]["check_types"]["type"], "array");
    assert!(schema["properties"]["check_types"]["items"].is_object());
}

// ============================================================================
// Error Code Constants Tests
// ============================================================================

#[test]
fn test_parse_error_code() {
    let response = JsonRpcResponse::error(None, -32700, "Parse error");
    assert_eq!(response.error.unwrap().code, -32700);
}

#[test]
fn test_invalid_request_code() {
    let response = JsonRpcResponse::error(None, -32600, "Invalid request");
    assert_eq!(response.error.unwrap().code, -32600);
}

#[test]
fn test_method_not_found_code() {
    let response = JsonRpcResponse::error(None, -32601, "Method not found");
    assert_eq!(response.error.unwrap().code, -32601);
}

#[test]
fn test_invalid_params_code() {
    let response = JsonRpcResponse::error(None, -32602, "Invalid params");
    assert_eq!(response.error.unwrap().code, -32602);
}

#[test]
fn test_internal_error_code() {
    let response = JsonRpcResponse::error(None, -32603, "Internal error");
    assert_eq!(response.error.unwrap().code, -32603);
}

// ============================================================================
// Serialization Round-trip Tests
// ============================================================================

#[test]
fn test_jsonrpc_response_roundtrip() {
    let response = JsonRpcResponse::success(Some(json!(42)), json!({"result": "test"}));
    let serialized = serde_json::to_string(&response).unwrap();
    let deserialized: Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["id"], 42);
    assert_eq!(deserialized["result"]["result"], "test");
}

#[test]
fn test_tool_roundtrip() {
    let tool = get_linear_tool();
    let serialized = serde_json::to_string(&tool).unwrap();
    let deserialized: Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["name"], "reasoning_linear");
    assert!(deserialized["inputSchema"].is_object());
}

#[test]
fn test_initialize_result_roundtrip() {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: Capabilities {
            tools: ToolCapabilities {
                list_changed: false,
            },
        },
        server_info: ServerInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    let serialized = serde_json::to_string(&result).unwrap();
    let deserialized: Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["protocolVersion"], "2024-11-05");
    assert_eq!(deserialized["serverInfo"]["name"], "test");
}
