# Handler Consistency Migration Plan

## Problem Statement

The handlers in `src/server/handlers.rs` have inconsistent patterns:
- **18 handlers** use the `execute_handler` helper for consistent error handling
- **11 handlers** use inline implementations with manual error mapping

This inconsistency leads to:
- Duplicated error handling boilerplate
- Inconsistent logging patterns
- Higher maintenance burden
- Potential for error handling bugs

## Current State Analysis

### Handlers Using `execute_handler` (18 total) ✅

| Handler | Line | Tool Name |
|---------|------|-----------|
| `handle_linear` | 71 | reasoning.linear |
| `handle_tree` | 79 | reasoning.tree |
| `handle_divergent` | 160 | reasoning.divergent |
| `handle_reflection` | 170 | reasoning.reflection |
| `handle_backtrack` | 207 | reasoning.backtrack |
| `handle_auto` | 270 | reasoning.auto |
| `handle_got_init` | 282 | reasoning.got.init |
| `handle_got_generate` | 290 | reasoning.got.generate |
| `handle_got_score` | 300 | reasoning.got.score |
| `handle_got_aggregate` | 310 | reasoning.got.aggregate |
| `handle_got_refine` | 320 | reasoning.got.refine |
| `handle_got_prune` | 330 | reasoning.got.prune |
| `handle_got_finalize` | 340 | reasoning.got.finalize |
| `handle_got_state` | 350 | reasoning.got.state |
| `handle_make_decision` | 865 | reasoning.make_decision |
| `handle_analyze_perspectives` | 875 | reasoning.analyze_perspectives |
| `handle_assess_evidence` | 888 | reasoning.assess_evidence |
| `handle_probabilistic` | 901 | reasoning.probabilistic |

### Handlers NOT Using `execute_handler` (11 total) ❌

| Handler | Line | Reason for Inline | Migration Strategy |
|---------|------|-------------------|-------------------|
| `handle_tree_focus` | 87 | Local struct `FocusParams` | Move struct, use execute_handler |
| `handle_tree_list` | 108 | Local struct `ListParams` | Move struct, use execute_handler |
| `handle_tree_complete` | 128 | Local struct + default fn | Move struct, use execute_handler |
| `handle_reflection_evaluate` | 180 | Local struct `EvaluateParams` | Move struct, use execute_handler |
| `handle_checkpoint_create` | 217 | Local struct `CreateParams` | Move struct, use execute_handler |
| `handle_checkpoint_list` | 246 | Local struct `ListParams` | Move struct, use execute_handler |
| `handle_detect_biases` | 428 | Complex validation + LLM call | Extract to mode, use execute_handler |
| `handle_detect_fallacies` | 560 | Complex validation + LLM call | Extract to mode, use execute_handler |
| `handle_preset_list` | 746 | Optional params handling | Move struct, use execute_handler |
| `handle_preset_run` | 773 | Complex preset execution | Keep inline (orchestration) |

## Solution Design

### Phase 1: Simple Handler Migration (6 handlers)

Handlers with local param structs that can directly use `execute_handler`:

1. **Move local param structs** to module-level with proper documentation
2. **Add mode methods** where needed to encapsulate logic
3. **Replace inline code** with `execute_handler` calls

#### 1.1 Tree Mode Auxiliary Handlers

**Current Pattern (handle_tree_focus):**
```rust
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
```

**Target Pattern:**
```rust
/// Parameters for tree focus operation
#[derive(Debug, Clone, Deserialize)]
pub struct TreeFocusParams {
    pub session_id: String,
    pub branch_id: String,
}

async fn handle_tree_focus(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.tree.focus",
        arguments,
        |params: TreeFocusParams| state.tree_mode.focus(params),
    )
    .await
}
```

**Required Mode Change:** Add `focus(params: TreeFocusParams)` method to TreeMode that wraps `focus_branch`.

#### 1.2 Handlers to Migrate

| Handler | New Param Struct | New Mode Method |
|---------|-----------------|-----------------|
| `handle_tree_focus` | `TreeFocusParams` | `TreeMode::focus(params)` |
| `handle_tree_list` | `TreeListParams` | `TreeMode::list(params)` |
| `handle_tree_complete` | `TreeCompleteParams` | `TreeMode::complete(params)` |
| `handle_reflection_evaluate` | `ReflectionEvaluateParams` | `ReflectionMode::evaluate(params)` |
| `handle_checkpoint_create` | `CheckpointCreateParams` | `BacktrackingMode::create(params)` |
| `handle_checkpoint_list` | `CheckpointListParams` | `BacktrackingMode::list(params)` |

