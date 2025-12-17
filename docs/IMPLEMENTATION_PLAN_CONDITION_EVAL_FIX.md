# Implementation Plan: Condition Evaluation Silent Defaults Fix

## Problem Statement

**Location**: `src/presets/executor.rs` (lines 271-290)

**Current Code Pattern**:
```rust
match condition.operator.as_str() {
    "gt" => {
        let val_f = val.as_f64().unwrap_or(0.0);
        let threshold = condition.value.as_f64().unwrap_or(0.0);
        val_f > threshold
    }
    // ... similar for gte, lt, lte
}
```

**Problem**: When condition values cannot be parsed as f64, they silently default to 0.0:
- `gt 0.5` becomes `0.0 > 0.0 = false` when parsing fails
- User gets no indication their condition was invalid
- Can cause unexpected workflow behavior (steps skipped without clear reason)

---

## Solution Design

### Approach: Change `evaluate_condition` to Return `Result<bool, ConditionError>`

Transform the function signature to return errors for invalid conditions, allowing callers to handle them appropriately.

**Key Changes**:
1. Add `ConditionError` enum in the presets module
2. Change `evaluate_condition` to return `Result<bool, ConditionError>`
3. Update caller in `execute_preset` to handle condition evaluation errors
4. Add appropriate logging and step result updates for invalid conditions

### Why This Approach?

1. **Explicit Failures**: Invalid conditions fail loudly, not silently
2. **Debuggability**: Error messages identify which field/value failed parsing
3. **User Visibility**: Step results show exactly why a condition failed
4. **Non-Breaking for Valid Cases**: Valid conditions behave exactly as before
5. **Consistent Error Handling**: Follows the pattern established elsewhere in the codebase

---

## Implementation Steps

### Step 1: Add ConditionError Type

**File**: `src/presets/executor.rs` (add near the top after imports)

```rust
use thiserror::Error;

/// Errors that can occur during condition evaluation.
#[derive(Debug, Error)]
pub enum ConditionError {
    /// Source value could not be parsed as a number for comparison.
    #[error("Invalid source value for field '{field}': expected number, got {actual_type}")]
    InvalidSourceValue {
        field: String,
        actual_type: String,
    },

    /// Threshold value in condition could not be parsed as a number.
    #[error("Invalid threshold value for operator '{operator}': expected number, got {actual_type}")]
    InvalidThresholdValue {
        operator: String,
        actual_type: String,
    },

    /// String comparison failed due to type mismatch.
    #[error("Invalid value for '{operator}' operator: expected string, got {actual_type}")]
    InvalidStringValue {
        operator: String,
        actual_type: String,
    },
}
```

### Step 2: Add Helper to Get JSON Type Name

**File**: `src/presets/executor.rs`

```rust
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
```

### Step 3: Update evaluate_condition Signature and Implementation

**File**: `src/presets/executor.rs`

Change from:
```rust
fn evaluate_condition(
    condition: &StepCondition,
    context: &HashMap<String, serde_json::Value>,
) -> bool {
```

To:
```rust
fn evaluate_condition(
    condition: &StepCondition,
    context: &HashMap<String, serde_json::Value>,
) -> Result<bool, ConditionError> {
```

### Step 4: Update Numeric Comparisons

**Before**:
```rust
"gt" => {
    let val_f = val.as_f64().unwrap_or(0.0);
    let threshold = condition.value.as_f64().unwrap_or(0.0);
    val_f > threshold
}
```

**After**:
```rust
"gt" => {
    let val_f = val.as_f64().ok_or_else(|| ConditionError::InvalidSourceValue {
        field: condition.field.clone().unwrap_or_else(|| "unknown".to_string()),
        actual_type: json_type_name(val).to_string(),
    })?;
    let threshold = condition.value.as_f64().ok_or_else(|| ConditionError::InvalidThresholdValue {
        operator: "gt".to_string(),
        actual_type: json_type_name(&condition.value).to_string(),
    })?;
    Ok(val_f > threshold)
}
```

### Step 5: Update "contains" Operator

**Before**:
```rust
"contains" => {
    if let (Some(s), Some(needle)) = (val.as_str(), condition.value.as_str()) {
        s.contains(needle)
    } else {
        false
    }
}
```

**After**:
```rust
"contains" => {
    let s = val.as_str().ok_or_else(|| ConditionError::InvalidStringValue {
        operator: "contains".to_string(),
        actual_type: json_type_name(val).to_string(),
    })?;
    let needle = condition.value.as_str().ok_or_else(|| ConditionError::InvalidStringValue {
        operator: "contains".to_string(),
        actual_type: json_type_name(&condition.value).to_string(),
    })?;
    Ok(s.contains(needle))
}
```

