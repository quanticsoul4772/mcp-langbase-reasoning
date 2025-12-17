# Implementation Plan: Response Parsing Fallbacks Fix

## Problem Statement

**Location**: 3 files with identical problematic patterns

| File | Function | Lines |
|------|----------|-------|
| `src/modes/divergent.rs` | `parse_response()` | 396-406 |
| `src/modes/tree.rs` | `parse_response()` | 439-449 |
| `src/modes/reflection.rs` | `parse_response()` | 519-529 |

**Current Code Pattern**:
```rust
let json_str = if completion.contains("```json") {
    completion
        .split("```json")
        .nth(1)
        .and_then(|s| s.split("```").next())
        .unwrap_or(completion)  // <-- Problem: silently uses raw completion
} else if completion.contains("```") {
    completion.split("```").nth(1).unwrap_or(completion)  // <-- Same issue
} else {
    completion
};
```

**Problem**: When JSON extraction from markdown code blocks fails:
1. The `contains("```json")` check passes, but `split().nth(1)` returns `None`
2. The fallback silently uses the raw `completion` text
3. Downstream `serde_json::from_str()` fails with a confusing JSON parse error
4. User sees "Failed to parse X response: expected value at line 1 column 1" - no indication the JSON block was malformed

**Example Scenario**:
```
Input: "Here's the result: ```json\n"  (incomplete block)
- contains("```json") → true ✓
- split("```json").nth(1) → Some("\n")
- split("```").next() → Some("\n")
- Result: "\n" → JSON parse fails with confusing error
```

---

## Solution Design

### Approach: Add Helper Function with Clear Errors

Create a dedicated `extract_json_from_completion()` function that:
1. Returns a `Result` with clear error messages for extraction failures
2. Provides context about what was expected vs. what was found
3. Logs a warning when falling back to raw completion
4. Centralizes the extraction logic (DRY)

**Helper Function**:
```rust
/// Extract JSON from a completion string, handling markdown code blocks.
///
/// Attempts extraction in this order:
/// 1. Try parsing as raw JSON first
/// 2. Extract from ```json ... ``` code blocks
/// 3. Extract from ``` ... ``` code blocks
/// 4. Return error if none work
fn extract_json_from_completion<'a>(completion: &'a str) -> Result<&'a str, String> {
    // First, try to parse as raw JSON (fast path)
    if completion.trim().starts_with('{') || completion.trim().starts_with('[') {
        return Ok(completion.trim());
    }

    // Try to extract from ```json ... ``` block
    if completion.contains("```json") {
        if let Some(json_content) = completion
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
        {
            let trimmed = json_content.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
            return Err("Found ```json block but content was empty".to_string());
        }
        return Err("Found ```json marker but block was malformed".to_string());
    }

    // Try to extract from ``` ... ``` block
    if completion.contains("```") {
        if let Some(content) = completion.split("```").nth(1) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
            return Err("Found ``` block but content was empty".to_string());
        }
        return Err("Found ``` marker but block was malformed".to_string());
    }

    // No code blocks found, try the raw completion
    Err(format!(
        "No JSON or code block found in response (first 100 chars: {})",
        &completion.chars().take(100).collect::<String>()
    ))
}
```

### Why This Approach?

1. **Clear Error Messages**: User knows exactly what failed (malformed block, empty content, etc.)
2. **Fast Path First**: Checks for raw JSON before doing string splitting
3. **No Silent Fallbacks**: Every failure path returns an explicit error
4. **Centralized Logic**: Single helper function reduces code duplication
5. **Debugging Aid**: Error includes preview of the completion for diagnosis

---

## Implementation Steps

### Step 1: Add Helper Function to Each Mode File

Add the `extract_json_from_completion` helper function after the imports in each file. While this creates some duplication, it maintains the self-contained nature of each mode module.

**Files**:
- `src/modes/divergent.rs`
- `src/modes/tree.rs`
- `src/modes/reflection.rs`

### Step 2: Update `parse_response` Functions

**Before** (divergent.rs example):
```rust
fn parse_response(&self, completion: &str) -> AppResult<DivergentResponse> {
    // Try to parse as JSON first
    if let Ok(response) = serde_json::from_str::<DivergentResponse>(completion) {
        return Ok(response);
    }

    // Try to extract JSON from markdown code blocks
    let json_str = if completion.contains("```json") {
        completion
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(completion)
    } else if completion.contains("```") {
        completion.split("```").nth(1).unwrap_or(completion)
    } else {
        completion
    };

    serde_json::from_str::<DivergentResponse>(json_str.trim()).map_err(|e| {
        ToolError::Reasoning {
            message: format!("Failed to parse divergent response: {}", e),
        }
        .into()
    })
}
```

**After**:
```rust
fn parse_response(&self, completion: &str) -> AppResult<DivergentResponse> {
    // Try to parse as JSON first (handles raw JSON and code blocks)
    let json_str = extract_json_from_completion(completion).map_err(|extraction_err| {
        warn!(
            error = %extraction_err,
            completion_preview = %completion.chars().take(200).collect::<String>(),
            "Failed to extract JSON from divergent response"
        );
        ToolError::Reasoning {
            message: format!("Failed to extract JSON from response: {}", extraction_err),
        }
    })?;

    serde_json::from_str::<DivergentResponse>(json_str).map_err(|e| {
        ToolError::Reasoning {
            message: format!("Failed to parse divergent response JSON: {}", e),
        }
        .into()
    })
}
```

### Step 3: Ensure `warn` is Imported

Verify each file imports `warn` from tracing. Based on previous work:
- `divergent.rs` - needs `warn` added (currently has `debug, info, warn`)
- `tree.rs` - needs `warn` added (currently has `debug, info, warn`)
- `reflection.rs` - needs `warn` added (currently has `debug, info, warn`)

(Note: Previous implementation already added `warn` to these files)

---

## Detailed Changes by File

### 1. `src/modes/divergent.rs`

**Add helper after imports** (after the `serialize_for_log` helper):
```rust
/// Extract JSON from a completion string, handling markdown code blocks.
fn extract_json_from_completion(completion: &str) -> Result<&str, String> {
    // Fast path: raw JSON
    let trimmed = completion.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(trimmed);
    }

    // Try ```json ... ``` blocks
    if completion.contains("```json") {
        return completion
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Found ```json block but content was empty or malformed".to_string());
    }

    // Try ``` ... ``` blocks
    if completion.contains("```") {
        return completion
            .split("```")
            .nth(1)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Found ``` block but content was empty or malformed".to_string());
    }

    Err(format!(
        "No JSON found in response. First 100 chars: '{}'",
        completion.chars().take(100).collect::<String>()
    ))
}
```

**Update `parse_response`**:
```rust
fn parse_response(&self, completion: &str) -> AppResult<DivergentResponse> {
    let json_str = extract_json_from_completion(completion).map_err(|e| {
        warn!(
            error = %e,
            completion_preview = %completion.chars().take(200).collect::<String>(),
            "Failed to extract JSON from divergent response"
        );
        ToolError::Reasoning {
            message: format!("Divergent response extraction failed: {}", e),
        }
    })?;

    serde_json::from_str::<DivergentResponse>(json_str).map_err(|e| {
        ToolError::Reasoning {
            message: format!("Failed to parse divergent response: {}", e),
        }
        .into()
    })
}
```

### 2. `src/modes/tree.rs`

Same pattern as divergent.rs, with "tree" in error messages.

### 3. `src/modes/reflection.rs`

Same pattern as divergent.rs, with "reflection" in error messages.

---

## Error Message Examples

**Before** (confusing):
```
Reasoning failed: Failed to parse divergent response: expected value at line 1 column 1
```

**After** (clear):
```
Reasoning failed: Divergent response extraction failed: Found ```json block but content was empty or malformed
```

Or:
```
Reasoning failed: Divergent response extraction failed: No JSON found in response. First 100 chars: 'I apologize, but I cannot...'
```

---

## Verification

### Tests
- Existing tests should pass (the happy path is unchanged)
- Consider adding unit tests for `extract_json_from_completion` edge cases

### Manual Verification
1. Run `cargo test` - all tests should pass
2. Run `cargo clippy -- -D warnings` - no warnings
3. Test with malformed LLM responses to see clear error messages

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing parsing | Low | Medium | Fast path checks raw JSON first |
| New errors for edge cases | Low | Low | Clear errors are better than confusing ones |
| Performance impact | Negligible | None | Fast path for common case |

---

## Success Criteria

- [ ] `extract_json_from_completion` helper added to all 3 files
- [ ] All `parse_response` functions updated to use the helper
- [ ] Error messages clearly indicate extraction vs. parsing failures
- [ ] Warning logs include completion preview for debugging
- [ ] All existing tests pass
- [ ] No clippy warnings
- [ ] Code compiles without errors

---

## Alternative Considered: Shared Module

Could create `src/utils/json_extraction.rs` with the helper exported for all modes. This was considered but rejected because:
1. Each mode is currently self-contained
2. The helper is small (~25 lines)
3. Keeping it local makes the module easier to understand in isolation

If more shared JSON utilities emerge, consider refactoring to a utility module.
