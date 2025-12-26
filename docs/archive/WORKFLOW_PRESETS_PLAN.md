# Implementation Plan: Workflow Presets

## Overview

Add workflow preset system to compose existing reasoning modes into higher-level workflows.

**Priority**: 0.80
**Tools to add**: `reasoning_preset_list`, `reasoning_preset_run`
**Estimated effort**: Medium (2-3 days)

---

## 1. Data Structures

### `src/presets/types.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A reusable workflow preset that composes reasoning modes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPreset {
    /// Unique preset identifier (e.g., "code-review").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what the preset does.
    pub description: String,
    /// Category for grouping (code, architecture, research, etc.).
    pub category: String,
    /// Ordered steps in the workflow.
    pub steps: Vec<PresetStep>,
    /// Input schema describing required/optional parameters.
    pub input_schema: HashMap<String, ParamSpec>,
    /// Expected output format description.
    pub output_format: String,
    /// Estimated execution time.
    pub estimated_time: String,
    /// Tags for searchability.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A single step in a preset workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetStep {
    /// Unique step identifier within the preset.
    pub step_id: String,
    /// Tool to invoke (e.g., "reasoning_linear", "reasoning_tree").
    pub tool: String,
    /// Description of what this step does.
    pub description: String,
    /// Maps preset inputs to tool parameters.
    pub input_map: HashMap<String, String>,
    /// Fixed parameters for this step.
    #[serde(default)]
    pub static_inputs: HashMap<String, serde_json::Value>,
    /// Conditional execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<StepCondition>,
    /// Save result with this key for later steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_as: Option<String>,
    /// Steps that must complete first.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// If true, failures don't stop the workflow.
    #[serde(default)]
    pub optional: bool,
}

/// Conditional execution logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCondition {
    /// Condition type (confidence_threshold, result_match).
    pub condition_type: String,
    /// Field to check from previous step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Comparison operator (gt, lt, eq, contains).
    pub operator: String,
    /// Value to compare against.
    pub value: serde_json::Value,
    /// Step to get the field from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_step: Option<String>,
}

/// Parameter specification for preset inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Parameter type (string, number, boolean, array, object).
    pub param_type: String,
    /// Whether the parameter is required.
    pub required: bool,
    /// Default value if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Description of the parameter.
    pub description: String,
    /// Example values.
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
}

/// Result of executing a preset workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetResult {
    /// Executed preset ID.
    pub preset_id: String,
    /// Number of steps completed.
    pub steps_completed: usize,
    /// Total number of steps.
    pub steps_total: usize,
    /// Individual step results.
    pub step_results: Vec<StepResult>,
    /// Aggregated final output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_output: Option<serde_json::Value>,
    /// Execution status (success, partial, failed).
    pub status: String,
    /// Total execution time in milliseconds.
    pub duration_ms: i64,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Result of a single step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step number (1-based).
    pub step: usize,
    /// Step identifier.
    pub step_id: String,
    /// Tool that was executed.
    pub tool: String,
    /// Tool's response.
    pub result: serde_json::Value,
    /// Execution time in milliseconds.
    pub duration_ms: i64,
    /// Status (success, failed, skipped).
    pub status: String,
    /// Error details if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Brief preset summary for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub step_count: usize,
    pub estimated_time: String,
}

impl WorkflowPreset {
    /// Create a summary from the full preset.
    pub fn to_summary(&self) -> PresetSummary {
        PresetSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            category: self.category.clone(),
            step_count: self.steps.len(),
            estimated_time: self.estimated_time.clone(),
        }
    }
}
```

---

## 2. Preset Registry

### `src/presets/registry.rs`

```rust
use std::collections::HashMap;
use std::sync::RwLock;
use crate::error::AppResult;
use super::types::{WorkflowPreset, PresetSummary};
use super::builtins;

/// Registry for workflow presets.
pub struct PresetRegistry {
    presets: RwLock<HashMap<String, WorkflowPreset>>,
}

impl PresetRegistry {
    /// Create a new registry with built-in presets.
    pub fn new() -> Self {
        let registry = Self {
            presets: RwLock::new(HashMap::new()),
        };
        registry.register_builtins();
        registry
    }

