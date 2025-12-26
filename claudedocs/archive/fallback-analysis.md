# Fallback, Workaround, and Silent Failure Analysis

Generated: 2025-12-19

## Executive Summary

- **Total fallback patterns found**: 78
- **Critical issues**: 4
- **Moderate issues**: 6
- **Low severity**: 68

---

## Critical Issues

### 1. Auto-Mode Silent Degradation

**Location**: `src/modes/auto.rs:158-164`

```rust
let recommended_mode = auto_response.recommended_mode.parse().unwrap_or_else(|_| {
    warn!(
        invalid_mode = %auto_response.recommended_mode,
        "Invalid mode returned by auto-router, falling back to Linear"
    );
    ReasoningMode::Linear
});
```

**Problem**: When Langbase returns an invalid mode string, the system silently falls back to Linear mode. Users receive a response but don't know their routing request failed.

**Impact**: High - Wrong reasoning mode used without user awareness.

**Recommendation**: Return error or include `fallback_used: true` in API response.

---

### 2. Timestamp Corruption

**Location**: `src/storage/sqlite.rs:1770-1782`

```rust
match DateTime::parse_from_rfc3339(ts_str) {
    Ok(dt) => dt.with_timezone(&chrono::Utc),
    Err(e) => {
        warn!(
            error = %e,
            timestamp = ts_str,
            context = context,
            "Failed to parse timestamp, using current time as fallback"
        );
        chrono::Utc::now()
    }
}
```

**Problem**: Invalid timestamps in the database are replaced with the current time instead of failing or being marked as invalid.

**Impact**: High - Creates misleading audit trails and data integrity issues.

**Recommendation**: Fail explicitly or add a flag indicating the timestamp was reconstructed.

---

### 3. Database Records Silently Skipped

**Location**: `src/storage/sqlite.rs:515-524`

```rust
let input: serde_json::Value = serde_json::from_str(&input_str).ok()?;
let output: Option<serde_json::Value> =
    output_str.and_then(|s| serde_json::from_str(&s).ok());
let created_at = DateTime::parse_from_rfc3339(&created_at_str)
    .ok()?
    .with_timezone(&Utc);
```

**Problem**: The `.ok()?` pattern silently filters out records that fail to parse. No logging occurs when records are skipped.

**Impact**: High - Data loss without any notification or logging.

**Recommendation**: Log warnings when records are skipped due to parse failures.

---

### 4. Enum Parse Defaults

**Location**: `src/storage/sqlite.rs:1784-1794`

```rust
fn parse_enum_with_logging<T: std::str::FromStr + Default>(value: &str, context: &str) -> T {
    match value.parse() {
        Ok(parsed) => parsed,
        Err(_) => {
            warn!(
                value = value,
                context = context,
                default = %std::any::type_name::<T>(),
                "Failed to parse enum value, using default"
            );
            T::default()
        }
    }
}
```

**Problem**: Unknown enum values silently become defaults. While logged, the caller has no way to know this happened.

**Impact**: Medium-High - Incorrect behavior without caller awareness.

**Recommendation**: Consider returning Result instead of silent default.

---

## Moderate Issues

### 5. Config Defaults Not Logged at Startup

**Location**: `src/config/mod.rs:176-214`

The following defaults are used without explicit startup logging:

| Variable | Default Value |
|----------|---------------|
| LANGBASE_BASE_URL | https://api.langbase.com |
| DATABASE_PATH | ./data/reasoning.db |
| DATABASE_MAX_CONNECTIONS | 5 |
| LOG_LEVEL | info |
| REQUEST_TIMEOUT_MS | 30000 |
| MAX_RETRIES | 3 |
| RETRY_DELAY_MS | 1000 |

**Recommendation**: Log all defaulted configuration values at startup.

---

### 6. Pipe Name Fallbacks

**Location**: `src/config/mod.rs:276-281`

