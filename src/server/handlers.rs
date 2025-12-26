use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::info;

use super::SharedState;
use crate::error::{McpError, McpResult};
use crate::modes::{
    AutoParams, BacktrackingParams, DecisionParams, DetectBiasesParams, DetectFallaciesParams,
    DivergentParams, EvidenceParams, GotAggregateParams, GotFinalizeParams, GotGenerateParams,
    GotGetStateParams, GotInitParams, GotPruneParams, GotRefineParams, GotScoreParams,
    LinearParams, PerspectiveParams, ProbabilisticParams, ReflectionParams, TreeParams,
};
use crate::presets::execute_preset;
use crate::self_improvement::InvocationEvent;
use crate::storage::BranchState;

// ============================================================================
// Auxiliary Handler Param Structs
// ============================================================================

/// Parameters for tree focus operation
#[derive(Debug, Clone, Deserialize)]
pub struct TreeFocusParams {
    /// Session ID containing the branch
    pub session_id: String,
    /// Branch ID to focus on
    pub branch_id: String,
}

/// Parameters for tree list operation
#[derive(Debug, Clone, Deserialize)]
pub struct TreeListParams {
    /// Session ID to list branches for
    pub session_id: String,
}

/// Parameters for tree complete operation
#[derive(Debug, Clone, Deserialize)]
pub struct TreeCompleteParams {
    /// Branch ID to mark as complete/abandoned
    pub branch_id: String,
    /// Whether to mark as completed (true) or abandoned (false)
    #[serde(default = "default_completed")]
    pub completed: bool,
}

fn default_completed() -> bool {
    true
}

/// Parameters for reflection evaluate operation
#[derive(Debug, Clone, Deserialize)]
pub struct ReflectionEvaluateParams {
    /// Session ID to evaluate
    pub session_id: String,
}

/// Parameters for checkpoint create operation
#[derive(Debug, Clone, Deserialize)]
pub struct CheckpointCreateParams {
    /// Session ID to create checkpoint for
    pub session_id: String,
    /// Name for the checkpoint
    pub name: String,
    /// Optional description
    pub description: Option<String>,
}

/// Parameters for checkpoint list operation
#[derive(Debug, Clone, Deserialize)]
pub struct CheckpointListParams {
    /// Session ID to list checkpoints for
    pub session_id: String,
}

/// Route tool calls to appropriate handlers
pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value> {
    info!(tool = %tool_name, "Routing tool call");

    // Start timing for self-improvement tracking
    let start = std::time::Instant::now();

    let result = match tool_name {
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
        // Phase 4 tools - Bias & Fallacy Detection
        "reasoning_detect_biases" => handle_detect_biases(state, arguments).await,
        "reasoning_detect_fallacies" => handle_detect_fallacies(state, arguments).await,
        // Phase 5 tools - Workflow Presets
        "reasoning_preset_list" => handle_preset_list(state, arguments).await,
        "reasoning_preset_run" => handle_preset_run(state, arguments).await,
        // Phase 6 tools - Decision Framework & Evidence Assessment
        "reasoning_make_decision" => handle_make_decision(state, arguments).await,
        "reasoning_analyze_perspectives" => handle_analyze_perspectives(state, arguments).await,
        "reasoning_assess_evidence" => handle_assess_evidence(state, arguments).await,
        "reasoning_probabilistic" => handle_probabilistic(state, arguments).await,
        // Metrics tools
        "reasoning_metrics_summary" => handle_metrics_summary(state).await,
        "reasoning_metrics_by_pipe" => handle_metrics_by_pipe(state, arguments).await,
        "reasoning_metrics_invocations" => handle_metrics_invocations(state, arguments).await,
        "reasoning_fallback_metrics" => handle_fallback_metrics(state).await,
        "reasoning_debug_config" => handle_debug_config(state).await,
        _ => Err(McpError::UnknownTool {
            tool_name: tool_name.to_string(),
        }),
    };

    // Record invocation for self-improvement system (if enabled)
    let latency_ms = start.elapsed().as_millis() as i64;
    let success = result.is_ok();

    // Extract quality score from response if available
    let quality_score = result
        .as_ref()
        .ok()
        .and_then(|v| v.get("confidence"))
        .and_then(|c| c.as_f64());

    state
        .record_invocation(InvocationEvent {
            tool_name: tool_name.to_string(),
            latency_ms,
            success,
            quality_score,
            timestamp: chrono::Utc::now(),
        })
        .await;

    result
}

