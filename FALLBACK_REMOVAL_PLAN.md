# Fallback Removal and Error Handling Improvement Plan

## Executive Summary

This plan addresses the systematic removal of fallback patterns that mask Langbase pipe failures, replacing them with proper error propagation to enable accurate integration testing.

---

## Phase 1: New Error Types

### 1.1 Add PipeError Variants to `src/error/mod.rs`

```rust
/// Langbase API errors for pipe communication.
#[derive(Debug, Error)]
pub enum LangbaseError {
    // ... existing variants ...

    /// Pipe response parsing failed - no fallback available
    #[error("Response parse failed for pipe '{pipe}': {message}")]
    ResponseParseFailed {
        /// Name of the pipe that returned unparseable response
        pipe: String,
        /// Description of the parse failure
        message: String,
        /// Raw response content (truncated for logging)
        raw_response: String,
    },

    /// Pipe not found (404 error)
    #[error("Pipe not found: {pipe} (verify pipe exists on Langbase)")]
    PipeNotFound {
        /// Name of the missing pipe
        pipe: String,
    },
}
```

### 1.2 Add ParseError to ToolError

```rust
/// Tool-specific errors with structured details.
#[derive(Debug, Error)]
pub enum ToolError {
    // ... existing variants ...

    /// Response parsing failed (strict mode - no fallback)
    #[error("Parse error in {mode} mode: {message}")]
    ParseFailed {
        /// Reasoning mode that failed
        mode: String,
        /// Description of parse failure
        message: String,
    },

    /// Pipe unavailable and no fallback allowed
    #[error("Pipe unavailable: {pipe} - {reason}")]
    PipeUnavailable {
        /// Name of the unavailable pipe
        pipe: String,
        /// Reason for unavailability
        reason: String,
    },
}
```

---

## Phase 2: Strict Mode Configuration

### 2.1 Add to `src/config/mod.rs`

```rust
/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    // ... existing fields ...

    /// Error handling behavior configuration.
    pub error_handling: ErrorHandlingConfig,
}

/// Error handling behavior configuration.
#[derive(Debug, Clone)]
pub struct ErrorHandlingConfig {
    /// When true, parsing errors return Err instead of fallback values.
    /// Recommended for integration testing.
    pub strict_mode: bool,

    /// When true, API failures return Err instead of local calculations.
    /// Recommended for production to ensure pipe coverage.
    pub require_pipe_response: bool,

    /// Maximum number of fallback usages before forcing error.
    /// 0 = unlimited (default behavior), >0 = limit
    pub max_fallback_count: u32,
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,  // Backward compatible
            require_pipe_response: false,
            max_fallback_count: 0,
        }
    }
}
```

### 2.2 Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `STRICT_MODE` | `false` | Enable strict error handling (no parse fallbacks) |
| `REQUIRE_PIPE_RESPONSE` | `false` | Fail instead of local calculation |
| `MAX_FALLBACK_COUNT` | `0` | Limit fallbacks per session (0=unlimited) |

### 2.3 Load in Config::from_env()

```rust
let error_handling = ErrorHandlingConfig {
    strict_mode: env::var("STRICT_MODE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false),
    require_pipe_response: env::var("REQUIRE_PIPE_RESPONSE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false),
    max_fallback_count: env::var("MAX_FALLBACK_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0),
};
```

---

## Phase 3: Fallback Removal by File

### 3.1 `src/modes/auto.rs` (lines 70-90)

**Current (problematic):**
```rust
fn from_completion(completion: &str) -> Self {
    match serde_json::from_str::<AutoResponse>(completion) {
        Ok(parsed) => parsed,
        Err(e) => {
            warn!("Failed to parse auto router response, using fallback");
            Self {
                recommended_mode: "linear".to_string(),
                confidence: 0.7,
                rationale: "Default to linear mode (fallback due to parse error)".to_string(),
                ...
            }
        }
    }
}
```

