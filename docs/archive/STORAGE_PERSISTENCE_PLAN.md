# Storage Persistence Implementation Plan

**STATUS: COMPLETED** (2025-12-17)

All 17 CRUD methods implemented, all 4 modes integrated with persistence, all 584 tests passing.

---

## Problem Statement

The Decision Framework (Phase 1) and Evidence Assessment (Phase 2) features have complete mode implementations (`DecisionMode`, `EvidenceMode`) and database migrations (`20240105000001_decision_evidence.sql`), but **no Storage trait methods exist to persist data to the new tables**.

### Current State
- Migration creates 4 tables: `decisions`, `perspective_analyses`, `evidence_assessments`, `probability_updates`
- `DecisionMode` generates `DecisionResult` and `PerspectiveResult` but doesn't persist them
- `EvidenceMode` generates `EvidenceResult` and `ProbabilisticResult` but doesn't persist them
- Storage trait has ~45 methods for other entities but **zero methods for these 4 tables**

### Impact
- Analysis results are lost after response is sent
- No session continuity for decision/evidence workflows
- Cannot query historical analyses
- Breaks consistency with other reasoning modes that persist all data

---

## Implementation Scope

### New Storage Types (in `src/storage/mod.rs`)

```rust
// ============================================================================
// Decision Framework Storage Types
// ============================================================================

/// Stored decision analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub session_id: String,
    pub question: String,
    pub options: Vec<String>,                    // JSON array
    pub criteria: Option<Vec<StoredCriterion>>,  // JSON array
    pub method: String,                          // 'weighted_sum', 'pairwise', 'topsis'
    pub recommendation: serde_json::Value,       // JSON object
    pub scores: serde_json::Value,               // JSON array
    pub sensitivity_analysis: Option<serde_json::Value>,
    pub trade_offs: Option<serde_json::Value>,
    pub constraints_satisfied: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCriterion {
    pub name: String,
    pub weight: f64,
    pub description: Option<String>,
}

/// Stored perspective analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveAnalysis {
    pub id: String,
    pub session_id: String,
    pub topic: String,
    pub stakeholders: serde_json::Value,        // JSON array
    pub power_matrix: Option<serde_json::Value>,
    pub conflicts: Option<serde_json::Value>,
    pub alignments: Option<serde_json::Value>,
    pub synthesis: serde_json::Value,           // JSON object
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

// ============================================================================
// Evidence Assessment Storage Types
// ============================================================================

/// Stored evidence assessment result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAssessment {
    pub id: String,
    pub session_id: String,
    pub claim: String,
    pub evidence: serde_json::Value,            // JSON array
    pub overall_support: serde_json::Value,     // JSON object
    pub evidence_analysis: serde_json::Value,   // JSON array
    pub chain_analysis: Option<serde_json::Value>,
    pub contradictions: Option<serde_json::Value>,
    pub gaps: Option<serde_json::Value>,
    pub recommendations: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Stored probability update result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilityUpdate {
    pub id: String,
    pub session_id: String,
    pub hypothesis: String,
    pub prior: f64,
    pub posterior: f64,
    pub confidence_lower: Option<f64>,
    pub confidence_upper: Option<f64>,
    pub confidence_level: Option<f64>,
    pub update_steps: serde_json::Value,        // JSON array
    pub uncertainty_analysis: Option<serde_json::Value>,
    pub sensitivity: Option<serde_json::Value>,
    pub interpretation: serde_json::Value,      // JSON object
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}
```

### Builder Patterns (following existing `Detection` pattern)

```rust
impl Decision {
    pub fn new(
        session_id: impl Into<String>,
        question: impl Into<String>,
        options: Vec<String>,
        method: impl Into<String>,
        recommendation: serde_json::Value,
        scores: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            question: question.into(),
            options,
            criteria: None,
            method: method.into(),
            recommendation,
            scores,
            sensitivity_analysis: None,
            trade_offs: None,
            constraints_satisfied: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    pub fn with_criteria(mut self, criteria: Vec<StoredCriterion>) -> Self {
        self.criteria = Some(criteria);
        self
    }

    pub fn with_sensitivity(mut self, analysis: serde_json::Value) -> Self {
        self.sensitivity_analysis = Some(analysis);
        self
    }

    pub fn with_trade_offs(mut self, trade_offs: serde_json::Value) -> Self {
        self.trade_offs = Some(trade_offs);
        self
    }

    pub fn with_constraints(mut self, satisfied: serde_json::Value) -> Self {
        self.constraints_satisfied = Some(satisfied);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}
```