/// Handle reasoning.linear tool call
async fn handle_linear(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler("reasoning.linear", arguments, |params: LinearParams| {
        state.linear_mode.process(params)
    })
    .await
}

/// Handle reasoning.tree tool call
async fn handle_tree(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler("reasoning.tree", arguments, |params: TreeParams| {
        state.tree_mode.process(params)
    })
    .await
}

/// Handle reasoning.tree.focus - focus on a specific branch
async fn handle_tree_focus(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.tree.focus",
        arguments,
        |params: TreeFocusParams| {
            let session_id = params.session_id;
            let branch_id = params.branch_id;
            async move { state.tree_mode.focus_branch(&session_id, &branch_id).await }
        },
    )
    .await
}

/// Handle reasoning.tree.list - list all branches in a session
async fn handle_tree_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.tree.list",
        arguments,
        |params: TreeListParams| {
            let session_id = params.session_id;
            async move { state.tree_mode.list_branches(&session_id).await }
        },
    )
    .await
}

/// Handle reasoning.tree.complete - mark a branch as completed or abandoned
async fn handle_tree_complete(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.tree.complete",
        arguments,
        |params: TreeCompleteParams| {
            let branch_id = params.branch_id;
            let branch_state = if params.completed {
                BranchState::Completed
            } else {
                BranchState::Abandoned
            };
            async move {
                state
                    .tree_mode
                    .update_branch_state(&branch_id, branch_state)
                    .await
            }
        },
    )
    .await
}

/// Handle reasoning.divergent tool call
async fn handle_divergent(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.divergent",
        arguments,
        |params: DivergentParams| state.divergent_mode.process(params),
    )
    .await
}

/// Handle reasoning.reflection tool call
async fn handle_reflection(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.reflection",
        arguments,
        |params: ReflectionParams| state.reflection_mode.process(params),
    )
    .await
}

/// Handle reasoning.reflection.evaluate - evaluate a session's reasoning quality
async fn handle_reflection_evaluate(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
    execute_handler(
        "reasoning.reflection.evaluate",
        arguments,
        |params: ReflectionEvaluateParams| {
            let session_id = params.session_id;
            async move { state.reflection_mode.evaluate_session(&session_id).await }
        },
    )
    .await
}

// ============================================================================
// Phase 3 Handlers - Backtracking
// ============================================================================

/// Handle reasoning.backtrack tool call
async fn handle_backtrack(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.backtrack",
        arguments,
        |params: BacktrackingParams| state.backtracking_mode.process(params),
    )
    .await
}

/// Handle reasoning.checkpoint.create tool call
async fn handle_checkpoint_create(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
    execute_handler(
        "reasoning.checkpoint.create",
        arguments,
        |params: CheckpointCreateParams| {
            let session_id = params.session_id;
            let name = params.name;
            let description = params.description;
            async move {
                state
                    .backtracking_mode
                    .create_checkpoint(&session_id, &name, description.as_deref())
                    .await
            }
        },
    )
    .await
}

/// Handle reasoning.checkpoint.list tool call
async fn handle_checkpoint_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.checkpoint.list",
        arguments,
        |params: CheckpointListParams| {
            let session_id = params.session_id;
            async move { state.backtracking_mode.list_checkpoints(&session_id).await }
        },
    )
    .await
}

// ============================================================================
// Phase 3 Handlers - Auto Router
// ============================================================================

/// Handle reasoning.auto tool call
async fn handle_auto(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler("reasoning.auto", arguments, |params: AutoParams| {
        state.auto_mode.route(params)
    })
    .await
}

// ============================================================================
// Phase 3 Handlers - Graph-of-Thoughts
// ============================================================================

/// Handle reasoning.got.init tool call
async fn handle_got_init(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler("reasoning.got.init", arguments, |params: GotInitParams| {
        state.got_mode.initialize(params)
    })
    .await
}