**New (strict mode aware):**
```rust
fn from_completion(completion: &str, strict_mode: bool) -> Result<Self, ToolError> {
    serde_json::from_str::<AutoResponse>(completion).map_err(|e| {
        if strict_mode {
            ToolError::ParseFailed {
                mode: "auto".to_string(),
                message: format!("JSON parse error: {}", e),
            }
        } else {
            warn!("Failed to parse auto router response, using fallback");
            // Return the fallback via a different mechanism
            return Ok(Self {
                recommended_mode: "linear".to_string(),
                confidence: 0.7,
                rationale: "Default to linear mode (fallback due to parse error)".to_string(),
                complexity: 0.5,
                metadata: None,
            });
        }
    })
}

// Better approach - separate function:
fn from_completion_strict(completion: &str) -> Result<Self, ToolError> {
    serde_json::from_str::<AutoResponse>(completion).map_err(|e| {
        ToolError::ParseFailed {
            mode: "auto".to_string(),
            message: format!("JSON parse error: {} | Preview: {}",
                e, &completion[..completion.len().min(200)]),
        }
    })
}
```

### 3.2 `src/modes/got.rs` (multiple locations)

**Files affected:**
- `from_completion()` at line 181-203 (GenerateResponse)
- `from_completion()` at line 282-306 (ScoreResponse)
- `from_completion()` at line 361-378 (AggregateResponse)
- `from_completion()` at line 443-450 (RefineResponse)

**Pattern to apply (same for all):**

```rust
impl GenerateResponse {
    fn from_completion(completion: &str) -> Result<Self, ToolError> {
        serde_json::from_str::<GenerateResponse>(completion).map_err(|e| {
            ToolError::ParseFailed {
                mode: "got_generate".to_string(),
                message: format!("Parse error: {} | Response preview: {}",
                    e, &completion[..completion.len().min(200)]),
            }
        })
    }
}
```

### 3.3 `src/modes/backtracking.rs` (lines 70-90)

```rust
impl BacktrackingResponse {
    fn from_completion(completion: &str) -> Result<Self, ToolError> {
        serde_json::from_str::<BacktrackingResponse>(completion).map_err(|e| {
            ToolError::ParseFailed {
                mode: "backtracking".to_string(),
                message: format!("Parse error: {}", e),
            }
        })
    }
}
```

### 3.4 `src/modes/evidence.rs` (lines 665-699) - CRITICAL

**Current (returns Ok on API failure):**
```rust
let response = match self.core.langbase().call_pipe(request).await {
    Ok(resp) => resp,
    Err(e) => {
        // Fallback: calculate locally
        warn!("Langbase call failed, using local Bayesian calculation");
        let (posterior, steps) = self.calculate_bayesian_update(&params);
        return Ok(ProbabilisticResult { ... });  // PROBLEM: Returns Ok!
    }
};
```

**New (propagate error in strict mode):**
```rust
let response = match self.core.langbase().call_pipe(request).await {
    Ok(resp) => resp,
    Err(e) => {
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.failure(e.to_string(), latency);
        self.core.storage().log_invocation(&invocation).await?;

        if self.config.error_handling.require_pipe_response {
            // Strict mode: propagate the error
            return Err(AppError::Langbase(e));
        }

        // Legacy fallback mode (deprecated)
        warn!(error = %e, "Langbase call failed, using local Bayesian calculation (DEPRECATED)");
        let (posterior, steps) = self.calculate_bayesian_update(&params);
        return Ok(ProbabilisticResult { ... });
    }
};
```

### 3.5 `src/modes/evidence.rs` (lines 923-957)

Same pattern - add strict mode check before fallback.

---

## Phase 4: Metrics Enhancement

### 4.1 Track Fallback Usage

Add to invocation logging:

```rust
pub struct InvocationMetadata {
    // ... existing fields ...

    /// Whether a fallback was used for this invocation
    pub fallback_used: bool,

    /// Type of fallback if used
    pub fallback_type: Option<String>,
}
```

