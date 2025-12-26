# Design: Fix Auto-Mode Silent Degradation

## Problem Statement

When Langbase returns an invalid mode string in `src/modes/auto.rs:158-164`, the system silently falls back to Linear mode. Users receive a response but don't know their routing request failed.

```rust
// Current problematic code
let recommended_mode = auto_response.recommended_mode.parse().unwrap_or_else(|_| {
    warn!(
        invalid_mode = %auto_response.recommended_mode,
        "Invalid mode returned by auto-router, falling back to Linear"
    );
    ReasoningMode::Linear
});
```

## Design Goals

1. Make fallback behavior transparent to API consumers
2. Allow callers to distinguish between successful routing and fallback
3. Track fallback usage in metrics
4. Maintain backward compatibility with existing API consumers

## Solution Design

### Option A: Add Fallback Fields to AutoResult (Recommended)

Add two new fields to `AutoResult` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoResult {
    pub recommended_mode: ReasoningMode,
    pub confidence: f64,
    pub rationale: String,
    pub complexity: f64,
    pub alternative_modes: Vec<ModeRecommendation>,

    // NEW FIELDS
    /// Whether a fallback was used due to invalid mode from Langbase
    #[serde(default)]
    pub fallback_used: bool,

    /// The original invalid mode string if fallback was used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_invalid_mode: Option<String>,
}
```

**Pros:**
- Backward compatible (new fields have defaults)
- Transparent to consumers
- Easy to track in metrics
- No breaking changes

**Cons:**
- Slightly larger response payload

### Option B: Return Error on Invalid Mode

```rust
let recommended_mode = auto_response.recommended_mode.parse().map_err(|_| {
    AppError::Tool(ToolError::ParseFailed {
        mode: "auto".to_string(),
        message: format!("Invalid mode '{}' returned by auto-router",
                        auto_response.recommended_mode),
    })
})?;
```

**Pros:**
- Explicit failure - caller knows something went wrong
- Forces caller to handle the error case

**Cons:**
- Breaking change - callers that worked before will now fail
- May be too aggressive for a routing fallback

### Option C: Configurable Behavior via STRICT_MODE

Use existing `STRICT_MODE` environment variable pattern:

```rust
if config.strict_mode {
    // Return error
    return Err(AppError::Tool(ToolError::ParseFailed { ... }));
} else {
    // Fallback with transparency
    (ReasoningMode::Linear, true, Some(auto_response.recommended_mode.clone()))
}
```

**Pros:**
- Flexible - operators choose behavior
- Consistent with existing strict mode pattern

**Cons:**
- More complex implementation
- Two code paths to maintain

## Recommended Solution: Option A + Metrics

Implement Option A with enhanced metrics tracking.

### Implementation Plan

#### Step 1: Update AutoResult Struct (src/modes/auto.rs)

```rust
/// Result of auto mode routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoResult {
    /// The recommended reasoning mode.
    pub recommended_mode: ReasoningMode,
    /// Confidence in the recommendation (0.0-1.0).
    pub confidence: f64,
    /// Explanation for the recommendation.
    pub rationale: String,
    /// Estimated problem complexity (0.0-1.0).
    pub complexity: f64,
    /// Alternative mode recommendations.
    pub alternative_modes: Vec<ModeRecommendation>,
    /// Whether a fallback was used due to invalid mode from Langbase.
    #[serde(default)]
    pub fallback_used: bool,
    /// The original invalid mode string if fallback was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_invalid_mode: Option<String>,
}
```

#### Step 2: Update route() Method (src/modes/auto.rs:157-196)

```rust
// Parse mode with fallback tracking
let (recommended_mode, fallback_used, original_invalid_mode) =
    match auto_response.recommended_mode.parse() {
        Ok(mode) => (mode, false, None),
        Err(_) => {
            warn!(
                invalid_mode = %auto_response.recommended_mode,
                "Invalid mode returned by auto-router, falling back to Linear"
            );
            (
                ReasoningMode::Linear,
                true,
                Some(auto_response.recommended_mode.clone())
            )
        }
    };

// Update invocation to track fallback
if fallback_used {
    invocation = invocation.with_fallback("invalid_mode_parse");
}

// ... existing logging ...