/// Handle reasoning.got.generate tool call
async fn handle_got_generate(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.generate",
        arguments,
        |params: GotGenerateParams| state.got_mode.generate(params),
    )
    .await
}

/// Handle reasoning.got.score tool call
async fn handle_got_score(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.score",
        arguments,
        |params: GotScoreParams| state.got_mode.score(params),
    )
    .await
}

/// Handle reasoning.got.aggregate tool call
async fn handle_got_aggregate(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.aggregate",
        arguments,
        |params: GotAggregateParams| state.got_mode.aggregate(params),
    )
    .await
}

/// Handle reasoning.got.refine tool call
async fn handle_got_refine(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.refine",
        arguments,
        |params: GotRefineParams| state.got_mode.refine(params),
    )
    .await
}

/// Handle reasoning.got.prune tool call
async fn handle_got_prune(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.prune",
        arguments,
        |params: GotPruneParams| state.got_mode.prune(params),
    )
    .await
}

/// Handle reasoning.got.finalize tool call
async fn handle_got_finalize(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.finalize",
        arguments,
        |params: GotFinalizeParams| state.got_mode.finalize(params),
    )
    .await
}

/// Handle reasoning.got.state tool call
async fn handle_got_state(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.got.state",
        arguments,
        |params: GotGetStateParams| state.got_mode.get_state(params),
    )
    .await
}

// ============================================================================
// Phase 4 Handlers - Bias & Fallacy Detection
// ============================================================================

/// Handle reasoning_detect_biases tool call
async fn handle_detect_biases(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.detect_biases",
        arguments,
        |params: DetectBiasesParams| state.detection_mode.detect_biases(params),
    )
    .await
}

/// Handle reasoning_detect_fallacies tool call
async fn handle_detect_fallacies(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
    execute_handler(
        "reasoning.detect_fallacies",
        arguments,
        |params: DetectFallaciesParams| state.detection_mode.detect_fallacies(params),
    )
    .await
}

// ============================================================================
// Phase 5 Handlers - Workflow Presets
// ============================================================================

/// Parameters for preset list
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PresetListParams {
    /// Optional category filter
    #[serde(default)]
    pub category: Option<String>,
}

/// Parameters for preset run
#[derive(Debug, Clone, Deserialize)]
pub struct PresetRunParams {
    /// ID of the preset to run
    pub preset_id: String,
    /// Input parameters for the workflow
    #[serde(default)]
    pub inputs: HashMap<String, serde_json::Value>,
    /// Optional session ID for context persistence
    pub session_id: Option<String>,
}

/// Response for preset list
#[derive(Debug, Clone, Serialize)]
pub struct PresetListResponse {
    /// Available presets
    pub presets: Vec<crate::presets::PresetSummary>,
    /// Total count
    pub count: usize,
    /// Available categories
    pub categories: Vec<String>,
}

/// Handle reasoning_preset_list tool call
async fn handle_preset_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    // Use default params if no arguments provided (allows calling with no args)
    let params: PresetListParams = parse_arguments_or_default(arguments)?;

    info!(category = ?params.category, "Listing presets");

    let presets = state.preset_registry.list(params.category.as_deref());
    let categories = state.preset_registry.categories();
    let count = presets.len();

    let response = PresetListResponse {
        presets,
        count,
        categories,
    };

    serde_json::to_value(response).map_err(McpError::Json)
}

