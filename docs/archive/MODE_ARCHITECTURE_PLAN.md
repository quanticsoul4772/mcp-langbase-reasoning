# Mode Architecture Deduplication Plan

## Problem Statement

All 10 mode structs share identical patterns with significant code duplication:
- `storage: SqliteStorage` field
- `langbase: LangbaseClient` field
- One or more `*_pipe: String` fields
- `fn new(storage, langbase, config)` constructor

This creates ~300 lines of duplicated boilerplate across mode files.

## Current State Analysis

### Mode Categories by Complexity

**Category A: Single-Pipe Modes (6 modes)**
| Mode | Struct Fields | Pipe Config Access |
|------|---------------|-------------------|
| `LinearMode` | storage, langbase, pipe_name | `config.pipes.linear.clone()` |
| `TreeMode` | storage, langbase, pipe_name | `config.pipes.tree.clone()` |
| `DivergentMode` | storage, langbase, pipe_name | `config.pipes.divergent.clone()` |
| `ReflectionMode` | storage, langbase, pipe_name | `config.pipes.reflection.clone()` |
| `BacktrackingMode` | storage, langbase, pipe_name | `config.pipes.backtracking...unwrap_or()` |
| `AutoMode` | storage, langbase, pipe_name | `config.pipes.auto...unwrap_or()` |

**Category B: Dual-Pipe Modes (3 modes)**
| Mode | Struct Fields | Pipe Config Access |
|------|---------------|-------------------|
| `DetectionMode` | storage, langbase, bias_pipe, fallacy_pipe | `config.pipes.detection.as_ref()...` |
| `DecisionMode` | storage, langbase, decision_pipe, perspective_pipe | `config.pipes.decision.as_ref()...` |
| `EvidenceMode` | storage, langbase, evidence_pipe, bayesian_pipe | `config.pipes.evidence.as_ref()...` |

**Category C: Multi-Pipe + Config Mode (1 mode)**
| Mode | Struct Fields | Pipe Config Access |
|------|---------------|-------------------|
| `GotMode` | storage, langbase, generate_pipe, score_pipe, aggregate_pipe, refine_pipe, config | Complex nested config |

### Code Duplication Analysis

```rust
// Pattern repeated in 6+ modes (Category A):
#[derive(Clone)]
pub struct XxxMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}

impl XxxMode {
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            pipe_name: config.pipes.xxx.clone(), // or .unwrap_or_else(...)
        }
    }
    // Mode-specific methods follow...
}
```

**Duplicated Lines Per Mode**: ~15-25 lines
**Total Duplication**: ~150-250 lines across 10 modes

## Solution Design

### Option 1: ModeCore Composition (Recommended)

Extract the common fields into a shared struct that modes contain:

```rust
// src/modes/core.rs
use crate::langbase::LangbaseClient;
use crate::storage::SqliteStorage;

/// Core infrastructure shared by all reasoning modes.
///
/// Contains the storage backend and Langbase client needed for
/// persisting data and calling LLM pipes.
#[derive(Clone)]
pub struct ModeCore {
    storage: SqliteStorage,
    langbase: LangbaseClient,
}

impl ModeCore {
    /// Create a new mode core with the given storage and langbase client.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        Self { storage, langbase }
    }

    /// Get a reference to the storage backend.
    #[inline]
    pub fn storage(&self) -> &SqliteStorage {
        &self.storage
    }

    /// Get a reference to the Langbase client.
    #[inline]
    pub fn langbase(&self) -> &LangbaseClient {
        &self.langbase
    }
}
```

**Usage in Single-Pipe Modes:**
```rust
#[derive(Clone)]
pub struct LinearMode {
    core: ModeCore,
    pipe_name: String,
}

impl LinearMode {
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            pipe_name: config.pipes.linear.clone(),
        }
    }

    // Access via core.storage() and core.langbase()
    pub async fn process(&self, params: LinearParams) -> AppResult<LinearResult> {
        let session = self.core.storage().get_or_create_session(&session_id).await?;
        let response = self.core.langbase().call_pipe(...).await?;
        // ...
    }
}
```

**Pros:**
- Simple composition, no trait complexity
- Explicit field access via methods
- Easy to understand and maintain
- No async trait issues

**Cons:**
- Still requires constructor boilerplate
- Minor indirection through methods

### Option 2: BaseMode Trait (Alternative)

Define a trait that provides access to common dependencies:

```rust
// src/modes/base.rs
use crate::langbase::LangbaseClient;
use crate::storage::SqliteStorage;

/// Base trait for all reasoning modes providing access to core infrastructure.
pub trait BaseMode {
    /// Get a reference to the storage backend.
    fn storage(&self) -> &SqliteStorage;

    /// Get a reference to the Langbase client.
    fn langbase(&self) -> &LangbaseClient;
}

/// Blanket implementation helper
#[derive(Clone)]
pub struct ModeInfra {
    pub storage: SqliteStorage,
    pub langbase: LangbaseClient,
}
```

**Usage:**
```rust
#[derive(Clone)]
pub struct LinearMode {
    infra: ModeInfra,
    pipe_name: String,
}

impl BaseMode for LinearMode {
    fn storage(&self) -> &SqliteStorage { &self.infra.storage }
    fn langbase(&self) -> &LangbaseClient { &self.infra.langbase }
}
```

**Pros:**
- Enables polymorphism if needed
- Can add shared behavior to trait

