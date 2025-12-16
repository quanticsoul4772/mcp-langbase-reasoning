use serde_json::Value;
use tracing::info;

use super::SharedState;
use crate::error::{McpError, McpResult};
use crate::modes::{
    AutoParams, BacktrackingParams, DivergentParams, GotAggregateParams, GotFinalizeParams,
    GotGenerateParams, GotGetStateParams, GotInitParams, GotPruneParams, GotRefineParams,
    GotScoreParams, LinearParams, ReflectionParams, TreeParams,
};
use crate::storage::BranchState;

/// Route tool calls to appropriate handlers
pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value> {
    info!(tool = %tool_name, "Routing tool call");

    match tool_name {
        // Phase 1-2 tools
        "reasoning_linear" => handle_linear(state, arguments).await,
        "reasoning_tree" => handle_tree(state, arguments).await,
        "reasoning_tree_focus" => handle_tree_focus(state, arguments).await,
        "reasoning_tree_list" => handle_tree_list(state, arguments).await,
        "reasoning_tree_complete" => handle_tree_complete(state, arguments).await,
        "reasoning_divergent" => handle_divergent(state, arguments).await,
        "reasoning_reflection" => handle_reflection(state, arguments).await,
        "reasoning_reflection_evaluate" => handle_reflection_evaluate(state, arguments).await,
        // Phase 3 tools - Backtracking
        "reasoning_backtrack" => handle_backtrack(state, arguments).await,
        "reasoning_checkpoint_create" => handle_checkpoint_create(state, arguments).await,
        "reasoning_checkpoint_list" => handle_checkpoint_list(state, arguments).await,
        // Phase 3 tools - Auto Router
        "reasoning_auto" => handle_auto(state, arguments).await,
        // Phase 3 tools - Graph-of-Thoughts
        "reasoning_got_init" => handle_got_init(state, arguments).await,
        "reasoning_got_generate" => handle_got_generate(state, arguments).await,
        "reasoning_got_score" => handle_got_score(state, arguments).await,
        "reasoning_got_aggregate" => handle_got_aggregate(state, arguments).await,
        "reasoning_got_refine" => handle_got_refine(state, arguments).await,
        "reasoning_got_prune" => handle_got_prune(state, arguments).await,
        "reasoning_got_finalize" => handle_got_finalize(state, arguments).await,
        "reasoning_got_state" => handle_got_state(state, arguments).await,
        _ => Err(McpError::UnknownTool {
            tool_name: tool_name.to_string(),
        }),
    }
}

