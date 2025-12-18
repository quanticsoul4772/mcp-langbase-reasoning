//! Preset workflow execution engine.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{info, warn};

use super::types::{PresetResult, PresetStep, StepCondition, StepResult, WorkflowPreset};
use crate::error::McpResult;
use crate::server::handle_tool_call;

/// Shared state type alias for the executor.
pub type SharedState = Arc<crate::server::AppState>;

/// Errors that can occur during condition evaluation.
#[derive(Debug, Error)]
pub enum ConditionError {
    /// Source value could not be parsed as a number for comparison.
    #[error("Invalid source value for field '{field}': expected number, got {actual_type}")]
    NonNumericSource {
        /// The field name that had an invalid value.
        field: String,
        /// The actual type of the value.
        actual_type: String,
    },

    /// Threshold value in condition could not be parsed as a number.
    #[error(
        "Invalid threshold value for operator '{operator}': expected number, got {actual_type}"
    )]
    NonNumericThreshold {
        /// The operator being used.
        operator: String,
        /// The actual type of the threshold value.
        actual_type: String,
    },

    /// String comparison failed due to type mismatch.
    #[error("Invalid value for '{operator}' operator: expected string, got {actual_type}")]
    ExpectedString {
        /// The operator being used.
        operator: String,
        /// The actual type of the value.
        actual_type: String,
    },
}