    /// Register a preset.
    pub fn register(&self, preset: WorkflowPreset) -> AppResult<()> {
        let mut presets = self.presets.write().unwrap();
        if presets.contains_key(&preset.id) {
            return Err(format!("Preset {} already exists", preset.id).into());
        }
        presets.insert(preset.id.clone(), preset);
        Ok(())
    }

    /// Get a preset by ID.
    pub fn get(&self, id: &str) -> Option<WorkflowPreset> {
        self.presets.read().unwrap().get(id).cloned()
    }

    /// List all presets, optionally filtered by category.
    pub fn list(&self, category: Option<&str>) -> Vec<PresetSummary> {
        self.presets
            .read()
            .unwrap()
            .values()
            .filter(|p| category.map_or(true, |c| p.category == c))
            .map(|p| p.to_summary())
            .collect()
    }

    /// Get all unique categories.
    pub fn categories(&self) -> Vec<String> {
        let presets = self.presets.read().unwrap();
        let mut cats: Vec<_> = presets.values().map(|p| p.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    fn register_builtins(&self) {
        let _ = self.register(builtins::code_review_preset());
        let _ = self.register(builtins::debug_analysis_preset());
        let _ = self.register(builtins::architecture_decision_preset());
    }
}

impl Default for PresetRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## 3. Built-in Presets

### `src/presets/builtins.rs`

```rust
use super::types::{WorkflowPreset, PresetStep, ParamSpec, StepCondition};
use std::collections::HashMap;
use serde_json::json;

/// Code review workflow using multiple reasoning modes.
pub fn code_review_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "code-review".to_string(),
        name: "Code Review".to_string(),
        description: "Analyze code quality using divergent thinking, \
            bias detection, and reflection".to_string(),
        category: "code".to_string(),
        estimated_time: "2-3 minutes".to_string(),
        output_format: "structured_review".to_string(),
        tags: vec!["code".to_string(), "review".to_string(), "quality".to_string()],
        input_schema: HashMap::from([
            ("code".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: true,
                default: None,
                description: "The code to review".to_string(),
                examples: vec![json!("function example() { ... }")],
            }),
            ("focus".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: false,
                default: Some(json!("quality")),
                description: "Review focus (quality, performance, security)".to_string(),
                examples: vec![json!("performance"), json!("security")],
            }),
        ]),
        steps: vec![
            PresetStep {
                step_id: "divergent_analysis".to_string(),
                tool: "reasoning_divergent".to_string(),
                description: "Generate multiple perspectives on the code".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "code".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("num_perspectives".to_string(), json!(3)),
                    ("challenge_assumptions".to_string(), json!(true)),
                ]),
                condition: None,
                store_as: Some("perspectives".to_string()),
                depends_on: vec![],
                optional: false,
            },
            PresetStep {
                step_id: "bias_check".to_string(),
                tool: "reasoning_detect_biases".to_string(),
                description: "Check for cognitive biases in reasoning".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "code".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("biases".to_string()),
                depends_on: vec!["divergent_analysis".to_string()],
                optional: true,
            },
            PresetStep {
                step_id: "fallacy_check".to_string(),
                tool: "reasoning_detect_fallacies".to_string(),
                description: "Check for logical fallacies".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "code".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("fallacies".to_string()),
                depends_on: vec!["divergent_analysis".to_string()],
                optional: true,
            },
            PresetStep {
                step_id: "reflect".to_string(),
                tool: "reasoning_reflection".to_string(),
                description: "Synthesize findings into final assessment".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "code".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("quality_threshold".to_string(), json!(0.7)),
                ]),
                condition: None,
                store_as: Some("reflection".to_string()),
                depends_on: vec![
                    "divergent_analysis".to_string(),
                    "bias_check".to_string(),
                    "fallacy_check".to_string(),
                ],
                optional: false,
            },
        ],
    }
}

/// Debug analysis workflow with hypothesis generation.
pub fn debug_analysis_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "debug-analysis".to_string(),
        name: "Debug Analysis".to_string(),
        description: "Systematic debugging with tree exploration \
            and hypothesis generation".to_string(),
        category: "code".to_string(),
        estimated_time: "3-5 minutes".to_string(),
        output_format: "debug_report".to_string(),
        tags: vec!["debug".to_string(), "analysis".to_string()],
        input_schema: HashMap::from([
            ("problem".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: true,
                default: None,
                description: "Description of the bug or issue".to_string(),
                examples: vec![json!("Function returns null unexpectedly")],
            }),
            ("context".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: false,
                default: None,
                description: "Relevant context (recent changes, environment)".to_string(),
                examples: vec![],
            }),
        ]),
        steps: vec![
            PresetStep {
                step_id: "linear_analysis".to_string(),
                tool: "reasoning_linear".to_string(),
                description: "Initial problem analysis".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "problem".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("confidence".to_string(), json!(0.8)),
                ]),
                condition: None,
                store_as: Some("initial".to_string()),
                depends_on: vec![],
                optional: false,
            },
            PresetStep {
                step_id: "hypothesis_tree".to_string(),
                tool: "reasoning_tree".to_string(),
                description: "Generate hypothesis branches".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "problem".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("num_branches".to_string(), json!(3)),
                ]),
                condition: None,
                store_as: Some("hypotheses".to_string()),
                depends_on: vec!["linear_analysis".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "checkpoint".to_string(),
                tool: "reasoning_checkpoint_create".to_string(),
                description: "Save state for potential backtracking".to_string(),
                input_map: HashMap::new(),
                static_inputs: HashMap::from([
                    ("name".to_string(), json!("debug-checkpoint")),
                    ("description".to_string(), json!("After hypothesis generation")),
                ]),
                condition: None,
                store_as: Some("checkpoint".to_string()),
                depends_on: vec!["hypothesis_tree".to_string()],
                optional: true,
            },
            PresetStep {
                step_id: "reflect".to_string(),
                tool: "reasoning_reflection".to_string(),
                description: "Evaluate hypotheses and recommend solution".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "problem".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("quality_threshold".to_string(), json!(0.75)),
                ]),
                condition: None,
                store_as: Some("conclusion".to_string()),
                depends_on: vec!["hypothesis_tree".to_string()],
                optional: false,
            },
        ],
    }
}

/// Architecture decision workflow with multi-criteria analysis.
pub fn architecture_decision_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "architecture-decision".to_string(),
        name: "Architecture Decision".to_string(),
        description: "Multi-criteria architectural analysis using \
            divergent thinking and Graph-of-Thoughts".to_string(),
        category: "architecture".to_string(),
        estimated_time: "4-6 minutes".to_string(),
        output_format: "adr".to_string(),
        tags: vec!["architecture".to_string(), "decision".to_string(), "adr".to_string()],
        input_schema: HashMap::from([
            ("question".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: true,
                default: None,
                description: "The architectural decision to make".to_string(),
                examples: vec![json!("Should we use microservices or monolith?")],
            }),
            ("constraints".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: false,
                default: None,
                description: "Known constraints or requirements".to_string(),
                examples: vec![json!("Must scale to 10k users, budget limited")],
            }),
        ]),
        steps: vec![
            PresetStep {
                step_id: "explore_options".to_string(),
                tool: "reasoning_divergent".to_string(),
                description: "Generate architectural options".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("num_perspectives".to_string(), json!(4)),
                    ("challenge_assumptions".to_string(), json!(true)),
                    ("force_rebellion".to_string(), json!(true)),
                ]),
                condition: None,
                store_as: Some("options".to_string()),
                depends_on: vec![],
                optional: false,
            },
            PresetStep {
                step_id: "got_init".to_string(),
                tool: "reasoning_got_init".to_string(),
                description: "Initialize decision graph".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("graph".to_string()),
                depends_on: vec!["explore_options".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "got_generate".to_string(),
                tool: "reasoning_got_generate".to_string(),
                description: "Expand decision tree with criteria".to_string(),
                input_map: HashMap::from([
                    ("problem".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("k".to_string(), json!(3)),
                ]),
                condition: None,
                store_as: Some("expansions".to_string()),
                depends_on: vec!["got_init".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "got_score".to_string(),
                tool: "reasoning_got_score".to_string(),
                description: "Score each option against criteria".to_string(),
                input_map: HashMap::from([
                    ("problem".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("scores".to_string()),
                depends_on: vec!["got_generate".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "got_finalize".to_string(),
                tool: "reasoning_got_finalize".to_string(),
                description: "Finalize decision with recommendation".to_string(),
                input_map: HashMap::new(),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("decision".to_string()),
                depends_on: vec!["got_score".to_string()],
                optional: false,
            },
        ],
    }
}
```

---

## 4. Preset Executor

### `src/presets/executor.rs`

```rust
use std::collections::HashMap;
use std::time::Instant;
use tracing::{info, warn};

use super::types::{WorkflowPreset, PresetResult, StepResult};
use crate::server::{handle_tool_call, SharedState};
use crate::error::McpResult;

/// Execute a workflow preset.
pub async fn execute_preset(
    state: &SharedState,
    preset: &WorkflowPreset,
    inputs: HashMap<String, serde_json::Value>,
) -> McpResult<PresetResult> {
    let start = Instant::now();
    let mut step_results = Vec::new();
    let mut context: HashMap<String, serde_json::Value> = inputs.clone();
    let mut completed_steps: std::collections::HashSet<String> = std::collections::HashSet::new();

    info!(preset_id = %preset.id, "Starting preset execution");

    for (idx, step) in preset.steps.iter().enumerate() {
        let step_start = Instant::now();

        // Check dependencies
        let deps_met = step.depends_on.iter().all(|d| completed_steps.contains(d));
        if !deps_met {
            warn!(step_id = %step.step_id, "Dependencies not met, skipping");
            step_results.push(StepResult {
                step: idx + 1,
                step_id: step.step_id.clone(),
                tool: step.tool.clone(),
                result: serde_json::json!(null),
                duration_ms: 0,
                status: "skipped".to_string(),
                error: Some("Dependencies not met".to_string()),
            });
            continue;
        }

        // Check condition
        if let Some(condition) = &step.condition {
            if !evaluate_condition(condition, &context) {
                step_results.push(StepResult {
                    step: idx + 1,
                    step_id: step.step_id.clone(),
                    tool: step.tool.clone(),
                    result: serde_json::json!(null),
                    duration_ms: 0,
                    status: "skipped".to_string(),
                    error: Some("Condition not met".to_string()),
                });
                continue;
            }
        }

        // Build tool arguments
        let arguments = build_step_arguments(step, &context);

        // Execute tool
        match handle_tool_call(state, &step.tool, Some(arguments.clone())).await {
            Ok(result) => {
                let duration = step_start.elapsed().as_millis() as i64;

                // Store result if configured
                if let Some(store_key) = &step.store_as {
                    context.insert(store_key.clone(), result.clone());
                }

                completed_steps.insert(step.step_id.clone());

                step_results.push(StepResult {
                    step: idx + 1,
                    step_id: step.step_id.clone(),
                    tool: step.tool.clone(),
                    result,
                    duration_ms: duration,
                    status: "success".to_string(),
                    error: None,
                });
            }
            Err(e) => {
                let duration = step_start.elapsed().as_millis() as i64;

                if step.optional {
                    warn!(step_id = %step.step_id, error = %e, "Optional step failed");
                    completed_steps.insert(step.step_id.clone());
                    step_results.push(StepResult {
                        step: idx + 1,
                        step_id: step.step_id.clone(),
                        tool: step.tool.clone(),
                        result: serde_json::json!(null),
                        duration_ms: duration,
                        status: "failed".to_string(),
                        error: Some(e.to_string()),
                    });
                } else {
                    // Non-optional step failed - stop execution
                    step_results.push(StepResult {
                        step: idx + 1,
                        step_id: step.step_id.clone(),
                        tool: step.tool.clone(),
                        result: serde_json::json!(null),
                        duration_ms: duration,
                        status: "failed".to_string(),
                        error: Some(e.to_string()),
                    });

                    return Ok(PresetResult {
                        preset_id: preset.id.clone(),
                        steps_completed: step_results.iter().filter(|s| s.status == "success").count(),
                        steps_total: preset.steps.len(),
                        step_results,
                        final_output: None,
                        status: "failed".to_string(),
                        duration_ms: start.elapsed().as_millis() as i64,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
    }

    let successful = step_results.iter().filter(|s| s.status == "success").count();
    let status = if successful == preset.steps.len() {
        "success"
    } else if successful > 0 {
        "partial"
    } else {
        "failed"
    };

    // Build final output from last successful step's result
    let final_output = step_results
        .iter()
        .rev()
        .find(|s| s.status == "success")
        .map(|s| s.result.clone());

    Ok(PresetResult {
        preset_id: preset.id.clone(),
        steps_completed: successful,
        steps_total: preset.steps.len(),
        step_results,
        final_output,
        status: status.to_string(),
        duration_ms: start.elapsed().as_millis() as i64,
        error: None,
    })
}

fn build_step_arguments(
    step: &super::types::PresetStep,
    context: &HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    let mut args = serde_json::Map::new();

    // Add mapped inputs
    for (param, source) in &step.input_map {
        if let Some(value) = context.get(source) {
            args.insert(param.clone(), value.clone());
        }
    }

    // Add static inputs
    for (key, value) in &step.static_inputs {
        args.insert(key.clone(), value.clone());
    }

    serde_json::Value::Object(args)
}

fn evaluate_condition(
    condition: &super::types::StepCondition,
    context: &HashMap<String, serde_json::Value>,
) -> bool {
    let source_value = condition
        .source_step
        .as_ref()
        .and_then(|s| context.get(s))
        .and_then(|v| {
            condition.field.as_ref().and_then(|f| {
                v.get(f)
            })
        });

    match (source_value, &condition.operator.as_str()) {
        (Some(val), &"gt") => {
            val.as_f64().unwrap_or(0.0) > condition.value.as_f64().unwrap_or(0.0)
        }
        (Some(val), &"lt") => {
            val.as_f64().unwrap_or(0.0) < condition.value.as_f64().unwrap_or(0.0)
        }
        (Some(val), &"eq") => val == &condition.value,
        (Some(val), &"contains") => {
            val.as_str()
                .map(|s| s.contains(condition.value.as_str().unwrap_or("")))
                .unwrap_or(false)
        }
        _ => true, // Default to true if condition can't be evaluated
    }
}
```

---

## 5. MCP Tool Definitions

### Add to `src/server/mcp.rs`

```rust
Tool {
    name: "reasoning_preset_list".to_string(),
    description: Some(
        "List available workflow presets. Presets are predefined \
        multi-step reasoning workflows for common tasks like code review, \
        debugging, and architecture decisions.".to_string()
    ),
    input_schema: json!({
        "type": "object",
        "properties": {
            "category": {
                "type": "string",
                "description": "Filter by category (code, architecture, research)"
            }
        }
    }),
},

Tool {
    name: "reasoning_preset_run".to_string(),
    description: Some(
        "Execute a workflow preset. Runs a predefined multi-step \
        reasoning workflow with the provided inputs.".to_string()
    ),
    input_schema: json!({
        "type": "object",
        "properties": {
            "preset_id": {
                "type": "string",
                "description": "ID of the preset to run (e.g., 'code-review')"
            },
            "inputs": {
                "type": "object",
                "description": "Input values matching the preset's input schema"
            },
            "dry_run": {
                "type": "boolean",
                "description": "Preview steps without executing (default: false)"
            }
        },
        "required": ["preset_id", "inputs"]
    }),
},
```

---

## 6. Handler Implementation

### Add to `src/server/handlers.rs`

```rust
/// Handle reasoning_preset_list tool
async fn handle_preset_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(Deserialize)]
    struct ListParams {
        category: Option<String>,
    }

    let params: ListParams = match arguments {
        Some(args) => serde_json::from_value(args).unwrap_or(ListParams { category: None }),
        None => ListParams { category: None },
    };

    let presets = state.preset_registry.list(params.category.as_deref());
    let categories = state.preset_registry.categories();

    Ok(json!({
        "presets": presets,
        "categories": categories,
        "count": presets.len()
    }))
}

/// Handle reasoning_preset_run tool
async fn handle_preset_run(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    #[derive(Deserialize)]
    struct RunParams {
        preset_id: String,
        inputs: HashMap<String, serde_json::Value>,
        #[serde(default)]
        dry_run: bool,
    }

    let params: RunParams = parse_arguments("reasoning_preset_run", arguments)?;

    let preset = state
        .preset_registry
        .get(&params.preset_id)
        .ok_or_else(|| McpError::InvalidParameters {
            tool_name: "reasoning_preset_run".to_string(),
            message: format!("Preset not found: {}", params.preset_id),
        })?;

    // Validate required inputs
    for (name, spec) in &preset.input_schema {
        if spec.required && !params.inputs.contains_key(name) {
            return Err(McpError::InvalidParameters {
                tool_name: "reasoning_preset_run".to_string(),
                message: format!("Missing required input: {}", name),
            });
        }
    }

    if params.dry_run {
        // Return preview of steps
        return Ok(json!({
            "preset_id": preset.id,
            "dry_run": true,
            "steps": preset.steps.iter().map(|s| json!({
                "step_id": s.step_id,
                "tool": s.tool,
                "description": s.description,
                "depends_on": s.depends_on,
                "optional": s.optional,
            })).collect::<Vec<_>>(),
            "input_schema": preset.input_schema,
        }));
    }

    let result = execute_preset(state, &preset, params.inputs).await?;
    serde_json::to_value(result).map_err(McpError::Json)
}
```

---

## 7. Module Structure

```
src/
├── presets/
│   ├── mod.rs           # Module exports
│   ├── types.rs         # Data structures
│   ├── registry.rs      # Preset registry
│   ├── executor.rs      # Workflow execution
│   └── builtins.rs      # Built-in presets
├── server/
│   ├── handlers.rs      # Add preset handlers
│   └── mcp.rs           # Add tool definitions
└── lib.rs               # Export presets module
```

---

## 8. Implementation Phases

### Phase 1: Core Types (0.5 days)
- [ ] Create `src/presets/mod.rs`
- [ ] Create `src/presets/types.rs`
- [ ] Add tests for types

### Phase 2: Registry (0.5 days)
- [ ] Create `src/presets/registry.rs`
- [ ] Add registry to SharedState
- [ ] Add tests for registry

### Phase 3: Built-in Presets (0.5 days)
- [ ] Create `src/presets/builtins.rs`
- [ ] Implement code-review preset
- [ ] Implement debug-analysis preset
- [ ] Implement architecture-decision preset

### Phase 4: Executor (0.5 days)
- [ ] Create `src/presets/executor.rs`
- [ ] Implement step execution logic
- [ ] Implement condition evaluation
- [ ] Implement result aggregation

### Phase 5: MCP Integration (0.5 days)
- [ ] Add tool definitions to mcp.rs
- [ ] Add handlers to handlers.rs
- [ ] Update handle_tool_call routing

### Phase 6: Testing & Documentation (0.5 days)
- [ ] Integration tests
- [ ] Update API_REFERENCE.md
- [ ] Update README.md

---

## 9. Success Criteria

- [ ] `reasoning_preset_list` returns all built-in presets
- [ ] `reasoning_preset_list` filters by category
- [ ] `reasoning_preset_run` executes code-review preset
- [ ] `reasoning_preset_run` executes debug-analysis preset
- [ ] `reasoning_preset_run` executes architecture-decision preset
- [ ] Step dependencies are respected
- [ ] Optional step failures don't stop workflow
- [ ] Dry-run mode returns step preview
- [ ] All tests pass

---

## 10. API Examples

### List Presets
```json
{
  "method": "tools/call",
  "params": {
    "name": "reasoning_preset_list",
    "arguments": {
      "category": "code"
    }
  }
}
```

### Run Code Review Preset
```json
{
  "method": "tools/call",
  "params": {
    "name": "reasoning_preset_run",
    "arguments": {
      "preset_id": "code-review",
      "inputs": {
        "code": "function add(a, b) { return a + b; }",
        "focus": "quality"
      }
    }
  }
}
```

### Dry Run
```json
{
  "method": "tools/call",
  "params": {
    "name": "reasoning_preset_run",
    "arguments": {
      "preset_id": "architecture-decision",
      "inputs": {
        "question": "Should we use microservices?"
      },
      "dry_run": true
    }
  }
}
```