### 4.2 New Metrics Endpoint

Add `reasoning_fallback_metrics` tool:

```json
{
    "total_fallbacks": 15,
    "fallbacks_by_type": {
        "parse_error": 10,
        "api_unavailable": 3,
        "local_calculation": 2
    },
    "fallbacks_by_pipe": {
        "auto-router-v1": 5,
        "got-generate-v1": 3,
        "decision-framework-v1": 2
    },
    "recommendation": "Consider enabling STRICT_MODE=true to surface actual failures"
}
```

---

## Phase 5: Implementation Order

### Step 1: Error Types (Low Risk)
1. Add new error variants to `src/error/mod.rs`
2. Run tests to ensure no regressions

### Step 2: Configuration (Low Risk)
1. Add `ErrorHandlingConfig` struct
2. Add env var loading
3. Default to backward-compatible (strict_mode=false)

### Step 3: Refactor from_completion Functions (Medium Risk)
1. Change signature to return `Result<Self, ToolError>`
2. Update all callers to handle Result
3. Add strict_mode parameter or access via config

### Step 4: Refactor API Fallbacks (High Risk)
1. Update `evidence.rs` Bayesian fallbacks
2. Test thoroughly - this changes behavior
3. Document migration path for users

### Step 5: Metrics (Low Risk)
1. Add fallback tracking to invocations
2. Add new metrics endpoint
3. Update documentation

---

## Phase 6: Testing Strategy

### 6.1 Unit Tests

```rust
#[test]
fn test_strict_mode_parse_error_propagates() {
    let bad_json = "not valid json";
    let result = AutoResponse::from_completion_strict(bad_json);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::ParseFailed { .. }));
}

#[test]
fn test_legacy_mode_parse_error_falls_back() {
    let bad_json = "not valid json";
    let result = AutoResponse::from_completion_legacy(bad_json);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().recommended_mode, "linear");
}
```

### 6.2 Integration Tests

```rust
#[tokio::test]
async fn test_strict_mode_detects_missing_pipe() {
    std::env::set_var("STRICT_MODE", "true");
    std::env::set_var("PIPE_AUTO", "nonexistent-pipe-v999");

    let result = reasoning_auto("test content").await;

    assert!(result.is_err());
    // Should see actual 404 error, not fallback response
}
```

---

## Phase 7: Migration Guide

### For Users

```bash
# Step 1: Enable strict mode in test environment
export STRICT_MODE=true
export REQUIRE_PIPE_RESPONSE=true

# Step 2: Run your integration tests
cargo test

# Step 3: Fix any pipe issues revealed by strict mode

# Step 4: Once all tests pass, enable in production
```

### Deprecation Timeline

| Version | Behavior |
|---------|----------|
| v1.1 | Add strict mode (opt-in) |
| v1.2 | Warn when fallbacks are used |
| v1.3 | strict_mode=true by default |
| v2.0 | Remove legacy fallback code |

---

## Summary of Files to Modify

| File | Changes | Risk |
|------|---------|------|
| `src/error/mod.rs` | Add error variants | Low |
| `src/config/mod.rs` | Add ErrorHandlingConfig | Low |
| `src/modes/auto.rs` | Refactor from_completion | Medium |
| `src/modes/got.rs` | Refactor 4x from_completion | Medium |
| `src/modes/backtracking.rs` | Refactor from_completion | Medium |
| `src/modes/evidence.rs` | Refactor API fallbacks | High |
| `src/storage/sqlite.rs` | Add fallback tracking | Low |

---

## Expected Outcome

After implementation:

1. **STRICT_MODE=true**: All pipe failures surface as errors
2. **Metrics show actual success rates** (not inflated by fallbacks)
3. **Integration tests fail when pipes are broken** (desired behavior)
4. **Backward compatible** for existing users (strict_mode=false default)
5. **Clear migration path** to strict mode