/// Handle reasoning_preset_run tool call
async fn handle_preset_run(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: PresetRunParams = parse_arguments("reasoning_preset_run", arguments)?;

    info!(preset_id = %params.preset_id, "Running preset");

    // Get the preset from registry
    let preset = state
        .preset_registry
        .get(&params.preset_id)
        .ok_or_else(|| McpError::InvalidParameters {
            tool_name: "reasoning_preset_run".to_string(),
            message: format!("Preset not found: {}", params.preset_id),
        })?;

    // Build inputs with session_id if provided
    let mut inputs = params.inputs;
    if let Some(session_id) = params.session_id {
        inputs.insert("session_id".to_string(), serde_json::json!(session_id));
    }

    // Execute the preset using Box::pin to handle async recursion
    let state_clone = state.clone();
    let result = Box::pin(execute_preset(&state_clone, &preset, inputs))
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: format!("Preset execution failed: {}", e),
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

/// Helper to parse arguments with Default fallback for missing arguments.
///
/// This is useful for tools that can be called with no arguments.
fn parse_arguments_or_default<T: serde::de::DeserializeOwned + Default>(
    arguments: Option<Value>,
) -> McpResult<T> {
    match arguments {
        Some(args) => serde_json::from_value(args).map_err(|e| McpError::InvalidParameters {
            tool_name: "parse_arguments_or_default".to_string(),
            message: e.to_string(),
        }),
        None => Ok(T::default()),
    }
}

/// Generic handler that executes a mode operation with consistent error handling.
///
/// This helper reduces boilerplate by handling:
/// - Argument parsing with typed deserialization
/// - Error conversion to McpError
/// - Result serialization to JSON Value
///
/// # Type Parameters
/// - `P`: Parameter type (must implement DeserializeOwned)
/// - `R`: Result type (must implement Serialize)
/// - `E`: Error type (must implement Display)
/// - `F`: Async operation that takes P and returns Result<R, E>
async fn execute_handler<P, R, E, F, Fut>(
    tool_name: &str,
    arguments: Option<Value>,
    operation: F,
) -> McpResult<Value>
where
    P: serde::de::DeserializeOwned,
    R: Serialize,
    E: std::fmt::Display,
    F: FnOnce(P) -> Fut,
    Fut: std::future::Future<Output = Result<R, E>>,
{
    let params: P = parse_arguments(tool_name, arguments)?;

    let result = operation(params)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: e.to_string(),
        })?;

    serde_json::to_value(result).map_err(McpError::Json)
}

// ============================================================================
// Phase 6 Handlers - Decision Framework & Evidence Assessment
// ============================================================================

/// Handle reasoning_make_decision tool call
async fn handle_make_decision(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.make_decision",
        arguments,
        |params: DecisionParams| state.decision_mode.make_decision(params),
    )
    .await
}

/// Handle reasoning_analyze_perspectives tool call
async fn handle_analyze_perspectives(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
    execute_handler(
        "reasoning.analyze_perspectives",
        arguments,
        |params: PerspectiveParams| state.decision_mode.analyze_perspectives(params),
    )
    .await
}

/// Handle reasoning_assess_evidence tool call
async fn handle_assess_evidence(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.assess_evidence",
        arguments,
        |params: EvidenceParams| state.evidence_mode.assess_evidence(params),
    )
    .await
}

/// Handle reasoning_probabilistic tool call
async fn handle_probabilistic(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.probabilistic",
        arguments,
        |params: ProbabilisticParams| state.evidence_mode.update_probability(params),
    )
    .await
}

// ============================================================================
// Metrics Handlers
// ============================================================================

/// Parameters for metrics by pipe query
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsByPipeParams {
    /// Name of the pipe to get metrics for
    pub pipe_name: String,
}