**Cons:**
- Trait implementation boilerplate for each mode
- Limited benefit without polymorphic use cases
- More complex than composition

### Option 3: Macro-Based Generation (Not Recommended)

Use declarative macros to generate mode structs:

```rust
define_mode!(
    LinearMode,
    single_pipe,
    pipe_name: config.pipes.linear.clone()
);
```

**Pros:**
- Maximum code reduction
- Consistent structure enforcement

**Cons:**
- Hard to debug and understand
- IDE support issues
- Rust macros have learning curve
- Hides what code actually does

## Recommendation: Option 1 (ModeCore Composition)

**Rationale:**
1. **Simplicity**: No trait boilerplate or macro complexity
2. **Explicit**: Clear what each mode contains
3. **Flexible**: Easy to customize per-mode without trait complications
4. **Maintainable**: Standard Rust composition pattern
5. **No Breaking Changes**: Internal refactor, public API unchanged

## Implementation Plan

### Phase 1: Create ModeCore (No Breaking Changes)

1. Create `src/modes/core.rs` with `ModeCore` struct
2. Export from `src/modes/mod.rs`
3. Add tests for `ModeCore`

**Estimated Changes**: +40 lines

### Phase 2: Migrate Single-Pipe Modes

Migrate in order (simplest first):
1. `LinearMode` - simplest, good test case
2. `TreeMode`
3. `DivergentMode`
4. `ReflectionMode`
5. `BacktrackingMode`
6. `AutoMode`

**Per-Mode Changes**:
- Replace `storage` and `langbase` fields with `core: ModeCore`
- Update constructor to create `ModeCore`
- Replace `self.storage` → `self.core.storage()`
- Replace `self.langbase` → `self.core.langbase()`

**Estimated Net Change Per Mode**: -5 to -10 lines

### Phase 3: Migrate Dual-Pipe Modes

1. `DetectionMode`
2. `DecisionMode`
3. `EvidenceMode`

Same pattern as Phase 2.

### Phase 4: Migrate GotMode

`GotMode` is the most complex with 4 pipes plus config. Same pattern applies:
- Replace storage/langbase fields with `core: ModeCore`
- Keep pipe fields and config as-is

### Phase 5: Cleanup and Documentation

1. Update rustdoc comments
2. Verify all tests pass
3. Update CLAUDE.md if needed

## Migration Checklist

### Phase 1: Core
- [ ] Create `src/modes/core.rs`
- [ ] Add `ModeCore` struct with `new()`, `storage()`, `langbase()` methods
- [ ] Update `src/modes/mod.rs` to include `mod core;` and `pub use core::*;`
- [ ] Add unit tests for `ModeCore`

### Phase 2: Single-Pipe Modes
- [ ] Migrate `LinearMode`
- [ ] Migrate `TreeMode`
- [ ] Migrate `DivergentMode`
- [ ] Migrate `ReflectionMode`
- [ ] Migrate `BacktrackingMode`
- [ ] Migrate `AutoMode`

### Phase 3: Dual-Pipe Modes
- [ ] Migrate `DetectionMode`
- [ ] Migrate `DecisionMode`
- [ ] Migrate `EvidenceMode`

### Phase 4: Complex Mode
- [ ] Migrate `GotMode`

### Phase 5: Verification
- [ ] Run `cargo clippy -- -D warnings`
- [ ] Run `cargo test`
- [ ] Verify all 584+ tests pass

## Expected Outcome

**Before:**
- ~250 lines of duplicated struct definitions and constructors
- Direct field access to `storage` and `langbase`
- Each mode independently defines same fields

**After:**
- `ModeCore` centralizes ~30 lines of shared infrastructure
- ~80-100 lines reduced across all modes
- Consistent accessor pattern via `core.storage()` and `core.langbase()`
- Single point for future infrastructure enhancements

## Files Modified

| File | Changes |
|------|---------|
| `src/modes/core.rs` | **NEW** - ModeCore struct |
| `src/modes/mod.rs` | Add module export |
| `src/modes/linear.rs` | Migrate to ModeCore |
| `src/modes/tree.rs` | Migrate to ModeCore |
| `src/modes/divergent.rs` | Migrate to ModeCore |
| `src/modes/reflection.rs` | Migrate to ModeCore |
| `src/modes/backtracking.rs` | Migrate to ModeCore |
| `src/modes/auto.rs` | Migrate to ModeCore |
| `src/modes/detection.rs` | Migrate to ModeCore |
| `src/modes/decision.rs` | Migrate to ModeCore |
| `src/modes/evidence.rs` | Migrate to ModeCore |
| `src/modes/got.rs` | Migrate to ModeCore |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking public API | None | N/A | Internal refactor only |
| Regression bugs | Low | Medium | Comprehensive test coverage (584+ tests) |
| Performance impact | Very Low | Low | Inline methods, same field access |
| Increased complexity | Low | Low | Composition is simpler than current duplication |

## Alternative Considered: No Change

**Argument**: The current duplication is only ~250 lines across 10 files.

**Counter-argument**:
1. Future modes would add more duplication
2. Bug fixes to infrastructure need 10 changes instead of 1
3. Inconsistent access patterns make code harder to understand
4. Technical debt accumulates

## Decision

Proceed with **Option 1: ModeCore Composition** as it provides the best balance of code reduction, simplicity, and maintainability without introducing complex abstractions.
