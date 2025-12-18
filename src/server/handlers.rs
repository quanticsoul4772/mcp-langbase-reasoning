use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;
use tracing::info;

use super::SharedState;
use crate::error::{McpError, McpResult};
use crate::langbase::{BiasDetectionResponse, FallacyDetectionResponse, Message, PipeRequest};
use crate::modes::{
    AutoParams, BacktrackingParams, DecisionParams, DivergentParams, EvidenceParams,
    GotAggregateParams, GotFinalizeParams, GotGenerateParams, GotGetStateParams, GotInitParams,
    GotPruneParams, GotRefineParams, GotScoreParams, LinearParams, PerspectiveParams,
    ProbabilisticParams, ReflectionParams, TreeParams,
};
use crate::presets::execute_preset;
use crate::prompts::{BIAS_DETECTION_PROMPT, FALLACY_DETECTION_PROMPT};
use crate::storage::{BranchState, Detection, DetectionType, Storage};

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
        _ => Err(McpError::UnknownTool {
            tool_name: tool_name.to_string(),
        }),
    }
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
    #[derive(serde::Deserialize)]
    struct CreateParams {
        session_id: String,
        name: String,
        description: Option<String>,
    }

    let params: CreateParams = parse_arguments("reasoning.checkpoint.create", arguments)?;

    let result = state
        .backtracking_mode
        .create_checkpoint(
            &params.session_id,
            &params.name,
            params.description.as_deref(),
        )
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

/// Parameters for bias detection
#[derive(Debug, Clone, Deserialize)]
pub struct DetectBiasesParams {
    /// Content to analyze for biases
    pub content: Option<String>,
    /// ID of an existing thought to analyze
    pub thought_id: Option<String>,
    /// Session ID for context and persistence
    pub session_id: Option<String>,
    /// Specific bias types to check (optional)
    pub check_types: Option<Vec<String>>,
}

/// Parameters for fallacy detection
#[derive(Debug, Clone, Deserialize)]
pub struct DetectFallaciesParams {
    /// Content to analyze for fallacies
    pub content: Option<String>,
    /// ID of an existing thought to analyze
    pub thought_id: Option<String>,
    /// Session ID for context and persistence
    pub session_id: Option<String>,
    /// Check for formal logical fallacies (default: true)
    #[serde(default = "default_true")]
    pub check_formal: bool,
    /// Check for informal logical fallacies (default: true)
    #[serde(default = "default_true")]
    pub check_informal: bool,
}

fn default_true() -> bool {
    true
}

/// Response for bias detection
#[derive(Debug, Clone, Serialize)]
pub struct DetectBiasesResponse {
    /// Detected biases
    pub detections: Vec<Detection>,
    /// Number of detections
    pub detection_count: usize,
    /// Length of analyzed content
    pub analyzed_content_length: usize,
    /// Overall assessment
    pub overall_assessment: Option<String>,
    /// Reasoning quality score (0.0-1.0)
    pub reasoning_quality: Option<f64>,
}

/// Response for fallacy detection
#[derive(Debug, Clone, Serialize)]
pub struct DetectFallaciesResponse {
    /// Detected fallacies
    pub detections: Vec<Detection>,
    /// Number of detections
    pub detection_count: usize,
    /// Length of analyzed content
    pub analyzed_content_length: usize,
    /// Overall assessment
    pub overall_assessment: Option<String>,
    /// Argument validity score (0.0-1.0)
    pub argument_validity: Option<f64>,
}