/// Parameters for metrics invocations query
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetricsInvocationsParams {
    /// Filter by pipe name
    #[serde(default)]
    pub pipe_name: Option<String>,
    /// Filter by tool name
    #[serde(default)]
    pub tool_name: Option<String>,
    /// Filter by session ID
    #[serde(default)]
    pub session_id: Option<String>,
    /// If true, only successful calls; if false, only failed calls
    #[serde(default)]
    pub success_only: Option<bool>,
    /// Maximum number of results to return
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Handle reasoning_metrics_summary tool call
async fn handle_metrics_summary(state: &SharedState) -> McpResult<Value> {
    use crate::storage::Storage;

    info!("Handling metrics summary request");

    let summaries =
        state
            .storage
            .get_pipe_usage_summary()
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Failed to get metrics: {}", e),
            })?;

    // Format the summaries into a more readable response
    let result = serde_json::json!({
        "total_pipes": summaries.len(),
        "pipes": summaries.iter().map(|s| serde_json::json!({
            "pipe_name": s.pipe_name,
            "total_calls": s.total_calls,
            "success_count": s.success_count,
            "failure_count": s.failure_count,
            "success_rate": format!("{:.1}%", s.success_rate * 100.0),
            "avg_latency_ms": format!("{:.0}", s.avg_latency_ms),
            "min_latency_ms": s.min_latency_ms,
            "max_latency_ms": s.max_latency_ms,
            "first_call": s.first_call.to_rfc3339(),
            "last_call": s.last_call.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "summary": if summaries.is_empty() {
            "No pipe invocations recorded yet.".to_string()
        } else {
            let total_calls: u64 = summaries.iter().map(|s| s.total_calls).sum();
            let total_success: u64 = summaries.iter().map(|s| s.success_count).sum();
            format!(
                "{} pipes, {} total calls, {:.1}% overall success rate",
                summaries.len(),
                total_calls,
                if total_calls > 0 { (total_success as f64 / total_calls as f64) * 100.0 } else { 0.0 }
            )
        }
    });

    Ok(result)
}

/// Handle reasoning_metrics_by_pipe tool call
async fn handle_metrics_by_pipe(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    use crate::storage::Storage;

    let params: MetricsByPipeParams = parse_arguments("reasoning_metrics_by_pipe", arguments)?;
    info!(pipe = %params.pipe_name, "Handling metrics by pipe request");

    let summary = state
        .storage
        .get_pipe_summary(&params.pipe_name)
        .await
        .map_err(|e| McpError::ExecutionFailed {
            message: format!("Failed to get pipe metrics: {}", e),
        })?;

    match summary {
        Some(s) => Ok(serde_json::json!({
            "found": true,
            "pipe_name": s.pipe_name,
            "total_calls": s.total_calls,
            "success_count": s.success_count,
            "failure_count": s.failure_count,
            "success_rate": format!("{:.1}%", s.success_rate * 100.0),
            "avg_latency_ms": format!("{:.0}", s.avg_latency_ms),
            "min_latency_ms": s.min_latency_ms,
            "max_latency_ms": s.max_latency_ms,
            "first_call": s.first_call.to_rfc3339(),
            "last_call": s.last_call.to_rfc3339(),
        })),
        None => Ok(serde_json::json!({
            "found": false,
            "pipe_name": params.pipe_name,
            "message": format!("No invocations found for pipe '{}'", params.pipe_name)
        })),
    }
}

/// Handle reasoning_metrics_invocations tool call
async fn handle_metrics_invocations(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
    use crate::storage::{MetricsFilter, Storage};

    let params: MetricsInvocationsParams = parse_arguments_or_default(arguments)?;
    info!("Handling metrics invocations request");

    // Build filter from params
    let mut filter = MetricsFilter::new();

    if let Some(pipe_name) = params.pipe_name {
        filter = filter.with_pipe(pipe_name);
    }
    if let Some(tool_name) = params.tool_name {
        filter = filter.with_tool(tool_name);
    }
    if let Some(session_id) = params.session_id {
        filter = filter.with_session(session_id);
    }
    if let Some(success_only) = params.success_only {
        if success_only {
            filter = filter.successful_only();
        } else {
            filter = filter.failed_only();
        }
    }
    filter = filter.with_limit(params.limit.unwrap_or(100).min(1000));

    let invocations =
        state
            .storage
            .get_invocations(filter)
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Failed to get invocations: {}", e),
            })?;

    let result = serde_json::json!({
        "count": invocations.len(),
        "invocations": invocations.iter().map(|inv| serde_json::json!({
            "id": inv.id,
            "tool_name": inv.tool_name,
            "pipe_name": inv.pipe_name,
            "session_id": inv.session_id,
            "success": inv.success,
            "error": inv.error,
            "latency_ms": inv.latency_ms,
            "created_at": inv.created_at.to_rfc3339(),
        })).collect::<Vec<_>>()
    });

    Ok(result)
}