Similar builders for `PerspectiveAnalysis`, `EvidenceAssessment`, and `ProbabilityUpdate`.

---

### Storage Trait Extensions

Add to `Storage` trait in `src/storage/mod.rs`:

```rust
#[async_trait]
pub trait Storage: Send + Sync {
    // ... existing methods ...

    // ========================================================================
    // Decision operations
    // ========================================================================

    /// Create a new decision analysis.
    async fn create_decision(&self, decision: &Decision) -> StorageResult<()>;

    /// Get a decision by ID.
    async fn get_decision(&self, id: &str) -> StorageResult<Option<Decision>>;

    /// Get all decisions in a session.
    async fn get_session_decisions(&self, session_id: &str) -> StorageResult<Vec<Decision>>;

    /// Get decisions by method type.
    async fn get_decisions_by_method(&self, method: &str) -> StorageResult<Vec<Decision>>;

    /// Delete a decision by ID.
    async fn delete_decision(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Perspective analysis operations
    // ========================================================================

    /// Create a new perspective analysis.
    async fn create_perspective(&self, analysis: &PerspectiveAnalysis) -> StorageResult<()>;

    /// Get a perspective analysis by ID.
    async fn get_perspective(&self, id: &str) -> StorageResult<Option<PerspectiveAnalysis>>;

    /// Get all perspective analyses in a session.
    async fn get_session_perspectives(&self, session_id: &str) -> StorageResult<Vec<PerspectiveAnalysis>>;

    /// Delete a perspective analysis by ID.
    async fn delete_perspective(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Evidence assessment operations
    // ========================================================================

    /// Create a new evidence assessment.
    async fn create_evidence_assessment(&self, assessment: &EvidenceAssessment) -> StorageResult<()>;

    /// Get an evidence assessment by ID.
    async fn get_evidence_assessment(&self, id: &str) -> StorageResult<Option<EvidenceAssessment>>;

    /// Get all evidence assessments in a session.
    async fn get_session_evidence_assessments(&self, session_id: &str) -> StorageResult<Vec<EvidenceAssessment>>;

    /// Delete an evidence assessment by ID.
    async fn delete_evidence_assessment(&self, id: &str) -> StorageResult<()>;

    // ========================================================================
    // Probability update operations
    // ========================================================================

    /// Create a new probability update.
    async fn create_probability_update(&self, update: &ProbabilityUpdate) -> StorageResult<()>;

    /// Get a probability update by ID.
    async fn get_probability_update(&self, id: &str) -> StorageResult<Option<ProbabilityUpdate>>;

    /// Get all probability updates in a session.
    async fn get_session_probability_updates(&self, session_id: &str) -> StorageResult<Vec<ProbabilityUpdate>>;

    /// Get probability updates for a hypothesis.
    async fn get_hypothesis_updates(&self, session_id: &str, hypothesis: &str) -> StorageResult<Vec<ProbabilityUpdate>>;

    /// Delete a probability update by ID.
    async fn delete_probability_update(&self, id: &str) -> StorageResult<()>;
}
```

---

### SqliteStorage Implementations

Add to `src/storage/sqlite.rs`:

```rust
// ============================================================================
// Decision Storage Implementation
// ============================================================================

async fn create_decision(&self, decision: &Decision) -> StorageResult<()> {
    let options_json = serde_json::to_string(&decision.options)?;
    let criteria_json = decision.criteria.as_ref()
        .map(|c| serde_json::to_string(c))
        .transpose()?;

    sqlx::query(
        r#"
        INSERT INTO decisions (
            id, session_id, question, options, criteria, method,
            recommendation, scores, sensitivity_analysis, trade_offs,
            constraints_satisfied, created_at, metadata
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(&decision.id)
    .bind(&decision.session_id)
    .bind(&decision.question)
    .bind(&options_json)
    .bind(&criteria_json)
    .bind(&decision.method)
    .bind(decision.recommendation.to_string())
    .bind(decision.scores.to_string())
    .bind(decision.sensitivity_analysis.as_ref().map(|v| v.to_string()))
    .bind(decision.trade_offs.as_ref().map(|v| v.to_string()))
    .bind(decision.constraints_satisfied.as_ref().map(|v| v.to_string()))
    .bind(decision.created_at.to_rfc3339())
    .bind(decision.metadata.as_ref().map(|v| v.to_string()))
    .execute(&self.pool)
    .await?;

    Ok(())
}

async fn get_decision(&self, id: &str) -> StorageResult<Option<Decision>> {
    let row = sqlx::query(
        "SELECT * FROM decisions WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(&self.pool)
    .await?;

    row.map(|r| Self::row_to_decision(r)).transpose()
}

async fn get_session_decisions(&self, session_id: &str) -> StorageResult<Vec<Decision>> {
    let rows = sqlx::query(
        "SELECT * FROM decisions WHERE session_id = ? ORDER BY created_at DESC"
    )
    .bind(session_id)
    .fetch_all(&self.pool)
    .await?;

    rows.into_iter().map(|r| Self::row_to_decision(r)).collect()
}

async fn get_decisions_by_method(&self, method: &str) -> StorageResult<Vec<Decision>> {
    let rows = sqlx::query(
        "SELECT * FROM decisions WHERE method = ? ORDER BY created_at DESC"
    )
    .bind(method)
    .fetch_all(&self.pool)
    .await?;

    rows.into_iter().map(|r| Self::row_to_decision(r)).collect()
}

async fn delete_decision(&self, id: &str) -> StorageResult<()> {
    sqlx::query("DELETE FROM decisions WHERE id = ?")
        .bind(id)
        .execute(&self.pool)
        .await?;
    Ok(())
}

// Helper to convert row to Decision
fn row_to_decision(row: SqliteRow) -> StorageResult<Decision> {
    use sqlx::Row;

    let options_str: String = row.get("options");
    let options: Vec<String> = serde_json::from_str(&options_str)?;

    let criteria: Option<Vec<StoredCriterion>> = row.get::<Option<String>, _>("criteria")
        .map(|s| serde_json::from_str(&s))
        .transpose()?;

    Ok(Decision {
        id: row.get("id"),
        session_id: row.get("session_id"),
        question: row.get("question"),
        options,
        criteria,
        method: row.get("method"),
        recommendation: serde_json::from_str(row.get("recommendation"))?,
        scores: serde_json::from_str(row.get("scores"))?,
        sensitivity_analysis: row.get::<Option<String>, _>("sensitivity_analysis")
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        trade_offs: row.get::<Option<String>, _>("trade_offs")
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        constraints_satisfied: row.get::<Option<String>, _>("constraints_satisfied")
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        created_at: DateTime::parse_from_rfc3339(row.get("created_at"))?.with_timezone(&Utc),
        metadata: row.get::<Option<String>, _>("metadata")
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
    })
}
```

Similar implementations for:
- `PerspectiveAnalysis` (create, get, get_session, delete)
- `EvidenceAssessment` (create, get, get_session, delete)
- `ProbabilityUpdate` (create, get, get_session, get_hypothesis, delete)

---

### Mode Integration

#### DecisionMode (`src/modes/decision.rs`)

Add persistence after building result:

```rust
// In make_decision(), after building DecisionResult:
let stored = Decision::new(
    &session.id,
    &params.question,
    params.options.clone(),
    params.method.to_string(),
    serde_json::to_value(&result.recommendation)?,
    serde_json::to_value(&result.scores)?,
)
.with_sensitivity(serde_json::to_value(&result.sensitivity_analysis)?)
.with_trade_offs(serde_json::to_value(&result.trade_offs)?)
.with_constraints(serde_json::to_value(&result.constraints_satisfied)?);

self.storage.create_decision(&stored).await?;

// In analyze_perspectives(), after building PerspectiveResult:
let stored = PerspectiveAnalysis::new(
    &session.id,
    &params.topic,
    serde_json::to_value(&result.stakeholders)?,
    serde_json::to_value(&result.synthesis)?,
    result.confidence,
)
.with_power_matrix(result.power_matrix.as_ref().map(|pm| serde_json::to_value(pm)).transpose()?)
.with_conflicts(serde_json::to_value(&result.conflicts)?)
.with_alignments(serde_json::to_value(&result.alignments)?);

self.storage.create_perspective(&stored).await?;
```