/// Handle reasoning_detect_biases tool call
async fn handle_detect_biases(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let start = Instant::now();
    let params: DetectBiasesParams = parse_arguments("reasoning_detect_biases", arguments)?;

    // Validate: either content or thought_id must be provided
    let (analysis_content, thought_id) = match (&params.content, &params.thought_id) {
        (Some(content), _) => (content.clone(), params.thought_id.clone()),
        (None, Some(thought_id)) => {
            // Get thought content from storage
            let thought = state
                .storage
                .get_thought(thought_id)
                .await
                .map_err(|e| McpError::ExecutionFailed {
                    message: format!("Failed to get thought: {}", e),
                })?
                .ok_or_else(|| McpError::InvalidParameters {
                    tool_name: "reasoning_detect_biases".to_string(),
                    message: format!("Thought not found: {}", thought_id),
                })?;
            (thought.content, Some(thought_id.clone()))
        }
        (None, None) => {
            return Err(McpError::InvalidParameters {
                tool_name: "reasoning_detect_biases".to_string(),
                message: "Either 'content' or 'thought_id' must be provided".to_string(),
            });
        }
    };

    // Get pipe name from config or use default
    let pipe_name = state
        .config
        .pipes
        .detection
        .as_ref()
        .and_then(|d| d.bias_pipe.clone())
        .unwrap_or_else(|| "detect-biases-v1".to_string());

    // Build messages for Langbase
    let mut messages = vec![Message::system(BIAS_DETECTION_PROMPT)];

    // Add specific bias types to check if provided
    if let Some(check_types) = &params.check_types {
        if !check_types.is_empty() {
            messages.push(Message::user(format!(
                "Focus specifically on detecting these bias types: {}\n\nContent to analyze:\n{}",
                check_types.join(", "),
                analysis_content
            )));
        } else {
            messages.push(Message::user(format!(
                "Analyze the following content for cognitive biases:\n\n{}",
                analysis_content
            )));
        }
    } else {
        messages.push(Message::user(format!(
            "Analyze the following content for cognitive biases:\n\n{}",
            analysis_content
        )));
    }

    // Call Langbase pipe
    let request = PipeRequest::new(&pipe_name, messages);
    let response =
        state
            .langbase
            .call_pipe(request)
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Langbase call failed: {}", e),
            })?;

    // Parse response
    let bias_response = BiasDetectionResponse::from_completion(&response.completion);

    // Convert to Detection structs and persist
    let mut detections = Vec::new();
    for detected in &bias_response.detections {
        let mut detection = Detection::new(
            DetectionType::Bias,
            &detected.bias_type,
            detected.severity,
            detected.confidence,
            &detected.explanation,
        );

        if let Some(session_id) = &params.session_id {
            detection = detection.with_session(session_id);
        }
        if let Some(tid) = &thought_id {
            detection = detection.with_thought(tid);
        }
        if let Some(remediation) = &detected.remediation {
            detection = detection.with_remediation(remediation);
        }
        if let Some(excerpt) = &detected.excerpt {
            detection = detection.with_metadata(serde_json::json!({ "excerpt": excerpt }));
        }

        // Persist to storage
        state
            .storage
            .create_detection(&detection)
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Failed to save detection: {}", e),
            })?;

        detections.push(detection);
    }

    let latency = start.elapsed().as_millis();
    info!(
        detection_count = detections.len(),
        latency_ms = latency,
        "Bias detection completed"
    );

    let response = DetectBiasesResponse {
        detections,
        detection_count: bias_response.detections.len(),
        analyzed_content_length: analysis_content.len(),
        overall_assessment: Some(bias_response.overall_assessment),
        reasoning_quality: Some(bias_response.reasoning_quality),
    };

    serde_json::to_value(response).map_err(McpError::Json)
}