/// Handle reasoning_debug_config tool call - returns current pipe configuration
async fn handle_debug_config(state: &SharedState) -> McpResult<Value> {
    info!("Handling debug config request");

    let config = &state.config;
    let pipes = &config.pipes;

    // Extract pipe names from optional configs
    let detection_pipe = pipes
        .detection
        .as_ref()
        .and_then(|d| d.pipe.clone())
        .unwrap_or_else(|| "<fallback: detection-v1>".to_string());

    let decision_pipe = pipes
        .decision
        .as_ref()
        .and_then(|d| d.pipe.clone())
        .unwrap_or_else(|| "<fallback: decision-framework-v1>".to_string());

    let got_pipe = pipes
        .got
        .as_ref()
        .and_then(|g| g.pipe.clone())
        .unwrap_or_else(|| "<fallback: got-reasoning-v1>".to_string());

    let evidence_pipe = pipes
        .evidence
        .as_ref()
        .and_then(|e| e.pipe.clone())
        .unwrap_or_else(|| "<fallback: decision-framework-v1>".to_string());

    Ok(serde_json::json!({
        "debug_info": "Current pipe configuration",
        "pipes": {
            "linear": pipes.linear,
            "tree": pipes.tree,
            "divergent": pipes.divergent,
            "reflection": pipes.reflection,
            "auto_router": pipes.auto_router,
            "got": got_pipe,
            "detection": detection_pipe,
            "decision": decision_pipe,
            "evidence": evidence_pipe,
        },
        "detection_config_present": pipes.detection.is_some(),
        "decision_config_present": pipes.decision.is_some(),
        "got_config_present": pipes.got.is_some(),
        "evidence_config_present": pipes.evidence.is_some(),
    }))
}