#### EvidenceMode (`src/modes/evidence.rs`)

Add persistence after building result:

```rust
// In assess_evidence(), after building EvidenceResult:
let stored = EvidenceAssessment::new(
    &session.id,
    &params.claim,
    serde_json::to_value(&params.evidence)?,
    serde_json::to_value(&result.overall_support)?,
    serde_json::to_value(&result.evidence_analyses)?,
)
.with_chain_analysis(result.chain_analysis.as_ref().map(|ca| serde_json::to_value(ca)).transpose()?)
.with_contradictions(serde_json::to_value(&result.contradictions)?)
.with_gaps(serde_json::to_value(&result.gaps)?)
.with_recommendations(serde_json::to_value(&result.recommendations)?);

self.storage.create_evidence_assessment(&stored).await?;

// In update_probability(), after building ProbabilisticResult:
let stored = ProbabilityUpdate::new(
    &session.id,
    &params.hypothesis,
    result.prior,
    result.posterior,
    serde_json::to_value(&result.update_steps)?,
    serde_json::to_value(&result.interpretation)?,
)
.with_confidence_interval(
    result.confidence_interval.as_ref().map(|ci| ci.lower),
    result.confidence_interval.as_ref().map(|ci| ci.upper),
    result.confidence_interval.as_ref().map(|ci| ci.level),
)
.with_uncertainty(serde_json::to_value(&result.uncertainty)?);

self.storage.create_probability_update(&stored).await?;
```

---

## Implementation Steps

### Step 1: Add Storage Types (~50 lines)
1. Add `Decision`, `StoredCriterion`, `PerspectiveAnalysis`, `EvidenceAssessment`, `ProbabilityUpdate` structs
2. Add builder methods for each type
3. Add tests for serialization/deserialization

### Step 2: Extend Storage Trait (~25 lines)
1. Add 17 new trait methods for CRUD operations
2. Document each method with doc comments

### Step 3: Implement SqliteStorage (~200 lines)
1. Implement all 17 methods in `sqlite.rs`
2. Add helper `row_to_*` conversion functions
3. Handle JSON serialization/deserialization properly

### Step 4: Integrate with Modes (~40 lines)
1. Add persistence calls in `DecisionMode::make_decision()`
2. Add persistence calls in `DecisionMode::analyze_perspectives()`
3. Add persistence calls in `EvidenceMode::assess_evidence()`
4. Add persistence calls in `EvidenceMode::update_probability()`

### Step 5: Add Tests (~150 lines)
1. Unit tests for storage types and builders
2. Integration tests for CRUD operations
3. End-to-end tests verifying persistence in mode handlers

---

## Estimated Effort

| Step | Lines of Code | Complexity |
|------|---------------|------------|
| Storage Types | ~150 | Low |
| Storage Trait | ~50 | Low |
| SqliteStorage | ~300 | Medium |
| Mode Integration | ~60 | Low |
| Tests | ~200 | Medium |
| **Total** | **~760** | **Medium** |

---

## Success Criteria

1. All 4 tables receive data when their respective mode handlers execute
2. Historical analyses can be retrieved via `get_session_*` methods
3. Cascade delete works correctly when sessions are deleted
4. All existing tests continue to pass
5. New tests cover CRUD operations and mode integration
6. No regressions in performance

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| JSON serialization failures | Use `serde_json::to_value()` with proper error handling |
| Schema mismatch | Validate against migration before implementing |
| Transaction failures | Log and return errors, don't swallow |
| Performance impact | Add indexes (already in migration), batch where possible |

---

## Files to Modify

1. `src/storage/mod.rs` - Add types and trait extensions
2. `src/storage/sqlite.rs` - Add SqliteStorage implementations
3. `src/modes/decision.rs` - Add persistence calls
4. `src/modes/evidence.rs` - Add persistence calls
5. `src/storage/types_tests.rs` - Add type tests (new file or extend)
6. `tests/storage_decision_evidence.rs` - Integration tests (new file)