/// Handle reasoning.linear tool call
async fn handle_linear(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: LinearParams = parse_arguments("reasoning.linear", arguments)?;

    let result = state
        .linear_mode
        .process(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.tree tool call
async fn handle_tree(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: TreeParams = parse_arguments("reasoning.tree", arguments)?;

    let result = state
        .tree_mode
        .process(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.tree.focus - focus on a specific branch
async fn handle_tree_focus(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(serde::Deserialize)]
    struct FocusParams {
        session_id: String,
        branch_id: String,
    }

    let params: FocusParams = parse_arguments("reasoning.tree.focus", arguments)?;

    let result = state
        .tree_mode
        .focus_branch(&params.session_id, &params.branch_id)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.tree.list - list all branches in a session
async fn handle_tree_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(serde::Deserialize)]
    struct ListParams {
        session_id: String,
    }

    let params: ListParams = parse_arguments("reasoning.tree.list", arguments)?;

    let result = state
        .tree_mode
        .list_branches(&params.session_id)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.tree.complete - mark a branch as completed or abandoned
async fn handle_tree_complete(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(serde::Deserialize)]
    struct CompleteParams {
        branch_id: String,
        #[serde(default = "default_completed")]
        completed: bool,
    }

    fn default_completed() -> bool {
        true
    }

    let params: CompleteParams = parse_arguments("reasoning.tree.complete", arguments)?;

    let state_to_set = if params.completed {
        BranchState::Completed
    } else {
        BranchState::Abandoned
    };

    let result = state
        .tree_mode
        .update_branch_state(&params.branch_id, state_to_set)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.divergent tool call
async fn handle_divergent(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: DivergentParams = parse_arguments("reasoning.divergent", arguments)?;

    let result = state
        .divergent_mode
        .process(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.reflection tool call
async fn handle_reflection(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: ReflectionParams = parse_arguments("reasoning.reflection", arguments)?;

    let result = state
        .reflection_mode
        .process(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.reflection.evaluate - evaluate a session's reasoning quality
async fn handle_reflection_evaluate(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(serde::Deserialize)]
    struct EvaluateParams {
        session_id: String,
    }

    let params: EvaluateParams = parse_arguments("reasoning.reflection.evaluate", arguments)?;

    let result = state
        .reflection_mode
        .evaluate_session(&params.session_id)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

// ============================================================================
// Phase 3 Handlers - Backtracking
// ============================================================================

/// Handle reasoning.backtrack tool call
async fn handle_backtrack(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: BacktrackingParams = parse_arguments("reasoning.backtrack", arguments)?;

    let result = state
        .backtracking_mode
        .process(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.checkpoint.create tool call
async fn handle_checkpoint_create(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(serde::Deserialize)]
    struct CreateParams {
        session_id: String,
        name: String,
        description: Option<String>,
    }

    let params: CreateParams = parse_arguments("reasoning.checkpoint.create", arguments)?;

    let result = state
        .backtracking_mode
        .create_checkpoint(&params.session_id, &params.name, params.description.as_deref())
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.checkpoint.list tool call
async fn handle_checkpoint_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(serde::Deserialize)]
    struct ListParams {
        session_id: String,
    }

    let params: ListParams = parse_arguments("reasoning.checkpoint.list", arguments)?;

    let result = state
        .backtracking_mode
        .list_checkpoints(&params.session_id)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

// ============================================================================
// Phase 3 Handlers - Auto Router
// ============================================================================

/// Handle reasoning.auto tool call
async fn handle_auto(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: AutoParams = parse_arguments("reasoning.auto", arguments)?;

    let result = state
        .auto_mode
        .route(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

// ============================================================================
// Phase 3 Handlers - Graph-of-Thoughts
// ============================================================================

/// Handle reasoning.got.init tool call
async fn handle_got_init(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotInitParams = parse_arguments("reasoning.got.init", arguments)?;

    let result = state
        .got_mode
        .initialize(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.generate tool call
async fn handle_got_generate(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotGenerateParams = parse_arguments("reasoning.got.generate", arguments)?;

    let result = state
        .got_mode
        .generate(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.score tool call
async fn handle_got_score(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotScoreParams = parse_arguments("reasoning.got.score", arguments)?;

    let result = state
        .got_mode
        .score(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.aggregate tool call
async fn handle_got_aggregate(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotAggregateParams = parse_arguments("reasoning.got.aggregate", arguments)?;

    let result = state
        .got_mode
        .aggregate(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.refine tool call
async fn handle_got_refine(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotRefineParams = parse_arguments("reasoning.got.refine", arguments)?;

    let result = state
        .got_mode
        .refine(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.prune tool call
async fn handle_got_prune(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotPruneParams = parse_arguments("reasoning.got.prune", arguments)?;

    let result = state
        .got_mode
        .prune(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.finalize tool call
async fn handle_got_finalize(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotFinalizeParams = parse_arguments("reasoning.got.finalize", arguments)?;

    let result = state
        .got_mode
        .finalize(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

/// Handle reasoning.got.state tool call
async fn handle_got_state(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: GotGetStateParams = parse_arguments("reasoning.got.state", arguments)?;

    let result = state
        .got_mode
        .get_state(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Helper to parse arguments with consistent error handling
fn parse_arguments<T: serde::de::DeserializeOwned>(
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<T> {
    match arguments {
        Some(args) => serde_json::from_value(args).map_err(|e| McpError::InvalidParameters {
            tool_name: tool_name.to_string(),
            message: e.to_string(),
        }),
        None => Err(McpError::InvalidParameters {
            tool_name: tool_name.to_string(),
            message: "Missing arguments".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestParams {
        content: String,
        value: i32,
    }

    #[test]
    fn test_parse_arguments_success() {
        let args = Some(json!({
            "content": "test content",
            "value": 42
        }));

        let result: McpResult<TestParams> = parse_arguments("test.tool", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "test content");
        assert_eq!(params.value, 42);
    }

    #[test]
    fn test_parse_arguments_missing_arguments() {
        let result: McpResult<TestParams> = parse_arguments("test.tool", None);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, McpError::InvalidParameters { .. }));
        assert!(err.to_string().contains("Missing arguments"));
        assert!(err.to_string().contains("test.tool"));
    }

    #[test]
    fn test_parse_arguments_invalid_json() {
        let args = Some(json!({
            "content": "test",
            // missing "value" field
        }));

        let result: McpResult<TestParams> = parse_arguments("reasoning.linear", args);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, McpError::InvalidParameters { .. }));
        assert!(err.to_string().contains("reasoning.linear"));
    }

    #[test]
    fn test_parse_arguments_wrong_type() {
        let args = Some(json!({
            "content": "test",
            "value": "not a number"  // wrong type
        }));

        let result: McpResult<TestParams> = parse_arguments("test.tool", args);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, McpError::InvalidParameters { .. }));
    }

    #[test]
    fn test_parse_arguments_extra_fields_ignored() {
        let args = Some(json!({
            "content": "test",
            "value": 10,
            "extra_field": "should be ignored"
        }));

        let result: McpResult<TestParams> = parse_arguments("test.tool", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "test");
        assert_eq!(params.value, 10);
    }

    #[test]
    fn test_parse_linear_params() {
        let args = Some(json!({
            "content": "What is 2+2?"
        }));

        let result: McpResult<LinearParams> = parse_arguments("reasoning.linear", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "What is 2+2?");
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_parse_linear_params_with_session() {
        let args = Some(json!({
            "content": "Continue reasoning",
            "session_id": "sess-123"
        }));

        let result: McpResult<LinearParams> = parse_arguments("reasoning.linear", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "Continue reasoning");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_parse_tree_params() {
        let args = Some(json!({
            "content": "Explore options"
        }));

        let result: McpResult<TreeParams> = parse_arguments("reasoning.tree", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "Explore options");
    }

    #[test]
    fn test_parse_tree_params_with_all_fields() {
        let args = Some(json!({
            "content": "Branch thought",
            "session_id": "sess-456",
            "branch_id": "branch-789",
            "num_branches": 4
        }));

        let result: McpResult<TreeParams> = parse_arguments("reasoning.tree", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "Branch thought");
        assert_eq!(params.session_id, Some("sess-456".to_string()));
        assert_eq!(params.branch_id, Some("branch-789".to_string()));
        assert_eq!(params.num_branches, 4);
    }

    #[test]
    fn test_parse_divergent_params() {
        let args = Some(json!({
            "content": "Generate perspectives"
        }));

        let result: McpResult<DivergentParams> = parse_arguments("reasoning.divergent", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "Generate perspectives");
    }

    #[test]
    fn test_parse_divergent_params_with_count() {
        let args = Some(json!({
            "content": "Generate perspectives",
            "num_perspectives": 5
        }));

        let result: McpResult<DivergentParams> = parse_arguments("reasoning.divergent", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "Generate perspectives");
        assert_eq!(params.num_perspectives, 5);
    }

    #[test]
    fn test_parse_reflection_params() {
        let args = Some(json!({
            "content": "Reflect on reasoning"
        }));

        let result: McpResult<ReflectionParams> = parse_arguments("reasoning.reflection", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, Some("Reflect on reasoning".to_string()));
    }

    #[test]
    fn test_parse_reflection_params_with_thought_id() {
        let args = Some(json!({
            "thought_id": "thought-123",
            "max_iterations": 5
        }));

        let result: McpResult<ReflectionParams> = parse_arguments("reasoning.reflection", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.thought_id, Some("thought-123".to_string()));
        assert_eq!(params.max_iterations, 5);
    }

    #[test]
    fn test_parse_reflection_params_with_all_fields() {
        let args = Some(json!({
            "content": "Deep reflection",
            "session_id": "sess-123",
            "max_iterations": 5,
            "quality_threshold": 0.9,
            "include_chain": true
        }));

        let result: McpResult<ReflectionParams> = parse_arguments("reasoning.reflection", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, Some("Deep reflection".to_string()));
        assert_eq!(params.session_id, Some("sess-123".to_string()));
        assert_eq!(params.max_iterations, 5);
        assert_eq!(params.quality_threshold, 0.9);
        assert!(params.include_chain);
    }
}