### Step 6: Update Caller in execute_preset

**Before** (lines 60-76):
```rust
if let Some(condition) = &step.condition {
    if !evaluate_condition(condition, &context) {
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
        completed_steps.insert(step.step_id.clone());
        continue;
    }
}
```

**After**:
```rust
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
            completed_steps.insert(step.step_id.clone());
            continue;
        }
    }
}
```

### Step 7: Update Unit Tests

**File**: `src/presets/executor.rs` (in tests module)

Add tests for error cases:

```rust
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
}

#[test]
fn test_evaluate_condition_contains_invalid_source() {
    let context = HashMap::from([(
        "output".to_string(),
        serde_json::json!({"message": 12345}),  // number, not string
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
}
```

Update existing tests to handle Result:

```rust
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

    assert!(evaluate_condition(&condition, &context).unwrap());  // Changed

    let condition_fail = StepCondition {
        condition_type: "confidence_threshold".to_string(),
        field: Some("confidence".to_string()),
        operator: "gt".to_string(),
        value: serde_json::json!(0.9),
        source_step: Some("analysis".to_string()),
    };

    assert!(!evaluate_condition(&condition_fail, &context).unwrap());  // Changed
}
```

---

## Complete Code Changes

### Change 1: Add imports and ConditionError

```diff
 use std::collections::{HashMap, HashSet};
 use std::sync::Arc;
 use std::time::Instant;
+use thiserror::Error;
 use tracing::{info, warn};

 use super::types::{PresetResult, PresetStep, StepCondition, StepResult, WorkflowPreset};
 use crate::error::McpResult;
 use crate::server::handle_tool_call;

 /// Shared state type alias for the executor.
 pub type SharedState = Arc<crate::server::AppState>;

+/// Errors that can occur during condition evaluation.
+#[derive(Debug, Error)]
+pub enum ConditionError {
+    /// Source value could not be parsed as a number for comparison.
+    #[error("Invalid source value for field '{field}': expected number, got {actual_type}")]
+    InvalidSourceValue {
+        field: String,
+        actual_type: String,
+    },
+
+    /// Threshold value in condition could not be parsed as a number.
+    #[error("Invalid threshold value for operator '{operator}': expected number, got {actual_type}")]
+    InvalidThresholdValue {
+        operator: String,
+        actual_type: String,
+    },
+
+    /// String comparison failed due to type mismatch.
+    #[error("Invalid value for '{operator}' operator: expected string, got {actual_type}")]
+    InvalidStringValue {
+        operator: String,
+        actual_type: String,
+    },
+}
+
+/// Get a human-readable type name for a JSON value.
+fn json_type_name(value: &serde_json::Value) -> &'static str {
+    match value {
+        serde_json::Value::Null => "null",
+        serde_json::Value::Bool(_) => "boolean",
+        serde_json::Value::Number(_) => "number",
+        serde_json::Value::String(_) => "string",
+        serde_json::Value::Array(_) => "array",
+        serde_json::Value::Object(_) => "object",
+    }
+}
```

### Change 2: Update evaluate_condition function

Replace the entire `evaluate_condition` function with the new implementation that returns `Result<bool, ConditionError>`.

### Change 3: Update caller in execute_preset

Replace the condition evaluation block with the new match-based error handling.

---

## Testing Strategy

### Unit Tests
1. Test `ConditionError` display messages
2. Test `json_type_name` helper
3. Test numeric comparison with invalid source value
4. Test numeric comparison with invalid threshold value
5. Test "contains" with invalid string values
6. Test valid conditions still work (update existing tests)

### Integration Tests
1. Verify existing presets still work
2. Test preset execution with invalid condition values
3. Verify step results contain meaningful error messages

### Manual Verification
1. Run `cargo test`
2. Run `cargo clippy -- -D warnings`
3. Test with actual workflow execution

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing presets | Low | Medium | Existing valid conditions unchanged |
| Test failures | Medium | Low | Update tests to handle Result |
| Performance impact | Negligible | None | Only adds error path handling |

---

## Success Criteria

- [ ] `ConditionError` enum added with descriptive variants
- [ ] `json_type_name` helper function added
- [ ] `evaluate_condition` returns `Result<bool, ConditionError>`
- [ ] All numeric comparisons (gt, gte, lt, lte) validate types
- [ ] String comparisons (contains) validate types
- [ ] Caller in `execute_preset` handles errors appropriately
- [ ] Step results include meaningful error messages for invalid conditions
- [ ] All existing tests updated and passing
- [ ] New error case tests added
- [ ] No clippy warnings
- [ ] Code compiles without errors