/// Get a human-readable type name for a JSON value.
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Execute a workflow preset.
///
/// Runs each step in order, respecting dependencies and conditions.
/// Results from each step can be stored and referenced by later steps.
pub async fn execute_preset(
    state: &SharedState,
    preset: &WorkflowPreset,
    inputs: HashMap<String, serde_json::Value>,
) -> McpResult<PresetResult> {
    let start = Instant::now();
    let mut step_results = Vec::new();
    let mut context: HashMap<String, serde_json::Value> = inputs.clone();
    let mut completed_steps: HashSet<String> = HashSet::new();

    info!(preset_id = %preset.id, steps = preset.steps.len(), "Starting preset execution");

    for (idx, step) in preset.steps.iter().enumerate() {
        let step_start = Instant::now();

        // Check dependencies
        let deps_met = step.depends_on.iter().all(|d| completed_steps.contains(d));
        if !deps_met {
            let missing: Vec<_> = step
                .depends_on
                .iter()
                .filter(|d| !completed_steps.contains(*d))
                .collect();
            warn!(
                step_id = %step.step_id,
                missing_deps = ?missing,
                "Dependencies not met, skipping"
            );
            step_results.push(StepResult {
                step: idx + 1,
                step_id: step.step_id.clone(),
                tool: step.tool.clone(),
                result: serde_json::json!(null),
                duration_ms: 0,
                status: "skipped".to_string(),
                error: Some(format!("Dependencies not met: {:?}", missing)),
            });
            continue;
        }

        // Check condition
        if let Some(condition) = &step.condition {
            match evaluate_condition(condition, &context) {
                Ok(true) => {
                    // Condition met, continue to execute step
                }
                Ok(false) => {
                    info!(step_id = %step.step_id, "Condition not met, skipping");
                    step_results.push(StepResult {
                        step: idx + 1,
                        step_id: step.step_id.clone(),
                        tool: step.tool.clone(),
                        result: serde_json::json!(null),
                        duration_ms: 0,
                        status: "skipped".to_string(),
                        error: Some("Condition not met".to_string()),
                    });
                    // Still mark as completed so dependents can proceed
                    completed_steps.insert(step.step_id.clone());
                    continue;
                }
                Err(e) => {
                    warn!(
                        step_id = %step.step_id,
                        error = %e,
                        "Condition evaluation failed"
                    );
                    step_results.push(StepResult {
                        step: idx + 1,
                        step_id: step.step_id.clone(),
                        tool: step.tool.clone(),
                        result: serde_json::json!(null),
                        duration_ms: 0,
                        status: "skipped".to_string(),
                        error: Some(format!("Condition evaluation failed: {}", e)),
                    });
                    // Still mark as completed so dependents can proceed
                    completed_steps.insert(step.step_id.clone());
                    continue;
                }
            }
        }

        // Build tool arguments
        let arguments = build_step_arguments(step, &context);

        info!(
            step_id = %step.step_id,
            tool = %step.tool,
            "Executing step"
        );

        // Execute tool
        match handle_tool_call(state, &step.tool, Some(arguments.clone())).await {
            Ok(result) => {
                let duration = step_start.elapsed().as_millis() as i64;

                // Store result if configured
                if let Some(store_key) = &step.store_as {
                    context.insert(store_key.clone(), result.clone());
                }

                completed_steps.insert(step.step_id.clone());

                info!(
                    step_id = %step.step_id,
                    duration_ms = duration,
                    "Step completed successfully"
                );

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
                    warn!(
                        step_id = %step.step_id,
                        error = %e,
                        "Optional step failed, continuing"
                    );
                    // Mark as completed so dependents can proceed
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
                    warn!(
                        step_id = %step.step_id,
                        error = %e,
                        "Required step failed, stopping workflow"
                    );
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
                        steps_completed: step_results
                            .iter()
                            .filter(|s| s.status == "success")
                            .count(),
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

    let successful = step_results
        .iter()
        .filter(|s| s.status == "success")
        .count();
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

    info!(
        preset_id = %preset.id,
        status = status,
        steps_completed = successful,
        duration_ms = start.elapsed().as_millis(),
        "Preset execution completed"
    );

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

/// Build tool arguments from step configuration and context.
fn build_step_arguments(
    step: &PresetStep,
    context: &HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    let mut args = serde_json::Map::new();

    // Add mapped inputs from context
    for (param, source) in &step.input_map {
        // Handle nested references like "analysis.thought_id"
        let value = if source.contains('.') {
            let parts: Vec<&str> = source.splitn(2, '.').collect();
            context.get(parts[0]).and_then(|v| v.get(parts[1])).cloned()
        } else {
            context.get(source).cloned()
        };

        if let Some(v) = value {
            args.insert(param.clone(), v);
        }
    }

    // Add static inputs (override mapped if same key)
    for (key, value) in &step.static_inputs {
        args.insert(key.clone(), value.clone());
    }

    // Add session_id from context if available and not already set
    if !args.contains_key("session_id") {
        if let Some(session_id) = context.get("session_id") {
            args.insert("session_id".to_string(), session_id.clone());
        }
    }

    serde_json::Value::Object(args)
}

/// Evaluate a step condition against the context.
///
/// Returns `Ok(true)` if condition is met, `Ok(false)` if not met,
/// or `Err(ConditionError)` if the condition is invalid (e.g., type mismatch).
fn evaluate_condition(
    condition: &StepCondition,
    context: &HashMap<String, serde_json::Value>,
) -> Result<bool, ConditionError> {
    // Get the source value to check
    let source_value = match (&condition.source_step, &condition.field) {
        (Some(step), Some(field)) => context.get(step).and_then(|v| v.get(field)),
        (Some(step), None) => context.get(step),
        (None, Some(field)) => {
            // Look in the entire context for the field
            context.get(field)
        }
        (None, None) => None,
    };

    let Some(val) = source_value else {
        // If we can't find the value, condition fails
        return Ok(false);
    };

    let field_name = condition
        .field
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    match condition.operator.as_str() {
        "gt" => {
            let val_f = val
                .as_f64()
                .ok_or_else(|| ConditionError::NonNumericSource {
                    field: field_name.clone(),
                    actual_type: json_type_name(val).to_string(),
                })?;
            let threshold =
                condition
                    .value
                    .as_f64()
                    .ok_or_else(|| ConditionError::NonNumericThreshold {
                        operator: "gt".to_string(),
                        actual_type: json_type_name(&condition.value).to_string(),
                    })?;
            Ok(val_f > threshold)
        }
        "gte" => {
            let val_f = val
                .as_f64()
                .ok_or_else(|| ConditionError::NonNumericSource {
                    field: field_name.clone(),
                    actual_type: json_type_name(val).to_string(),
                })?;
            let threshold =
                condition
                    .value
                    .as_f64()
                    .ok_or_else(|| ConditionError::NonNumericThreshold {
                        operator: "gte".to_string(),
                        actual_type: json_type_name(&condition.value).to_string(),
                    })?;
            Ok(val_f >= threshold)
        }
        "lt" => {
            let val_f = val
                .as_f64()
                .ok_or_else(|| ConditionError::NonNumericSource {
                    field: field_name.clone(),
                    actual_type: json_type_name(val).to_string(),
                })?;
            let threshold =
                condition
                    .value
                    .as_f64()
                    .ok_or_else(|| ConditionError::NonNumericThreshold {
                        operator: "lt".to_string(),
                        actual_type: json_type_name(&condition.value).to_string(),
                    })?;
            Ok(val_f < threshold)
        }
        "lte" => {
            let val_f = val
                .as_f64()
                .ok_or_else(|| ConditionError::NonNumericSource {
                    field: field_name.clone(),
                    actual_type: json_type_name(val).to_string(),
                })?;
            let threshold =
                condition
                    .value
                    .as_f64()
                    .ok_or_else(|| ConditionError::NonNumericThreshold {
                        operator: "lte".to_string(),
                        actual_type: json_type_name(&condition.value).to_string(),
                    })?;
            Ok(val_f <= threshold)
        }
        "eq" => Ok(val == &condition.value),
        "neq" => Ok(val != &condition.value),
        "contains" => {
            let s = val.as_str().ok_or_else(|| ConditionError::ExpectedString {
                operator: "contains".to_string(),
                actual_type: json_type_name(val).to_string(),
            })?;
            let needle =
                condition
                    .value
                    .as_str()
                    .ok_or_else(|| ConditionError::ExpectedString {
                        operator: "contains".to_string(),
                        actual_type: json_type_name(&condition.value).to_string(),
                    })?;
            Ok(s.contains(needle))
        }
        "exists" => Ok(true), // If we got here, the value exists
        _ => {
            warn!(operator = %condition.operator, "Unknown condition operator");
            Ok(true) // Default to true for unknown operators
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_step_arguments_simple() {
        let step = PresetStep::new("test", "reasoning_linear")
            .with_input("content", "code")
            .with_static("confidence", serde_json::json!(0.8));

        let context =
            HashMap::from([("code".to_string(), serde_json::json!("function test() {}"))]);

        let args = build_step_arguments(&step, &context);
        assert_eq!(
            args.get("content"),
            Some(&serde_json::json!("function test() {}"))
        );
        assert_eq!(args.get("confidence"), Some(&serde_json::json!(0.8)));
    }

    #[test]
    fn test_build_step_arguments_nested() {
        let step = PresetStep::new("test", "reasoning_linear")
            .with_input("thought_id", "analysis.thought_id");

        let context = HashMap::from([(
            "analysis".to_string(),
            serde_json::json!({
                "thought_id": "thought-123",
                "confidence": 0.9
            }),
        )]);

        let args = build_step_arguments(&step, &context);
        assert_eq!(
            args.get("thought_id"),
            Some(&serde_json::json!("thought-123"))
        );
    }

    #[test]
    fn test_evaluate_condition_gt() {
        let context = HashMap::from([(
            "analysis".to_string(),
            serde_json::json!({"confidence": 0.8}),
        )]);

        let condition = StepCondition {
            condition_type: "confidence_threshold".to_string(),
            field: Some("confidence".to_string()),
            operator: "gt".to_string(),
            value: serde_json::json!(0.7),
            source_step: Some("analysis".to_string()),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());

        let condition_fail = StepCondition {
            condition_type: "confidence_threshold".to_string(),
            field: Some("confidence".to_string()),
            operator: "gt".to_string(),
            value: serde_json::json!(0.9),
            source_step: Some("analysis".to_string()),
        };

        assert!(!evaluate_condition(&condition_fail, &context).unwrap());
    }

    #[test]
    fn test_evaluate_condition_eq() {
        let context = HashMap::from([(
            "result".to_string(),
            serde_json::json!({"status": "success"}),
        )]);

        let condition = StepCondition {
            condition_type: "result_match".to_string(),
            field: Some("status".to_string()),
            operator: "eq".to_string(),
            value: serde_json::json!("success"),
            source_step: Some("result".to_string()),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_condition_contains() {
        let context = HashMap::from([(
            "output".to_string(),
            serde_json::json!({"message": "Error: connection failed"}),
        )]);

        let condition = StepCondition {
            condition_type: "result_match".to_string(),
            field: Some("message".to_string()),
            operator: "contains".to_string(),
            value: serde_json::json!("Error"),
            source_step: Some("output".to_string()),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_condition_missing_value() {
        let context = HashMap::new();

        let condition = StepCondition {
            condition_type: "test".to_string(),
            field: Some("missing".to_string()),
            operator: "eq".to_string(),
            value: serde_json::json!("value"),
            source_step: Some("nonexistent".to_string()),
        };

        // Missing values should fail the condition (returns Ok(false))
        assert!(!evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_condition_invalid_source_type() {
        let context = HashMap::from([(
            "analysis".to_string(),
            serde_json::json!({"confidence": "not-a-number"}),
        )]);

        let condition = StepCondition {
            condition_type: "confidence_threshold".to_string(),
            field: Some("confidence".to_string()),
            operator: "gt".to_string(),
            value: serde_json::json!(0.7),
            source_step: Some("analysis".to_string()),
        };

        let result = evaluate_condition(&condition, &context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("expected number"));
        assert!(err.to_string().contains("confidence"));
    }

    #[test]
    fn test_evaluate_condition_invalid_threshold_type() {
        let context = HashMap::from([(
            "analysis".to_string(),
            serde_json::json!({"confidence": 0.8}),
        )]);

        let condition = StepCondition {
            condition_type: "confidence_threshold".to_string(),
            field: Some("confidence".to_string()),
            operator: "gt".to_string(),
            value: serde_json::json!("not-a-number"),
            source_step: Some("analysis".to_string()),
        };

        let result = evaluate_condition(&condition, &context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid threshold"));
        assert!(err.to_string().contains("gt"));
    }

    #[test]
    fn test_evaluate_condition_contains_invalid_source() {
        let context = HashMap::from([(
            "output".to_string(),
            serde_json::json!({"message": 12345}), // number, not string
        )]);

        let condition = StepCondition {
            condition_type: "result_match".to_string(),
            field: Some("message".to_string()),
            operator: "contains".to_string(),
            value: serde_json::json!("Error"),
            source_step: Some("output".to_string()),
        };

        let result = evaluate_condition(&condition, &context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("contains"));
        assert!(err.to_string().contains("number"));
    }

    #[test]
    fn test_evaluate_condition_contains_invalid_needle() {
        let context = HashMap::from([(
            "output".to_string(),
            serde_json::json!({"message": "Error: connection failed"}),
        )]);

        let condition = StepCondition {
            condition_type: "result_match".to_string(),
            field: Some("message".to_string()),
            operator: "contains".to_string(),
            value: serde_json::json!(123), // number, not string
            source_step: Some("output".to_string()),
        };

        let result = evaluate_condition(&condition, &context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("contains"));
    }

    #[test]
    fn test_evaluate_condition_gte() {
        let context = HashMap::from([("analysis".to_string(), serde_json::json!({"score": 0.7}))]);

        let condition = StepCondition {
            condition_type: "score_threshold".to_string(),
            field: Some("score".to_string()),
            operator: "gte".to_string(),
            value: serde_json::json!(0.7),
            source_step: Some("analysis".to_string()),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_condition_lt() {
        let context = HashMap::from([("analysis".to_string(), serde_json::json!({"score": 0.3}))]);

        let condition = StepCondition {
            condition_type: "score_threshold".to_string(),
            field: Some("score".to_string()),
            operator: "lt".to_string(),
            value: serde_json::json!(0.5),
            source_step: Some("analysis".to_string()),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_condition_lte() {
        let context = HashMap::from([("analysis".to_string(), serde_json::json!({"score": 0.5}))]);

        let condition = StepCondition {
            condition_type: "score_threshold".to_string(),
            field: Some("score".to_string()),
            operator: "lte".to_string(),
            value: serde_json::json!(0.5),
            source_step: Some("analysis".to_string()),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_condition_error_display() {
        let err = ConditionError::NonNumericSource {
            field: "confidence".to_string(),
            actual_type: "string".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid source value for field 'confidence': expected number, got string"
        );

        let err = ConditionError::NonNumericThreshold {
            operator: "gt".to_string(),
            actual_type: "string".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid threshold value for operator 'gt': expected number, got string"
        );

        let err = ConditionError::ExpectedString {
            operator: "contains".to_string(),
            actual_type: "number".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid value for 'contains' operator: expected string, got number"
        );
    }

    #[test]
    fn test_json_type_name() {
        assert_eq!(json_type_name(&serde_json::json!(null)), "null");
        assert_eq!(json_type_name(&serde_json::json!(true)), "boolean");
        assert_eq!(json_type_name(&serde_json::json!(42)), "number");
        assert_eq!(json_type_name(&serde_json::json!(3.14)), "number");
        assert_eq!(json_type_name(&serde_json::json!("hello")), "string");
        assert_eq!(json_type_name(&serde_json::json!([1, 2, 3])), "array");
        assert_eq!(
            json_type_name(&serde_json::json!({"key": "value"})),
            "object"
        );
    }
}