```rust
linear: env::var("PIPE_LINEAR").unwrap_or_else(|_| "linear-reasoning-v1".to_string()),
tree: env::var("PIPE_TREE").unwrap_or_else(|_| "tree-reasoning-v1".to_string()),
divergent: env::var("PIPE_DIVERGENT").unwrap_or_else(|_| "divergent-reasoning-v1".to_string()),
reflection: env::var("PIPE_REFLECTION").unwrap_or_else(|_| "reflection-v1".to_string()),
auto_router: env::var("PIPE_AUTO").unwrap_or_else(|_| "mode-router-v1".to_string()),
```

**Problem**: Hardcoded pipe names used when environment variables are missing.

**Impact**: Medium - May connect to wrong Langbase pipes.

---

### 7. JSON Deserialization Returns Empty/Null

**Location**: `src/storage/sqlite.rs:2166-2203`

Multiple fields use `unwrap_or_else` to return empty or null values:

- Decision options -> `Vec::new()`
- Decision criteria -> `None`
- Recommendation -> `Value::Null`
- Scores -> `Value::Null`
- Stakeholders -> `Value::Null`
- Synthesis -> `Value::Null`

**Problem**: Partial data returned instead of errors.

**Impact**: Medium - Downstream code may misbehave with incomplete data.

---

### 8. Serialization Error Masking

**Location**: `src/modes/mod.rs:53-68`

```rust
pub(crate) fn serialize_for_log<T: serde::Serialize>(
    value: &T,
    context: &str,
) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|e| {
        warn!(
            error = %e,
            context = %context,
            "Failed to serialize value for invocation log"
        );
        serde_json::json!({
            "serialization_error": e.to_string(),
            "context": context
        })
    })
}
```

**Problem**: Serialization failures converted to error JSON objects silently.

**Impact**: Low-Medium - Invocation logs may contain error objects instead of actual data.

---

### 9. Debug Config Fallback Strings

**Location**: `src/server/handlers.rs:795-813`

```rust
let detection_pipe = pipes
    .detection
    .as_ref()
    .and_then(|d| d.pipe.clone())
    .unwrap_or_else(|| "<fallback: detection-v1>".to_string());
```

**Problem**: Debug endpoint shows fallback markers, but actual runtime uses these invisibly.

**Impact**: Low - Debug tool only, but inconsistent with runtime behavior.

---

### 10. Invocation Logging Failures Swallowed

**Location**: Multiple files in `src/modes/`

```rust
if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
    warn!(
        error = %log_err,
        tool = %invocation.tool_name,
        "Failed to log invocation - audit trail incomplete"
    );
}
```

**Problem**: Audit trail gaps when storage fails.

**Impact**: Medium - Operations succeed but aren't logged.

---

## Pattern Statistics

### Fallback Types Found

| Pattern | Count |
|---------|-------|
| unwrap_or_else | 48 |
| unwrap_or_default | 15 |
| unwrap_or(value) | 12 |
| .ok()? chains | 8 |
| .ok().and_then() | 6 |

### Files with Highest Fallback Density

1. `src/storage/sqlite.rs` - 28 patterns
2. `src/config/mod.rs` - 18 patterns
3. `src/modes/auto.rs` - 8 patterns
4. `src/modes/got.rs` - 7 patterns
5. `src/modes/evidence.rs` - 6 patterns

---

## Existing Mitigations

The codebase already has good practices in place:

1. **Fallback metrics tracking** via `FallbackMetricsSummary` struct and `reasoning_fallback_metrics` MCP tool
2. **STRICT_MODE** environment variable option referenced in fallback recommendations
3. **Structured logging** with `tracing` for most fallback events
4. **Invocation tracking** with `fallback_used` and `fallback_type` fields in storage

---

## Remediation Priorities

### P0 - Immediate

1. Auto-mode silent degradation - Add fallback indicator to response
2. Timestamp fallback corruption - Fail or mark data as suspect

### P1 - Soon

3. Log all config defaults at startup
4. Add fallback_used field to API responses
5. Improve DB record skip logging

### P2 - When Convenient

6. Consider strict mode for JSON parsing
7. Review enum default behavior

---

## No Issues Found

- No `todo!()` macros
- No `unimplemented!()` macros
- No `panic!()` calls in production code
- `.unwrap()` and `.expect()` only used in tests