/// Handle reasoning_fallback_metrics tool call - returns fallback usage statistics
async fn handle_fallback_metrics(state: &SharedState) -> McpResult<Value> {
    use crate::storage::Storage;

    info!("Handling fallback metrics request");

    let metrics =
        state
            .storage
            .get_fallback_metrics()
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Failed to get fallback metrics: {}", e),
            })?;

    Ok(serde_json::json!({
        "total_fallbacks": metrics.total_fallbacks,
        "fallbacks_by_type": metrics.fallbacks_by_type,
        "fallbacks_by_pipe": metrics.fallbacks_by_pipe,
        "total_invocations": metrics.total_invocations,
        "fallback_rate": metrics.fallback_rate,
        "recommendation": metrics.recommendation
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct TestParams {
        #[serde(default)]
        content: String,
        #[serde(default)]
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
        // TestParams now has default fields, so missing fields are OK.
        // Test with wrong type instead to trigger actual error
        let args = Some(json!({
            "content": 123,  // wrong type: expected string, got number
            "value": "not a number"  // wrong type: expected i32
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

    // ============================================================================
    // Phase 4 - Detection parameter parsing tests
    // ============================================================================

    #[test]
    fn test_parse_detect_biases_params_with_content() {
        let args = Some(json!({
            "content": "I think this is obviously the best solution because everyone agrees."
        }));

        let result: McpResult<DetectBiasesParams> =
            parse_arguments("reasoning_detect_biases", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(
            params.content,
            Some(
                "I think this is obviously the best solution because everyone agrees.".to_string()
            )
        );
        assert!(params.thought_id.is_none());
        assert!(params.session_id.is_none());
        assert!(params.check_types.is_none());
    }

    #[test]
    fn test_parse_detect_biases_params_with_thought_id() {
        let args = Some(json!({
            "thought_id": "thought-123",
            "session_id": "sess-456"
        }));

        let result: McpResult<DetectBiasesParams> =
            parse_arguments("reasoning_detect_biases", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert!(params.content.is_none());
        assert_eq!(params.thought_id, Some("thought-123".to_string()));
        assert_eq!(params.session_id, Some("sess-456".to_string()));
    }

    #[test]
    fn test_parse_detect_biases_params_with_check_types() {
        let args = Some(json!({
            "content": "Some content to analyze",
            "check_types": ["confirmation_bias", "anchoring_bias"]
        }));

        let result: McpResult<DetectBiasesParams> =
            parse_arguments("reasoning_detect_biases", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert!(params.check_types.is_some());
        let check_types = params.check_types.unwrap();
        assert_eq!(check_types.len(), 2);
        assert!(check_types.contains(&"confirmation_bias".to_string()));
        assert!(check_types.contains(&"anchoring_bias".to_string()));
    }

    #[test]
    fn test_parse_detect_fallacies_params_with_content() {
        let args = Some(json!({
            "content": "You can't trust his argument because he's not a scientist."
        }));

        let result: McpResult<DetectFallaciesParams> =
            parse_arguments("reasoning_detect_fallacies", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(
            params.content,
            Some("You can't trust his argument because he's not a scientist.".to_string())
        );
        assert!(params.thought_id.is_none());
        // Defaults should be true
        assert!(params.check_formal);
        assert!(params.check_informal);
    }

    #[test]
    fn test_parse_detect_fallacies_params_with_thought_id() {
        let args = Some(json!({
            "thought_id": "thought-789"
        }));

        let result: McpResult<DetectFallaciesParams> =
            parse_arguments("reasoning_detect_fallacies", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert!(params.content.is_none());
        assert_eq!(params.thought_id, Some("thought-789".to_string()));
    }

    #[test]
    fn test_parse_detect_fallacies_params_with_custom_checks() {
        let args = Some(json!({
            "content": "Some argument",
            "check_formal": false,
            "check_informal": true
        }));

        let result: McpResult<DetectFallaciesParams> =
            parse_arguments("reasoning_detect_fallacies", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert!(!params.check_formal);
        assert!(params.check_informal);
    }

    #[test]
    fn test_parse_detect_fallacies_params_defaults() {
        let args = Some(json!({
            "content": "Test content"
        }));

        let result: McpResult<DetectFallaciesParams> =
            parse_arguments("reasoning_detect_fallacies", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        // Default values should be true
        assert!(params.check_formal);
        assert!(params.check_informal);
    }

    // ============================================================================
    // Auxiliary parameter parsing tests
    // ============================================================================

    #[test]
    fn test_parse_tree_focus_params() {
        let args = Some(json!({
            "session_id": "sess-123",
            "branch_id": "branch-456"
        }));

        let result: McpResult<TreeFocusParams> = parse_arguments("reasoning.tree.focus", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.session_id, "sess-123");
        assert_eq!(params.branch_id, "branch-456");
    }

    #[test]
    fn test_parse_tree_focus_params_missing_field() {
        let args = Some(json!({
            "session_id": "sess-123"
            // missing branch_id
        }));

        let result: McpResult<TreeFocusParams> = parse_arguments("reasoning.tree.focus", args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tree_list_params() {
        let args = Some(json!({
            "session_id": "sess-789"
        }));

        let result: McpResult<TreeListParams> = parse_arguments("reasoning.tree.list", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.session_id, "sess-789");
    }

    #[test]
    fn test_parse_tree_complete_params_default_completed() {
        let args = Some(json!({
            "branch_id": "branch-123"
        }));

        let result: McpResult<TreeCompleteParams> =
            parse_arguments("reasoning.tree.complete", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.branch_id, "branch-123");
        assert!(params.completed); // Should default to true
    }

    #[test]
    fn test_parse_tree_complete_params_completed_true() {
        let args = Some(json!({
            "branch_id": "branch-456",
            "completed": true
        }));

        let result: McpResult<TreeCompleteParams> =
            parse_arguments("reasoning.tree.complete", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.branch_id, "branch-456");
        assert!(params.completed);
    }

    #[test]
    fn test_parse_tree_complete_params_completed_false() {
        let args = Some(json!({
            "branch_id": "branch-789",
            "completed": false
        }));

        let result: McpResult<TreeCompleteParams> =
            parse_arguments("reasoning.tree.complete", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.branch_id, "branch-789");
        assert!(!params.completed);
    }

    #[test]
    fn test_parse_reflection_evaluate_params() {
        let args = Some(json!({
            "session_id": "sess-reflection-123"
        }));

        let result: McpResult<ReflectionEvaluateParams> =
            parse_arguments("reasoning.reflection.evaluate", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.session_id, "sess-reflection-123");
    }

    #[test]
    fn test_parse_checkpoint_create_params_with_description() {
        let args = Some(json!({
            "session_id": "sess-checkpoint-1",
            "name": "before-refactor",
            "description": "Checkpoint before major refactoring"
        }));

        let result: McpResult<CheckpointCreateParams> =
            parse_arguments("reasoning.checkpoint.create", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.session_id, "sess-checkpoint-1");
        assert_eq!(params.name, "before-refactor");
        assert_eq!(
            params.description,
            Some("Checkpoint before major refactoring".to_string())
        );
    }

    #[test]
    fn test_parse_checkpoint_create_params_without_description() {
        let args = Some(json!({
            "session_id": "sess-checkpoint-2",
            "name": "milestone-v1"
        }));

        let result: McpResult<CheckpointCreateParams> =
            parse_arguments("reasoning.checkpoint.create", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.session_id, "sess-checkpoint-2");
        assert_eq!(params.name, "milestone-v1");
        assert!(params.description.is_none());
    }

    #[test]
    fn test_parse_checkpoint_list_params() {
        let args = Some(json!({
            "session_id": "sess-list-checkpoints"
        }));

        let result: McpResult<CheckpointListParams> =
            parse_arguments("reasoning.checkpoint.list", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.session_id, "sess-list-checkpoints");
    }

    #[test]
    fn test_parse_preset_list_params_with_category() {
        let args = Some(json!({
            "category": "analysis"
        }));

        let result: McpResult<PresetListParams> = parse_arguments("reasoning.preset.list", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.category, Some("analysis".to_string()));
    }

    #[test]
    fn test_parse_preset_list_params_without_category() {
        let args = Some(json!({}));

        let result: McpResult<PresetListParams> = parse_arguments("reasoning.preset.list", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert!(params.category.is_none());
    }

    #[test]
    fn test_parse_preset_run_params_with_all_fields() {
        let args = Some(json!({
            "preset_id": "critical-analysis-v1",
            "inputs": {
                "content": "Analyze this text",
                "depth": 3
            },
            "session_id": "sess-preset-123"
        }));

        let result: McpResult<PresetRunParams> = parse_arguments("reasoning.preset.run", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.preset_id, "critical-analysis-v1");
        assert_eq!(params.inputs.len(), 2);
        assert_eq!(
            params.inputs.get("content"),
            Some(&json!("Analyze this text"))
        );
        assert_eq!(params.inputs.get("depth"), Some(&json!(3)));
        assert_eq!(params.session_id, Some("sess-preset-123".to_string()));
    }

    #[test]
    fn test_parse_preset_run_params_without_session_id() {
        let args = Some(json!({
            "preset_id": "quick-check",
            "inputs": {
                "text": "Sample input"
            }
        }));

        let result: McpResult<PresetRunParams> = parse_arguments("reasoning.preset.run", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.preset_id, "quick-check");
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_parse_preset_run_params_empty_inputs() {
        let args = Some(json!({
            "preset_id": "no-input-preset"
        }));

        let result: McpResult<PresetRunParams> = parse_arguments("reasoning.preset.run", args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.preset_id, "no-input-preset");
        assert!(params.inputs.is_empty());
    }

    // ============================================================================
    // Helper function tests
    // ============================================================================

    #[test]
    fn test_default_completed_function() {
        assert!(default_completed());
    }

    #[test]
    fn test_parse_arguments_or_default_with_arguments() {
        let args = Some(json!({
            "content": "test",
            "value": 99
        }));

        let result: McpResult<TestParams> = parse_arguments_or_default(args);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.content, "test");
        assert_eq!(params.value, 99);
    }

    #[test]
    fn test_parse_arguments_or_default_without_arguments() {
        #[derive(Debug, Default, Deserialize, PartialEq)]
        struct DefaultParams {
            #[serde(default)]
            name: String,
            #[serde(default)]
            count: i32,
        }

        let result: McpResult<DefaultParams> = parse_arguments_or_default(None);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.name, "");
        assert_eq!(params.count, 0);
    }

    #[test]
    fn test_parse_arguments_or_default_preset_list_params() {
        // Test the actual use case from handle_preset_list
        let result: McpResult<PresetListParams> = parse_arguments_or_default(None);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert!(params.category.is_none());
    }

    #[test]
    fn test_parse_arguments_or_default_invalid_json() {
        // TestParams now has default fields, so missing fields are OK.
        // Test with wrong type instead to trigger actual error
        let args = Some(json!({
            "content": 999,  // wrong type: expected string
            "value": "invalid"  // wrong type: expected i32
        }));

        let result: McpResult<TestParams> = parse_arguments_or_default(args);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, McpError::InvalidParameters { .. }));
    }
}
