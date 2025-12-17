//! Data types for workflow presets.

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
    #[serde(default)]
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
    /// Preset identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Category.
    pub category: String,
    /// Number of steps.
    pub step_count: usize,
    /// Estimated execution time.
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

impl PresetStep {
    /// Create a new preset step.
    pub fn new(step_id: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            tool: tool.into(),
            description: String::new(),
            input_map: HashMap::new(),
            static_inputs: HashMap::new(),
            condition: None,
            store_as: None,
            depends_on: Vec::new(),
            optional: false,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Add an input mapping.
    pub fn with_input(mut self, param: impl Into<String>, source: impl Into<String>) -> Self {
        self.input_map.insert(param.into(), source.into());
        self
    }

    /// Add a static input.
    pub fn with_static(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.static_inputs.insert(key.into(), value);
        self
    }

    /// Set the store key.
    pub fn store_as(mut self, key: impl Into<String>) -> Self {
        self.store_as = Some(key.into());
        self
    }

    /// Add dependencies.
    pub fn depends_on(mut self, deps: Vec<String>) -> Self {
        self.depends_on = deps;
        self
    }

    /// Mark as optional.
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_to_summary() {
        let preset = WorkflowPreset {
            id: "test".to_string(),
            name: "Test Preset".to_string(),
            description: "A test preset".to_string(),
            category: "testing".to_string(),
            steps: vec![
                PresetStep::new("step1", "tool1"),
                PresetStep::new("step2", "tool2"),
            ],
            input_schema: HashMap::new(),
            output_format: "json".to_string(),
            estimated_time: "1 minute".to_string(),
            tags: vec!["test".to_string()],
        };

        let summary = preset.to_summary();
        assert_eq!(summary.id, "test");
        assert_eq!(summary.name, "Test Preset");
        assert_eq!(summary.step_count, 2);
    }

    #[test]
    fn test_preset_step_builder() {
        let step = PresetStep::new("analyze", "reasoning_linear")
            .with_description("Analyze the input")
            .with_input("content", "code")
            .with_static("confidence", serde_json::json!(0.8))
            .store_as("analysis")
            .depends_on(vec!["init".to_string()])
            .optional();

        assert_eq!(step.step_id, "analyze");
        assert_eq!(step.tool, "reasoning_linear");
        assert_eq!(step.description, "Analyze the input");
        assert_eq!(step.input_map.get("content"), Some(&"code".to_string()));
        assert_eq!(
            step.static_inputs.get("confidence"),
            Some(&serde_json::json!(0.8))
        );
        assert_eq!(step.store_as, Some("analysis".to_string()));
        assert_eq!(step.depends_on, vec!["init".to_string()]);
        assert!(step.optional);
    }

    #[test]
    fn test_preset_result_serialization() {
        let result = PresetResult {
            preset_id: "test".to_string(),
            steps_completed: 2,
            steps_total: 3,
            step_results: vec![],
            final_output: Some(serde_json::json!({"key": "value"})),
            status: "partial".to_string(),
            duration_ms: 1500,
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"preset_id\":\"test\""));
        assert!(json.contains("\"steps_completed\":2"));
        assert!(!json.contains("\"error\"")); // Should be skipped when None
    }

    #[test]
    fn test_step_condition_serialization() {
        let condition = StepCondition {
            condition_type: "confidence_threshold".to_string(),
            field: Some("confidence".to_string()),
            operator: "gt".to_string(),
            value: serde_json::json!(0.7),
            source_step: Some("analyze".to_string()),
        };

        let json = serde_json::to_string(&condition).unwrap();
        let parsed: StepCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.operator, "gt");
    }
}