/// Handle reasoning_detect_fallacies tool call
async fn handle_detect_fallacies(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
    let start = Instant::now();
    let params: DetectFallaciesParams = parse_arguments("reasoning_detect_fallacies", arguments)?;

    // Validate: either content or thought_id must be provided
    let (analysis_content, thought_id) = match (&params.content, &params.thought_id) {
        (Some(content), _) => (content.clone(), params.thought_id.clone()),
        (None, Some(thought_id)) => {
            // Get thought content from storage
            let thought = state
                .storage
                .get_thought(thought_id)
                .await
                .map_err(|e| McpError::ExecutionFailed {
                    message: format!("Failed to get thought: {}", e),
                })?
                .ok_or_else(|| McpError::InvalidParameters {
                    tool_name: "reasoning_detect_fallacies".to_string(),
                    message: format!("Thought not found: {}", thought_id),
                })?;
            (thought.content, Some(thought_id.clone()))
        }
        (None, None) => {
            return Err(McpError::InvalidParameters {
                tool_name: "reasoning_detect_fallacies".to_string(),
                message: "Either 'content' or 'thought_id' must be provided".to_string(),
            });
        }
    };

    // Log what types we're checking
    info!(
        check_formal = %params.check_formal,
        check_informal = %params.check_informal,
        "Detecting fallacies"
    );

    // Get pipe name from config or use default
    let pipe_name = state
        .config
        .pipes
        .detection
        .as_ref()
        .and_then(|d| d.fallacy_pipe.clone())
        .unwrap_or_else(|| "detect-fallacies-v1".to_string());

    // Build messages for Langbase
    let mut messages = vec![Message::system(FALLACY_DETECTION_PROMPT)];

    // Build instruction based on what types to check
    let check_instruction = match (params.check_formal, params.check_informal) {
        (true, true) => "Check for both formal and informal logical fallacies.".to_string(),
        (true, false) => "Focus only on formal logical fallacies (structural errors).".to_string(),
        (false, true) => {
            "Focus only on informal logical fallacies (content/context errors).".to_string()
        }
        (false, false) => {
            return Err(McpError::InvalidParameters {
                tool_name: "reasoning_detect_fallacies".to_string(),
                message: "At least one of check_formal or check_informal must be true".to_string(),
            });
        }
    };

    messages.push(Message::user(format!(
        "{}\n\nContent to analyze:\n{}",
        check_instruction, analysis_content
    )));

    // Call Langbase pipe
    let request = PipeRequest::new(&pipe_name, messages);
    let response =
        state
            .langbase
            .call_pipe(request)
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Langbase call failed: {}", e),
            })?;

    // Parse response
    let fallacy_response = FallacyDetectionResponse::from_completion(&response.completion);

    // Convert to Detection structs and persist
    let mut detections = Vec::new();
    for detected in &fallacy_response.detections {
        // Filter based on check_formal/check_informal params
        let is_formal = detected.category.to_lowercase() == "formal";
        if (is_formal && !params.check_formal) || (!is_formal && !params.check_informal) {
            continue;
        }

        let mut detection = Detection::new(
            DetectionType::Fallacy,
            &detected.fallacy_type,
            detected.severity,
            detected.confidence,
            &detected.explanation,
        );

        if let Some(session_id) = &params.session_id {
            detection = detection.with_session(session_id);
        }
        if let Some(tid) = &thought_id {
            detection = detection.with_thought(tid);
        }
        if let Some(remediation) = &detected.remediation {
            detection = detection.with_remediation(remediation);
        }

        // Store category and excerpt in metadata
        let mut meta = serde_json::Map::new();
        meta.insert("category".to_string(), serde_json::json!(detected.category));
        if let Some(excerpt) = &detected.excerpt {
            meta.insert("excerpt".to_string(), serde_json::json!(excerpt));
        }
        detection = detection.with_metadata(serde_json::Value::Object(meta));

        // Persist to storage
        state
            .storage
            .create_detection(&detection)
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: format!("Failed to save detection: {}", e),
            })?;

        detections.push(detection);
    }

    let latency = start.elapsed().as_millis();
    info!(
        detection_count = detections.len(),
        latency_ms = latency,
        "Fallacy detection completed"
    );

    let response = DetectFallaciesResponse {
        detections,
        detection_count: fallacy_response.detections.len(),
        analyzed_content_length: analysis_content.len(),
        overall_assessment: Some(fallacy_response.overall_assessment),
        argument_validity: Some(fallacy_response.argument_validity),
    };

    serde_json::to_value(response).map_err(McpError::Json)
}

// ============================================================================
// Phase 5 Handlers - Workflow Presets
// ============================================================================

/// Parameters for preset list
#[derive(Debug, Clone, Deserialize)]
pub struct PresetListParams {
    /// Optional category filter
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
    let params: PresetListParams = match arguments {
        Some(args) => {
            serde_json::from_value(args).map_err(|e| McpError::InvalidParameters {
                tool_name: "reasoning_preset_list".to_string(),
                message: e.to_string(),
            })?
        }
        None => PresetListParams { category: None },
    };

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
async fn handle_assess_evidence(
    state: &SharedState,
    arguments: Option<Value>,
) -> McpResult<Value> {
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
}