### Phase 2: Detection Handler Migration (2 handlers)

These handlers have complex validation logic that should be moved to dedicated mode structs.

#### 2.1 Create Detection Mode

Create `src/modes/detection.rs` with:
- `DetectionMode` struct holding storage + langbase client
- `detect_biases(params: DetectBiasesParams)` method
- `detect_fallacies(params: DetectFallaciesParams)` method

**Benefits:**
- Centralizes detection logic
- Enables reuse from other parts of the system
- Consistent with other mode patterns
- Makes handlers trivial `execute_handler` wrappers

#### 2.2 Move Param Structs

Move `DetectBiasesParams` and `DetectFallaciesParams` to modes module alongside the mode implementation.

### Phase 3: Preset Handler Cleanup (2 handlers)

#### 3.1 handle_preset_list

**Current:** Manual optional params handling
**Fix:** Use `#[serde(default)]` for optional category, use execute_handler

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PresetListParams {
    #[serde(default)]
    pub category: Option<String>,
}

async fn handle_preset_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning.preset.list",
        arguments,
        |params: PresetListParams| async move {
            let presets = state.preset_registry.list(params.category.as_deref());
            let categories = state.preset_registry.categories();
            Ok::<_, std::convert::Infallible>(PresetListResponse {
                presets,
                count: presets.len(),
                categories,
            })
        },
    )
    .await
}
```

#### 3.2 handle_preset_run

**Decision:** Keep inline implementation.

**Rationale:**
- This is an orchestration handler that calls other handlers recursively
- The `Box::pin` for async recursion is specific to this handler
- Moving to a mode would just shift complexity without benefit

## Implementation Plan

### Step 1: Add Param Structs (handlers.rs)

Add module-level param structs:
```rust
// Tree auxiliary params
#[derive(Debug, Clone, Deserialize)]
pub struct TreeFocusParams {
    pub session_id: String,
    pub branch_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeListParams {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeCompleteParams {
    pub branch_id: String,
    #[serde(default = "default_completed")]
    pub completed: bool,
}

fn default_completed() -> bool {
    true
}

// Reflection auxiliary params
#[derive(Debug, Clone, Deserialize)]
pub struct ReflectionEvaluateParams {
    pub session_id: String,
}

// Checkpoint params
#[derive(Debug, Clone, Deserialize)]
pub struct CheckpointCreateParams {
    pub session_id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckpointListParams {
    pub session_id: String,
}
```

### Step 2: Add Mode Methods

#### tree.rs
```rust
impl TreeMode {
    pub async fn focus(&self, params: TreeFocusParams) -> AppResult<FocusResult> {
        self.focus_branch(&params.session_id, &params.branch_id).await
    }

    pub async fn list(&self, params: TreeListParams) -> AppResult<ListResult> {
        self.list_branches(&params.session_id).await
    }

    pub async fn complete(&self, params: TreeCompleteParams) -> AppResult<CompleteResult> {
        let state = if params.completed {
            BranchState::Completed
        } else {
            BranchState::Abandoned
        };
        self.update_branch_state(&params.branch_id, state).await
    }
}
```

#### reflection.rs
```rust
impl ReflectionMode {
    pub async fn evaluate(&self, params: ReflectionEvaluateParams) -> AppResult<SessionEvaluation> {
        self.evaluate_session(&params.session_id).await
    }
}
```

#### backtracking.rs
```rust
impl BacktrackingMode {
    pub async fn create_checkpoint_from_params(&self, params: CheckpointCreateParams) -> AppResult<Checkpoint> {
        self.create_checkpoint(&params.session_id, &params.name, params.description.as_deref()).await
    }

    pub async fn list_checkpoints_from_params(&self, params: CheckpointListParams) -> AppResult<Vec<Checkpoint>> {
        self.list_checkpoints(&params.session_id).await
    }
}
```

### Step 3: Create Detection Mode

Create `src/modes/detection.rs`:
```rust
pub struct DetectionMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    config: Config,
}

impl DetectionMode {
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: Config) -> Self {
        Self { storage, langbase, config }
    }

    pub async fn detect_biases(&self, params: DetectBiasesParams) -> AppResult<DetectBiasesResult> {
        // Move logic from handle_detect_biases
    }

    pub async fn detect_fallacies(&self, params: DetectFallaciesParams) -> AppResult<DetectFallaciesResult> {
        // Move logic from handle_detect_fallacies
    }
}
```

### Step 4: Update Handlers

Replace all inline handlers with `execute_handler` calls:
```rust
async fn handle_tree_focus(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler("reasoning.tree.focus", arguments, |params: TreeFocusParams| {
        state.tree_mode.focus(params)
    })
    .await
}
```

### Step 5: Fix handle_preset_list

Add serde default handling for optional args:
```rust
async fn handle_preset_list(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    // Use Default for no-args case
    let params: PresetListParams = arguments
        .map(|args| serde_json::from_value(args))
        .transpose()
        .map_err(|e| McpError::InvalidParameters {
            tool_name: "reasoning.preset.list".to_string(),
            message: e.to_string(),
        })?
        .unwrap_or_default();

    // ... rest of logic
}
```

Or create a variant of `execute_handler` that handles optional args with defaults.

## Migration Checklist

### Phase 1: Simple Handlers
- [ ] Add `TreeFocusParams`, `TreeListParams`, `TreeCompleteParams` structs
- [ ] Add `TreeMode::focus()`, `list()`, `complete()` methods
- [ ] Migrate `handle_tree_focus` to use execute_handler
- [ ] Migrate `handle_tree_list` to use execute_handler
- [ ] Migrate `handle_tree_complete` to use execute_handler
- [ ] Add `ReflectionEvaluateParams` struct
- [ ] Add `ReflectionMode::evaluate()` method
- [ ] Migrate `handle_reflection_evaluate` to use execute_handler
- [ ] Add `CheckpointCreateParams`, `CheckpointListParams` structs
- [ ] Add `BacktrackingMode::create_checkpoint_from_params()`, `list_checkpoints_from_params()` methods
- [ ] Migrate `handle_checkpoint_create` to use execute_handler
- [ ] Migrate `handle_checkpoint_list` to use execute_handler

### Phase 2: Detection Handlers
- [ ] Create `src/modes/detection.rs`
- [ ] Move `DetectBiasesParams` to modes
- [ ] Move `DetectFallaciesParams` to modes
- [ ] Implement `DetectionMode::detect_biases()`
- [ ] Implement `DetectionMode::detect_fallacies()`
- [ ] Add `DetectionMode` to `SharedState`
- [ ] Migrate `handle_detect_biases` to use execute_handler
- [ ] Migrate `handle_detect_fallacies` to use execute_handler

### Phase 3: Preset Handlers
- [ ] Add `Default` impl for `PresetListParams`
- [ ] Migrate `handle_preset_list` to cleaner pattern
- [ ] Keep `handle_preset_run` as-is (documented exception)

### Verification
- [ ] Run `cargo clippy -- -D warnings`
- [ ] Run `cargo test`
- [ ] Verify all 29 handlers route correctly

## Expected Outcome

After migration:
- **27 handlers** using `execute_handler` pattern
- **1 handler** (`handle_preset_run`) with documented exception
- **1 handler** (`handle_preset_list`) using optional-args variant
- Consistent error handling across all handlers
- Reduced code duplication (~200 lines)
- Better separation of concerns (logic in modes, routing in handlers)

## Files Modified

| File | Changes |
|------|---------|
| `src/server/handlers.rs` | Add param structs, migrate handlers |
| `src/modes/tree.rs` | Add focus/list/complete methods |
| `src/modes/reflection.rs` | Add evaluate method |
| `src/modes/backtracking.rs` | Add checkpoint param methods |
| `src/modes/detection.rs` | **NEW** - Detection mode |
| `src/modes/mod.rs` | Export DetectionMode |
| `src/server/mod.rs` | Add DetectionMode to SharedState |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing API | Low | High | No API changes, only internal refactor |
| Regression in error handling | Low | Medium | Comprehensive test coverage |
| Performance impact | Very Low | Low | No additional async overhead |