Ok(AutoResult {
    recommended_mode,
    confidence: auto_response.confidence,
    rationale: auto_response.rationale,
    complexity: auto_response.complexity,
    alternative_modes: alternatives,
    fallback_used,
    original_invalid_mode,
})
```

#### Step 3: Update local_heuristics() Return Values

All `local_heuristics()` return points need updating:

```rust
fn local_heuristics(&self, params: &AutoParams) -> Option<AutoResult> {
    // ... existing logic ...

    return Some(AutoResult {
        recommended_mode: ReasoningMode::Linear,
        confidence: 0.9,
        rationale: "Short content is best handled with linear reasoning".to_string(),
        complexity: 0.2,
        alternative_modes: vec![],
        fallback_used: false,  // NEW
        original_invalid_mode: None,  // NEW
    });
}
```

#### Step 4: Update Tests (src/modes/auto.rs tests section)

Add tests for new fields:

```rust
#[test]
fn test_auto_result_fallback_fields_default() {
    let json = r#"{
        "recommended_mode": "linear",
        "confidence": 0.9,
        "rationale": "test",
        "complexity": 0.5,
        "alternative_modes": []
    }"#;
    let result: AutoResult = serde_json::from_str(json).unwrap();
    assert!(!result.fallback_used);
    assert!(result.original_invalid_mode.is_none());
}

#[test]
fn test_auto_result_with_fallback() {
    let result = AutoResult {
        recommended_mode: ReasoningMode::Linear,
        confidence: 0.5,
        rationale: "Fallback due to invalid mode".to_string(),
        complexity: 0.5,
        alternative_modes: vec![],
        fallback_used: true,
        original_invalid_mode: Some("invalid_mode_xyz".to_string()),
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"fallback_used\":true"));
    assert!(json.contains("\"original_invalid_mode\":\"invalid_mode_xyz\""));
}

#[test]
fn test_auto_result_fallback_not_serialized_when_none() {
    let result = AutoResult {
        recommended_mode: ReasoningMode::Linear,
        confidence: 0.9,
        rationale: "test".to_string(),
        complexity: 0.5,
        alternative_modes: vec![],
        fallback_used: false,
        original_invalid_mode: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    // original_invalid_mode should be skipped when None
    assert!(!json.contains("original_invalid_mode"));
}
```

### Files to Modify

| File | Changes |
|------|---------|
| `src/modes/auto.rs` | Add fields to AutoResult, update route(), update local_heuristics() |
| `src/modes/auto.rs` (tests) | Add tests for new fields |

### API Response Change

Before:
```json
{
  "recommended_mode": "linear",
  "confidence": 0.85,
  "rationale": "...",
  "complexity": 0.5,
  "alternative_modes": []
}
```

After (when fallback used):
```json
{
  "recommended_mode": "linear",
  "confidence": 0.85,
  "rationale": "...",
  "complexity": 0.5,
  "alternative_modes": [],
  "fallback_used": true,
  "original_invalid_mode": "invalid_xyz"
}
```

After (normal operation):
```json
{
  "recommended_mode": "linear",
  "confidence": 0.85,
  "rationale": "...",
  "complexity": 0.5,
  "alternative_modes": [],
  "fallback_used": false
}
```

### Backward Compatibility

- `fallback_used` defaults to `false` via `#[serde(default)]`
- `original_invalid_mode` is skipped when `None`
- Existing consumers parsing the response will work unchanged
- New consumers can check `fallback_used` field

### Metrics Integration

The invocation is already tracked. Adding `.with_fallback("invalid_mode_parse")` will:
1. Set `fallback_used = true` on the Invocation
2. Set `fallback_type = "invalid_mode_parse"`
3. Appear in `reasoning_fallback_metrics` tool output

## Effort Estimate

- Implementation: ~30 minutes
- Testing: ~15 minutes
- Total: ~45 minutes

## Verification Checklist

- [ ] AutoResult struct updated with new fields
- [ ] route() method tracks fallback and populates new fields
- [ ] local_heuristics() returns include new fields (set to false/None)
- [ ] Invocation marked with fallback when fallback used
- [ ] Unit tests pass
- [ ] cargo clippy passes
- [ ] cargo fmt passes
