# Implementation Plan: Decision Framework & Evidence Assessment

> **Note:** This plan was the original specification. The actual implementation uses a **consolidated pipe architecture** where all decision/evidence operations route through `decision-framework-v1` with dynamic prompts. The deprecated pipe names (`decision-maker-v1`, `perspective-analyzer-v1`, `evidence-assessor-v1`, `bayesian-updater-v1`) mentioned below were never created - instead, a single consolidated pipe handles all operations.

## Overview

This plan details the implementation of 4 new reasoning tools for the mcp-langbase-reasoning server:

| Tool | Category | Priority |
|------|----------|----------|
| `reasoning_make_decision` | Decision Framework | 0.78 |
| `reasoning_analyze_perspectives` | Decision Framework | 0.78 |
| `reasoning_assess_evidence` | Evidence Assessment | 0.73 |
| `reasoning_probabilistic` | Evidence Assessment | 0.73 |

**Total Estimated Effort:** 5-7 days

---

## Phase 1: Decision Framework (2-3 days)

### 1.1 Create `src/modes/decision.rs`

#### Data Structures

```rust
//! Decision framework reasoning mode - structured multi-criteria decision making.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::{DECISION_MAKER_PROMPT, PERSPECTIVE_ANALYZER_PROMPT};
use crate::storage::{Invocation, Session, SqliteStorage, Storage, Thought};

// ============================================================================
// Parameters
// ============================================================================

/// Input parameters for multi-criteria decision making.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionParams {
    /// The decision question to analyze.
    pub question: String,
    /// Available options/alternatives (2-6 options).
    pub options: Vec<String>,
    /// Evaluation criteria with weights (should sum to 1.0).
    #[serde(default)]
    pub criteria: Vec<Criterion>,
    /// Hard constraints that must be satisfied.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Optional session ID (creates new if not provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Decision method to use.
    #[serde(default)]
    pub method: DecisionMethod,
}

/// A single criterion for decision evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    /// Criterion name.
    pub name: String,
    /// Weight (0.0-1.0).
    pub weight: f64,
    /// Description of the criterion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Decision analysis method.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMethod {
    #[default]
    WeightedSum,
    Pairwise,
    Topsis,
}

/// Input parameters for stakeholder perspective analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveParams {
    /// The topic or decision to analyze from multiple perspectives.
    pub topic: String,
    /// Stakeholders to consider (optional - will infer if not provided).
    #[serde(default)]
    pub stakeholders: Vec<StakeholderInput>,
    /// Additional context about the situation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Optional session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Include power/interest analysis.
    #[serde(default = "default_true")]
    pub include_power_matrix: bool,
}

fn default_true() -> bool {
    true
}

/// Stakeholder input for perspective analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeholderInput {
    /// Stakeholder name.
    pub name: String,
    /// Stakeholder role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Known interests.
    #[serde(default)]
    pub interests: Vec<String>,
}

// ============================================================================
// Responses
// ============================================================================

/// Result of multi-criteria decision analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResult {
    /// Unique decision ID.
    pub decision_id: String,
    /// Session ID.
    pub session_id: String,
    /// The question analyzed.
    pub question: String,
    /// The recommendation.
    pub recommendation: Recommendation,
    /// Scores for all options.
    pub scores: Vec<OptionScore>,
    /// Sensitivity analysis results.
    pub sensitivity_analysis: SensitivityAnalysis,
    /// Trade-offs between top options.
    pub trade_offs: Vec<TradeOff>,
    /// Constraint satisfaction per option.
    pub constraints_satisfied: HashMap<String, bool>,
}

/// The recommended option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// The recommended option.
    pub option: String,
    /// Overall score.
    pub score: f64,
    /// Confidence in the recommendation.
    pub confidence: f64,
    /// Rationale for the recommendation.
    pub rationale: String,
}

/// Score breakdown for an option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionScore {
    /// The option.
    pub option: String,
    /// Total weighted score.
    pub total_score: f64,
    /// Score breakdown by criterion.
    pub criteria_scores: HashMap<String, CriterionScore>,
    /// Rank (1 = best).
    pub rank: usize,
}

/// Score for a single criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionScore {
    /// Raw score (0.0-1.0).
    pub score: f64,
    /// Reasoning for the score.
    pub reasoning: String,
}

/// Sensitivity analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityAnalysis {
    /// Whether the recommendation is robust.
    pub robust: bool,
    /// Criteria that most affect the outcome.
    pub critical_criteria: Vec<String>,
    /// How much each criterion weight can change before ranking changes.
    pub threshold_changes: HashMap<String, f64>,
}

/// Trade-off between options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOff {
    /// Options being compared.
    pub between: (String, String),
    /// Description of the trade-off.
    pub trade_off: String,
}

/// Result of stakeholder perspective analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveResult {
    /// Unique analysis ID.
    pub analysis_id: String,
    /// Session ID.
    pub session_id: String,
    /// The topic analyzed.
    pub topic: String,
    /// Analyzed stakeholders.
    pub stakeholders: Vec<StakeholderAnalysis>,
    /// Power/interest matrix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_matrix: Option<PowerMatrix>,
    /// Identified conflicts.
    pub conflicts: Vec<Conflict>,
    /// Identified alignments.
    pub alignments: Vec<Alignment>,
    /// Synthesis of perspectives.
    pub synthesis: Synthesis,
    /// Overall confidence.
    pub confidence: f64,
}

/// Analysis of a single stakeholder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeholderAnalysis {
    /// Stakeholder name.
    pub name: String,
    /// Role.
    pub role: String,
    /// Their perspective on the topic.
    pub perspective: String,
    /// Their interests.
    pub interests: Vec<String>,
    /// Their concerns.
    pub concerns: Vec<String>,
    /// Power level (0.0-1.0).
    pub power_level: f64,
    /// Interest level (0.0-1.0).
    pub interest_level: f64,
    /// Quadrant in power/interest matrix.
    pub quadrant: Quadrant,
    /// Recommended engagement strategy.
    pub engagement_strategy: String,
}

/// Quadrant in power/interest matrix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quadrant {
    KeyPlayer,      // High power, high interest
    KeepSatisfied,  // High power, low interest
    KeepInformed,   // Low power, high interest
    MinimalEffort,  // Low power, low interest
}

/// Power/interest matrix breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMatrix {
    /// Key players (high power, high interest).
    pub key_players: Vec<String>,
    /// Keep satisfied (high power, low interest).
    pub keep_satisfied: Vec<String>,
    /// Keep informed (low power, high interest).
    pub keep_informed: Vec<String>,
    /// Minimal effort (low power, low interest).
    pub minimal_effort: Vec<String>,
}

/// Conflict between stakeholders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    /// Stakeholders in conflict.
    pub stakeholders: (String, String),
    /// The issue causing conflict.
    pub issue: String,
    /// Severity (0.0-1.0).
    pub severity: f64,
    /// Suggested resolution approach.
    pub resolution_approach: String,
}

/// Alignment between stakeholders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alignment {
    /// Aligned stakeholders.
    pub stakeholders: (String, String),
    /// Shared interest.
    pub shared_interest: String,
}

/// Synthesis of stakeholder perspectives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synthesis {
    /// Areas of consensus.
    pub consensus_areas: Vec<String>,
    /// Contentious areas.
    pub contentious_areas: Vec<String>,
    /// Overall recommendation.
    pub recommendation: String,
}

// ============================================================================
// Mode Handler
// ============================================================================

/// Decision framework mode handler.
#[derive(Clone)]
pub struct DecisionMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    decision_pipe: String,
    perspective_pipe: String,
}

impl DecisionMode {
    /// Create a new decision mode handler.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            decision_pipe: config.pipes.decision
                .as_ref()
                .and_then(|d| d.decision_pipe.clone())
                .unwrap_or_else(|| "decision-maker-v1".to_string()),
            perspective_pipe: config.pipes.decision
                .as_ref()
                .and_then(|d| d.perspective_pipe.clone())
                .unwrap_or_else(|| "perspective-analyzer-v1".to_string()),
        }
    }

    /// Process a multi-criteria decision request.
    pub async fn make_decision(&self, params: DecisionParams) -> AppResult<DecisionResult> {
        let start = Instant::now();

        // Validate input
        self.validate_decision_params(&params)?;

        // Get or create session
        let session = self.get_or_create_session(params.session_id.as_deref(), "decision").await?;

        // Build messages for Langbase
        let messages = self.build_decision_messages(&params);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.decision_pipe, messages);
        let response = self.langbase.call_pipe(request).await?;

        // Parse response
        let decision_response = self.parse_decision_response(&response.completion)?;

        // Generate decision ID
        let decision_id = uuid::Uuid::new_v4().to_string();

        // Persist to storage
        self.persist_decision(&session, &decision_id, &params, &decision_response).await?;

        // Log invocation
        self.log_invocation(&session, "make_decision", &params, &decision_response, start.elapsed().as_millis() as i64).await?;

        info!(
            decision_id = %decision_id,
            recommendation = %decision_response.recommendation.option,
            latency_ms = %start.elapsed().as_millis(),
            "Decision analysis completed"
        );

        Ok(decision_response)
    }

    /// Process a stakeholder perspective analysis request.
    pub async fn analyze_perspectives(&self, params: PerspectiveParams) -> AppResult<PerspectiveResult> {
        let start = Instant::now();

        // Validate input
        if params.topic.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "topic".to_string(),
                reason: "Topic cannot be empty".to_string(),
            }.into());
        }

        // Get or create session
        let session = self.get_or_create_session(params.session_id.as_deref(), "perspective").await?;

        // Build messages for Langbase
        let messages = self.build_perspective_messages(&params);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.perspective_pipe, messages);
        let response = self.langbase.call_pipe(request).await?;

        // Parse response
        let perspective_response = self.parse_perspective_response(&response.completion, &params)?;

        // Persist to storage
        self.persist_perspective(&session, &perspective_response).await?;

        // Log invocation
        self.log_invocation(&session, "analyze_perspectives", &params, &perspective_response, start.elapsed().as_millis() as i64).await?;

        info!(
            analysis_id = %perspective_response.analysis_id,
            stakeholder_count = %perspective_response.stakeholders.len(),
            latency_ms = %start.elapsed().as_millis(),
            "Perspective analysis completed"
        );

        Ok(perspective_response)
    }

    // ... private helper methods ...
}
```

### 1.2 Create System Prompts

Add to `src/prompts.rs`:

```rust
/// System prompt for multi-criteria decision making.
pub const DECISION_MAKER_PROMPT: &str = r#"You are a structured decision analysis assistant. Evaluate options using multi-criteria decision analysis.

Your response MUST be valid JSON in this format:
{
  "recommendation": {
    "option": "best option",
    "score": 0.85,
    "confidence": 0.82,
    "rationale": "why this is the best choice"
  },
  "scores": [
    {
      "option": "option name",
      "total_score": 0.85,
      "criteria_scores": {
        "criterion_name": {
          "score": 0.9,
          "reasoning": "justification"
        }
      },
      "rank": 1
    }
  ],
  "sensitivity_analysis": {
    "robust": true,
    "critical_criteria": ["most impactful criteria"],
    "threshold_changes": {"criterion": 0.15}
  },
  "trade_offs": [
    {
      "between": ["option_a", "option_b"],
      "trade_off": "description of trade-off"
    }
  ],
  "constraints_satisfied": {"option": true}
}

Guidelines:
- Score each option against each criterion (0.0-1.0)
- Apply weights to calculate total scores
- Identify trade-offs between top options
- Perform sensitivity analysis on weights
- Check constraint satisfaction
- Provide clear rationale

Always respond with valid JSON only."#;

/// System prompt for stakeholder perspective analysis.
pub const PERSPECTIVE_ANALYZER_PROMPT: &str = r#"You are a stakeholder analysis assistant. Analyze perspectives, power dynamics, and alignment.

Your response MUST be valid JSON in this format:
{
  "stakeholders": [
    {
      "name": "stakeholder name",
      "role": "their role",
      "perspective": "their viewpoint on the topic",
      "interests": ["what they care about"],
      "concerns": ["what worries them"],
      "power_level": 0.8,
      "interest_level": 0.9,
      "quadrant": "key_player",
      "engagement_strategy": "recommended approach"
    }
  ],
  "power_matrix": {
    "key_players": ["names"],
    "keep_satisfied": ["names"],
    "keep_informed": ["names"],
    "minimal_effort": ["names"]
  },
  "conflicts": [
    {
      "stakeholders": ["name1", "name2"],
      "issue": "what causes conflict",
      "severity": 0.7,
      "resolution_approach": "how to resolve"
    }
  ],
  "alignments": [
    {
      "stakeholders": ["name1", "name2"],
      "shared_interest": "common ground"
    }
  ],
  "synthesis": {
    "consensus_areas": ["where stakeholders agree"],
    "contentious_areas": ["where they disagree"],
    "recommendation": "overall strategic recommendation"
  },
  "confidence": 0.82
}

Guidelines:
- Infer stakeholders if not provided
- Assign power/interest levels objectively
- Categorize into power/interest quadrants
- Identify conflicts and their severity
- Find alignment opportunities
- Provide actionable engagement strategies

Always respond with valid JSON only."#;
```

### 1.3 Storage Schema Extension

Add to `src/storage/sqlite.rs` (schema initialization):

```sql
-- Decision analysis storage
CREATE TABLE IF NOT EXISTS decisions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    question TEXT NOT NULL,
    options TEXT NOT NULL,           -- JSON array
    criteria TEXT,                   -- JSON array
    method TEXT NOT NULL,
    recommendation TEXT NOT NULL,    -- JSON object
    scores TEXT NOT NULL,            -- JSON array
    sensitivity TEXT,                -- JSON object
    trade_offs TEXT,                 -- JSON array
    constraints_satisfied TEXT,      -- JSON object
    confidence REAL NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_decisions_session ON decisions(session_id);

-- Perspective analysis storage
CREATE TABLE IF NOT EXISTS perspective_analyses (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    topic TEXT NOT NULL,
    stakeholders TEXT NOT NULL,      -- JSON array
    power_matrix TEXT,               -- JSON object
    conflicts TEXT,                  -- JSON array
    alignments TEXT,                 -- JSON array
    synthesis TEXT NOT NULL,         -- JSON object
    confidence REAL NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_perspectives_session ON perspective_analyses(session_id);
```

### 1.4 MCP Tool Definitions

Add to `src/server/mcp.rs` in the `get_tool_definitions()` function:

```rust
Tool {
    name: "reasoning_make_decision".to_string(),
    description: "Structured multi-criteria decision making with weighted scoring, sensitivity analysis, and trade-off identification.".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "question": {
                "type": "string",
                "description": "The decision question to analyze"
            },
            "options": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Available options/alternatives (2-6 options)",
                "minItems": 2,
                "maxItems": 6
            },
            "criteria": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "weight": { "type": "number", "minimum": 0, "maximum": 1 },
                        "description": { "type": "string" }
                    },
                    "required": ["name", "weight"]
                },
                "description": "Evaluation criteria with weights (should sum to 1.0)"
            },
            "constraints": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Hard constraints that must be satisfied"
            },
            "session_id": {
                "type": "string",
                "description": "Optional session ID for context"
            },
            "method": {
                "type": "string",
                "enum": ["weighted_sum", "pairwise", "topsis"],
                "description": "Decision method to use (default: weighted_sum)"
            }
        },
        "required": ["question", "options"]
    }),
},

Tool {
    name: "reasoning_analyze_perspectives".to_string(),
    description: "Stakeholder analysis with power/interest mapping, conflict identification, and perspective synthesis.".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "topic": {
                "type": "string",
                "description": "The topic or decision to analyze from multiple perspectives"
            },
            "stakeholders": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "role": { "type": "string" },
                        "interests": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "required": ["name"]
                },
                "description": "Stakeholders to consider (optional - will infer if not provided)"
            },
            "context": {
                "type": "string",
                "description": "Additional context about the situation"
            },
            "session_id": {
                "type": "string",
                "description": "Optional session ID"
            },
            "include_power_matrix": {
                "type": "boolean",
                "description": "Include power/interest analysis (default: true)"
            }
        },
        "required": ["topic"]
    }),
},
```

### 1.5 Handler Implementation

Add to `src/server/handlers.rs`:

```rust
// In handle_tool_call match:
"reasoning_make_decision" => handle_make_decision(state, arguments).await,
"reasoning_analyze_perspectives" => handle_analyze_perspectives(state, arguments).await,

// Handler functions:
async fn handle_make_decision(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning_make_decision",
        arguments,
        |params: DecisionParams| state.decision_mode.make_decision(params),
    )
    .await
}

async fn handle_analyze_perspectives(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    execute_handler(
        "reasoning_analyze_perspectives",
        arguments,
        |params: PerspectiveParams| state.decision_mode.analyze_perspectives(params),
    )
    .await
}
```

### 1.6 Config Extension

Add to `src/config/mod.rs`:

```rust
/// Decision framework pipe configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionPipes {
    /// Pipe name for decision analysis.
    pub decision_pipe: Option<String>,
    /// Pipe name for perspective analysis.
    pub perspective_pipe: Option<String>,
}

// In PipesConfig struct:
pub decision: Option<DecisionPipes>,
```

### 1.7 Module Integration

Update `src/modes/mod.rs`:

```rust
mod decision;

pub use decision::*;

// Add to ReasoningMode enum:
Decision,
Perspective,
```

Update `src/server/mod.rs` (SharedState):

```rust
pub decision_mode: DecisionMode,

// In new() function:
decision_mode: DecisionMode::new(storage.clone(), langbase.clone(), &config),
```

---

## Phase 2: Evidence Assessment (2-3 days)

### 2.1 Create `src/modes/evidence.rs`

#### Data Structures

```rust
//! Evidence assessment reasoning mode - structured evidence evaluation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::{EVIDENCE_ASSESSOR_PROMPT, BAYESIAN_UPDATER_PROMPT};
use crate::storage::{Invocation, Session, SqliteStorage, Storage, Thought};

// ============================================================================
// Parameters
// ============================================================================

/// Input parameters for evidence assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceParams {
    /// The claim or hypothesis being evaluated.
    pub claim: String,
    /// Evidence items to evaluate.
    pub evidence: Vec<EvidenceItem>,
    /// Context for evaluation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Optional session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Analyze inferential chains.
    #[serde(default = "default_true")]
    pub include_chain_analysis: bool,
}

fn default_true() -> bool {
    true
}

/// A single piece of evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// The evidence content.
    pub content: String,
    /// Source of the evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Type of source.
    #[serde(default)]
    pub source_type: SourceType,
}

/// Type of evidence source.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    #[default]
    Unknown,
    Primary,
    Secondary,
    Expert,
    Anecdotal,
    Statistical,
}

/// Input parameters for probabilistic reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilisticParams {
    /// The hypothesis to evaluate.
    pub hypothesis: String,
    /// Prior probability before new evidence (default: 0.5).
    #[serde(default = "default_prior")]
    pub prior_probability: f64,
    /// New evidence to incorporate.
    #[serde(default)]
    pub new_evidence: Vec<NewEvidence>,
    /// Optional session ID for tracking updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Show calculation steps.
    #[serde(default = "default_true")]
    pub show_work: bool,
}

fn default_prior() -> f64 {
    0.5
}

/// New evidence for probability update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvidence {
    /// Description of the evidence.
    pub description: String,
    /// P(evidence|hypothesis_true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likelihood_if_true: Option<f64>,
    /// P(evidence|hypothesis_false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likelihood_if_false: Option<f64>,
}

// ============================================================================
// Responses
// ============================================================================

/// Result of evidence assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceResult {
    /// Unique assessment ID.
    pub assessment_id: String,
    /// Session ID.
    pub session_id: String,
    /// The claim evaluated.
    pub claim: String,
    /// Overall support level.
    pub overall_support: OverallSupport,
    /// Analysis of each evidence item.
    pub evidence_analysis: Vec<EvidenceAnalysis>,
    /// Chain analysis (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_analysis: Option<ChainAnalysis>,
    /// Contradictions found.
    pub contradictions: Vec<Contradiction>,
    /// Evidence gaps.
    pub gaps: Vec<Gap>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Overall support assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallSupport {
    /// Support level.
    pub level: SupportLevel,
    /// Confidence in assessment.
    pub confidence: f64,
    /// Explanation.
    pub explanation: String,
}

/// Level of support for a claim.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Strong,
    Moderate,
    Weak,
    Insufficient,
    Contradictory,
}

/// Analysis of a single evidence item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAnalysis {
    /// Evidence ID.
    pub evidence_id: String,
    /// Summary of the content.
    pub content_summary: String,
    /// Relevance assessment.
    pub relevance: Relevance,
    /// Credibility assessment.
    pub credibility: Credibility,
    /// Computed weight.
    pub weight: f64,
    /// Whether it supports the claim.
    pub supports_claim: bool,
    /// Inferential distance from claim.
    pub inferential_distance: u32,
}

/// Relevance assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relevance {
    /// Relevance score (0.0-1.0).
    pub score: f64,
    /// Type of relevance.
    pub relevance_type: RelevanceType,
    /// Explanation.
    pub explanation: String,
}

/// Type of relevance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelevanceType {
    Direct,
    Indirect,
    Tangential,
}

/// Credibility assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credibility {
    /// Overall credibility score.
    pub score: f64,
    /// Component factors.
    pub factors: CredibilityFactors,
    /// Concerns about credibility.
    pub concerns: Vec<String>,
}

/// Credibility factors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredibilityFactors {
    /// Source reliability.
    pub source_reliability: f64,
    /// Methodology quality.
    pub methodology: f64,
    /// Recency of evidence.
    pub recency: f64,
    /// Corroboration by other evidence.
    pub corroboration: f64,
}

/// Chain analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainAnalysis {
    /// Primary inferential chain.
    pub primary_chain: Vec<String>,
    /// Weak links in the chain.
    pub weak_links: Vec<WeakLink>,
    /// Redundant evidence items.
    pub redundancy: Vec<String>,
    /// Synergistic evidence combinations.
    pub synergies: Vec<Synergy>,
}

/// A weak link in the inferential chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeakLink {
    /// From element.
    pub from: String,
    /// To element.
    pub to: String,
    /// Description of weakness.
    pub weakness: String,
    /// Impact on conclusion.
    pub impact: f64,
}

/// Synergistic evidence combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synergy {
    /// Evidence IDs that combine.
    pub evidence_ids: Vec<String>,
    /// Combined strength.
    pub combined_strength: f64,
    /// Explanation.
    pub explanation: String,
}

/// Contradiction between evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// First evidence item.
    pub evidence_a: String,
    /// Second evidence item.
    pub evidence_b: String,
    /// Nature of contradiction.
    pub nature: String,
    /// Potential resolution.
    pub resolution: String,
}

/// Gap in evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    /// Description of the gap.
    pub gap: String,
    /// Importance of filling it.
    pub importance: f64,
    /// Suggested evidence to gather.
    pub suggested_evidence: String,
}

/// Result of probabilistic reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilisticResult {
    /// Unique update ID.
    pub update_id: String,
    /// Session ID.
    pub session_id: String,
    /// The hypothesis.
    pub hypothesis: String,
    /// Prior probability.
    pub prior: f64,
    /// Posterior probability.
    pub posterior: f64,
    /// Confidence interval.
    pub confidence_interval: ConfidenceInterval,
    /// Update steps.
    pub update_steps: Vec<UpdateStep>,
    /// Uncertainty analysis.
    pub uncertainty_analysis: UncertaintyAnalysis,
    /// Sensitivity analysis.
    pub sensitivity: Sensitivity,
    /// Human-readable interpretation.
    pub interpretation: Interpretation,
}

/// Confidence interval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    /// Lower bound.
    pub lower: f64,
    /// Upper bound.
    pub upper: f64,
    /// Confidence level.
    pub level: f64,
}

/// A single probability update step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStep {
    /// Evidence description.
    pub evidence: String,
    /// Prior before this update.
    pub prior_before: f64,
    /// Likelihood ratio.
    pub likelihood_ratio: f64,
    /// Posterior after this update.
    pub posterior_after: f64,
    /// Explanation.
    pub explanation: String,
}

/// Uncertainty analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyAnalysis {
    /// Entropy before updates.
    pub entropy_before: f64,
    /// Entropy after updates.
    pub entropy_after: f64,
    /// Information gained.
    pub information_gained: f64,
    /// Description of remaining uncertainty.
    pub remaining_uncertainty: String,
}

/// Sensitivity analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sensitivity {
    /// Most influential evidence.
    pub most_influential_evidence: String,
    /// Robustness score.
    pub robustness: f64,
    /// Critical assumptions.
    pub critical_assumptions: Vec<String>,
}

/// Human-readable interpretation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interpretation {
    /// Verbal probability.
    pub verbal_probability: String,
    /// Recommendation.
    pub recommendation: String,
    /// Caveats.
    pub caveats: Vec<String>,
}

// ============================================================================
// Mode Handler
// ============================================================================

/// Evidence assessment mode handler.
#[derive(Clone)]
pub struct EvidenceMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    evidence_pipe: String,
    bayesian_pipe: String,
}

impl EvidenceMode {
    /// Create a new evidence mode handler.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            evidence_pipe: config.pipes.evidence
                .as_ref()
                .and_then(|e| e.evidence_pipe.clone())
                .unwrap_or_else(|| "evidence-assessor-v1".to_string()),
            bayesian_pipe: config.pipes.evidence
                .as_ref()
                .and_then(|e| e.bayesian_pipe.clone())
                .unwrap_or_else(|| "bayesian-updater-v1".to_string()),
        }
    }

    /// Assess evidence for a claim.
    pub async fn assess_evidence(&self, params: EvidenceParams) -> AppResult<EvidenceResult> {
        // Implementation following established patterns...
    }

    /// Perform probabilistic reasoning.
    pub async fn probabilistic(&self, params: ProbabilisticParams) -> AppResult<ProbabilisticResult> {
        // Implementation following established patterns...
    }
}
```

### 2.2 System Prompts

Add to `src/prompts.rs`:

```rust
/// System prompt for evidence assessment.
pub const EVIDENCE_ASSESSOR_PROMPT: &str = r#"You are an evidence assessment assistant. Evaluate evidence for relevance, credibility, and support for claims.

Your response MUST be valid JSON in this format:
{
  "overall_support": {
    "level": "strong|moderate|weak|insufficient|contradictory",
    "confidence": 0.75,
    "explanation": "why this level"
  },
  "evidence_analysis": [
    {
      "evidence_id": "e1",
      "content_summary": "summary",
      "relevance": {
        "score": 0.85,
        "relevance_type": "direct|indirect|tangential",
        "explanation": "why relevant"
      },
      "credibility": {
        "score": 0.80,
        "factors": {
          "source_reliability": 0.9,
          "methodology": 0.7,
          "recency": 0.8,
          "corroboration": 0.75
        },
        "concerns": ["concerns"]
      },
      "weight": 0.68,
      "supports_claim": true,
      "inferential_distance": 2
    }
  ],
  "chain_analysis": {
    "primary_chain": ["e1", "inference", "claim"],
    "weak_links": [{"from": "a", "to": "b", "weakness": "desc", "impact": 0.3}],
    "redundancy": ["e2", "e3"],
    "synergies": [{"evidence_ids": ["e1", "e2"], "combined_strength": 0.9, "explanation": "why"}]
  },
  "contradictions": [
    {"evidence_a": "e1", "evidence_b": "e2", "nature": "desc", "resolution": "approach"}
  ],
  "gaps": [
    {"gap": "what's missing", "importance": 0.8, "suggested_evidence": "what to gather"}
  ],
  "recommendations": ["actionable recommendations"]
}

Guidelines:
- Evaluate each evidence item independently
- Consider source credibility factors
- Identify inferential chains
- Detect contradictions explicitly
- Note evidence gaps
- Provide actionable recommendations

Always respond with valid JSON only."#;

/// System prompt for Bayesian probability updates.
pub const BAYESIAN_UPDATER_PROMPT: &str = r#"You are a probabilistic reasoning assistant. Update beliefs using Bayesian inference.

Your response MUST be valid JSON in this format:
{
  "prior": 0.5,
  "posterior": 0.73,
  "confidence_interval": {"lower": 0.65, "upper": 0.81, "level": 0.95},
  "update_steps": [
    {
      "evidence": "description",
      "prior_before": 0.5,
      "likelihood_ratio": 2.5,
      "posterior_after": 0.71,
      "explanation": "how this evidence updates belief"
    }
  ],
  "uncertainty_analysis": {
    "entropy_before": 1.0,
    "entropy_after": 0.83,
    "information_gained": 0.17,
    "remaining_uncertainty": "what remains uncertain"
  },
  "sensitivity": {
    "most_influential_evidence": "which evidence matters most",
    "robustness": 0.8,
    "critical_assumptions": ["key assumptions"]
  },
  "interpretation": {
    "verbal_probability": "likely|unlikely|uncertain|etc",
    "recommendation": "what to do",
    "caveats": ["important caveats"]
  }
}

Guidelines:
- Apply Bayes' rule correctly
- Estimate likelihood ratios when not provided
- Calculate entropy and information gain
- Provide verbal probability interpretations
- Note critical assumptions

Always respond with valid JSON only."#;
```

### 2.3 Storage Schema

Add to schema:

```sql
-- Evidence assessment storage
CREATE TABLE IF NOT EXISTS evidence_assessments (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    claim TEXT NOT NULL,
    evidence TEXT NOT NULL,              -- JSON array
    overall_support TEXT NOT NULL,       -- JSON object
    evidence_analysis TEXT NOT NULL,     -- JSON array
    chain_analysis TEXT,                 -- JSON object
    contradictions TEXT,                 -- JSON array
    gaps TEXT,                           -- JSON array
    recommendations TEXT,                -- JSON array
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_session ON evidence_assessments(session_id);

-- Probability updates storage
CREATE TABLE IF NOT EXISTS probability_updates (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    hypothesis TEXT NOT NULL,
    prior REAL NOT NULL,
    posterior REAL NOT NULL,
    confidence_lower REAL,
    confidence_upper REAL,
    update_steps TEXT NOT NULL,          -- JSON array
    uncertainty_analysis TEXT,           -- JSON object
    sensitivity TEXT,                    -- JSON object
    interpretation TEXT NOT NULL,        -- JSON object
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_probability_session ON probability_updates(session_id);
```

### 2.4 MCP Tool Definitions

```rust
Tool {
    name: "reasoning_assess_evidence".to_string(),
    description: "Structured evidence evaluation with relevance scoring, credibility assessment, chain analysis, and contradiction detection.".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "claim": {
                "type": "string",
                "description": "The claim or hypothesis being evaluated"
            },
            "evidence": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" },
                        "source": { "type": "string" },
                        "source_type": {
                            "type": "string",
                            "enum": ["primary", "secondary", "expert", "anecdotal", "statistical"]
                        }
                    },
                    "required": ["content"]
                },
                "description": "Evidence items to evaluate"
            },
            "context": {
                "type": "string",
                "description": "Context for evaluation"
            },
            "session_id": {
                "type": "string",
                "description": "Optional session ID"
            },
            "include_chain_analysis": {
                "type": "boolean",
                "description": "Analyze inferential chains (default: true)"
            }
        },
        "required": ["claim", "evidence"]
    }),
},

Tool {
    name: "reasoning_probabilistic".to_string(),
    description: "Bayesian-style probability updates with uncertainty quantification, sensitivity analysis, and human-readable interpretations.".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "hypothesis": {
                "type": "string",
                "description": "The hypothesis to evaluate"
            },
            "prior_probability": {
                "type": "number",
                "minimum": 0,
                "maximum": 1,
                "description": "Prior probability before new evidence (default: 0.5)"
            },
            "new_evidence": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "description": { "type": "string" },
                        "likelihood_if_true": {
                            "type": "number",
                            "minimum": 0,
                            "maximum": 1,
                            "description": "P(evidence|hypothesis_true)"
                        },
                        "likelihood_if_false": {
                            "type": "number",
                            "minimum": 0,
                            "maximum": 1,
                            "description": "P(evidence|hypothesis_false)"
                        }
                    },
                    "required": ["description"]
                },
                "description": "New evidence to incorporate"
            },
            "session_id": {
                "type": "string",
                "description": "Optional session ID for tracking updates"
            },
            "show_work": {
                "type": "boolean",
                "description": "Show calculation steps (default: true)"
            }
        },
        "required": ["hypothesis"]
    }),
},
```

---

## Phase 3: Integration & Presets (1-2 days)

### 3.1 Add Workflow Presets

Add to `src/presets/builtins.rs`:

```rust
/// Strategic decision preset combining divergent thinking, perspective analysis, and decision making.
pub fn strategic_decision_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "strategic-decision".to_string(),
        name: "Strategic Decision".to_string(),
        description: "Comprehensive decision analysis with stakeholder perspectives, \
            multi-criteria evaluation, and bias detection".to_string(),
        category: "decision".to_string(),
        estimated_time: "4-6 minutes".to_string(),
        output_format: "decision_report".to_string(),
        tags: vec!["decision".to_string(), "strategy".to_string(), "stakeholder".to_string()],
        input_schema: HashMap::from([
            ("question".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: true,
                default: None,
                description: "The decision question to analyze".to_string(),
                examples: vec![json!("Should we expand into the European market?")],
            }),
            ("options".to_string(), ParamSpec {
                param_type: "array".to_string(),
                required: true,
                default: None,
                description: "Available options to evaluate".to_string(),
                examples: vec![json!(["Option A", "Option B", "Option C"])],
            }),
        ]),
        steps: vec![
            PresetStep {
                step_id: "generate_options".to_string(),
                tool: "reasoning_divergent".to_string(),
                description: "Generate additional options and perspectives".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("num_perspectives".to_string(), json!(3)),
                    ("challenge_assumptions".to_string(), json!(true)),
                ]),
                condition: None,
                store_as: Some("perspectives".to_string()),
                depends_on: vec![],
                optional: false,
            },
            PresetStep {
                step_id: "stakeholder_analysis".to_string(),
                tool: "reasoning_analyze_perspectives".to_string(),
                description: "Analyze stakeholder perspectives".to_string(),
                input_map: HashMap::from([
                    ("topic".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("include_power_matrix".to_string(), json!(true)),
                ]),
                condition: None,
                store_as: Some("stakeholders".to_string()),
                depends_on: vec!["generate_options".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "decision_analysis".to_string(),
                tool: "reasoning_make_decision".to_string(),
                description: "Multi-criteria decision analysis".to_string(),
                input_map: HashMap::from([
                    ("question".to_string(), "question".to_string()),
                    ("options".to_string(), "options".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("method".to_string(), json!("weighted_sum")),
                ]),
                condition: None,
                store_as: Some("decision".to_string()),
                depends_on: vec!["stakeholder_analysis".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "bias_check".to_string(),
                tool: "reasoning_detect_biases".to_string(),
                description: "Check for decision-making biases".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("biases".to_string()),
                depends_on: vec!["decision_analysis".to_string()],
                optional: true,
            },
            PresetStep {
                step_id: "synthesis".to_string(),
                tool: "reasoning_reflection".to_string(),
                description: "Synthesize findings into final recommendation".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "question".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("quality_threshold".to_string(), json!(0.75)),
                ]),
                condition: None,
                store_as: Some("final".to_string()),
                depends_on: vec![
                    "decision_analysis".to_string(),
                    "bias_check".to_string(),
                ],
                optional: false,
            },
        ],
    }
}

/// Evidence-based conclusion preset.
pub fn evidence_based_conclusion_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "evidence-based-conclusion".to_string(),
        name: "Evidence-Based Conclusion".to_string(),
        description: "Systematic evidence evaluation with probability updates and fallacy detection".to_string(),
        category: "research".to_string(),
        estimated_time: "3-5 minutes".to_string(),
        output_format: "evidence_report".to_string(),
        tags: vec!["evidence".to_string(), "research".to_string(), "analysis".to_string()],
        input_schema: HashMap::from([
            ("claim".to_string(), ParamSpec {
                param_type: "string".to_string(),
                required: true,
                default: None,
                description: "The claim to evaluate".to_string(),
                examples: vec![json!("Product X improves customer satisfaction")],
            }),
            ("evidence".to_string(), ParamSpec {
                param_type: "array".to_string(),
                required: true,
                default: None,
                description: "Evidence items to evaluate".to_string(),
                examples: vec![],
            }),
        ]),
        steps: vec![
            PresetStep {
                step_id: "initial_analysis".to_string(),
                tool: "reasoning_linear".to_string(),
                description: "Initial claim analysis".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "claim".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("initial".to_string()),
                depends_on: vec![],
                optional: false,
            },
            PresetStep {
                step_id: "evidence_assessment".to_string(),
                tool: "reasoning_assess_evidence".to_string(),
                description: "Comprehensive evidence evaluation".to_string(),
                input_map: HashMap::from([
                    ("claim".to_string(), "claim".to_string()),
                    ("evidence".to_string(), "evidence".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("include_chain_analysis".to_string(), json!(true)),
                ]),
                condition: None,
                store_as: Some("assessment".to_string()),
                depends_on: vec!["initial_analysis".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "probability_update".to_string(),
                tool: "reasoning_probabilistic".to_string(),
                description: "Update probability based on evidence".to_string(),
                input_map: HashMap::from([
                    ("hypothesis".to_string(), "claim".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("show_work".to_string(), json!(true)),
                ]),
                condition: None,
                store_as: Some("probability".to_string()),
                depends_on: vec!["evidence_assessment".to_string()],
                optional: false,
            },
            PresetStep {
                step_id: "fallacy_check".to_string(),
                tool: "reasoning_detect_fallacies".to_string(),
                description: "Check for logical fallacies".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "claim".to_string()),
                ]),
                static_inputs: HashMap::new(),
                condition: None,
                store_as: Some("fallacies".to_string()),
                depends_on: vec!["probability_update".to_string()],
                optional: true,
            },
            PresetStep {
                step_id: "conclusion".to_string(),
                tool: "reasoning_reflection".to_string(),
                description: "Synthesize evidence-based conclusion".to_string(),
                input_map: HashMap::from([
                    ("content".to_string(), "claim".to_string()),
                ]),
                static_inputs: HashMap::from([
                    ("quality_threshold".to_string(), json!(0.8)),
                ]),
                condition: None,
                store_as: Some("conclusion".to_string()),
                depends_on: vec![
                    "evidence_assessment".to_string(),
                    "probability_update".to_string(),
                    "fallacy_check".to_string(),
                ],
                optional: false,
            },
        ],
    }
}
```

Register in `src/presets/registry.rs`:

```rust
fn register_builtins(&self) {
    let _ = self.register(builtins::code_review_preset());
    let _ = self.register(builtins::debug_analysis_preset());
    let _ = self.register(builtins::architecture_decision_preset());
    // NEW:
    let _ = self.register(builtins::strategic_decision_preset());
    let _ = self.register(builtins::evidence_based_conclusion_preset());
}
```

---

## Phase 4: Testing (1 day)

### 4.1 Unit Tests

Create `src/modes/decision_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_decision_params_deserialization() {
        let json = json!({
            "question": "Should we proceed?",
            "options": ["Yes", "No", "Delay"]
        });

        let params: DecisionParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.question, "Should we proceed?");
        assert_eq!(params.options.len(), 3);
        assert!(params.criteria.is_empty());
        assert!(matches!(params.method, DecisionMethod::WeightedSum));
    }

    #[test]
    fn test_decision_params_with_criteria() {
        let json = json!({
            "question": "Which vendor?",
            "options": ["Vendor A", "Vendor B"],
            "criteria": [
                {"name": "cost", "weight": 0.4},
                {"name": "quality", "weight": 0.6}
            ]
        });

        let params: DecisionParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.criteria.len(), 2);
        assert_eq!(params.criteria[0].weight, 0.4);
    }

    #[test]
    fn test_perspective_params_defaults() {
        let json = json!({
            "topic": "New policy implementation"
        });

        let params: PerspectiveParams = serde_json::from_value(json).unwrap();
        assert!(params.include_power_matrix);
        assert!(params.stakeholders.is_empty());
    }

    #[test]
    fn test_quadrant_serialization() {
        assert_eq!(
            serde_json::to_string(&Quadrant::KeyPlayer).unwrap(),
            "\"key_player\""
        );
    }
}
```

Create `src/modes/evidence_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_evidence_params_deserialization() {
        let json = json!({
            "claim": "X causes Y",
            "evidence": [
                {"content": "Study shows correlation"},
                {"content": "Expert testimony", "source_type": "expert"}
            ]
        });

        let params: EvidenceParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.claim, "X causes Y");
        assert_eq!(params.evidence.len(), 2);
        assert!(params.include_chain_analysis);
    }

    #[test]
    fn test_probabilistic_params_defaults() {
        let json = json!({
            "hypothesis": "Treatment is effective"
        });

        let params: ProbabilisticParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.prior_probability, 0.5);
        assert!(params.show_work);
    }

    #[test]
    fn test_support_level_serialization() {
        assert_eq!(
            serde_json::to_string(&SupportLevel::Strong).unwrap(),
            "\"strong\""
        );
        assert_eq!(
            serde_json::to_string(&SupportLevel::Contradictory).unwrap(),
            "\"contradictory\""
        );
    }
}
```

### 4.2 Integration Tests

Create `tests/decision_integration.rs`:

```rust
//! Integration tests for decision framework tools.

use mcp_langbase_reasoning::modes::{DecisionParams, PerspectiveParams};
use serde_json::json;

#[tokio::test]
async fn test_make_decision_mcp_call() {
    // Test MCP protocol flow
}

#[tokio::test]
async fn test_analyze_perspectives_mcp_call() {
    // Test MCP protocol flow
}

#[tokio::test]
async fn test_strategic_decision_preset() {
    // Test preset execution
}
```

---

## Success Criteria

### Decision Framework
- [ ] `reasoning_make_decision` returns ranked options with weighted scores
- [ ] `reasoning_make_decision` performs sensitivity analysis
- [ ] `reasoning_analyze_perspectives` identifies stakeholders (inferred if not provided)
- [ ] `reasoning_analyze_perspectives` generates power/interest matrix
- [ ] Conflicts and alignments are detected
- [ ] All tests pass

### Evidence Assessment
- [ ] `reasoning_assess_evidence` evaluates relevance and credibility
- [ ] `reasoning_assess_evidence` performs chain analysis
- [ ] `reasoning_assess_evidence` identifies contradictions and gaps
- [ ] `reasoning_probabilistic` correctly applies Bayes' rule
- [ ] `reasoning_probabilistic` provides uncertainty quantification
- [ ] Human-readable interpretations are generated
- [ ] All tests pass

### Integration
- [ ] All 4 tools accessible via MCP
- [ ] Tools work in workflow presets
- [ ] Results persist to SQLite
- [ ] Documentation updated (README.md, API_REFERENCE.md)
- [ ] `cargo test` passes (all tests)
- [ ] `cargo clippy` passes (no warnings)

---

## File Checklist

### New Files
- [ ] `src/modes/decision.rs`
- [ ] `src/modes/evidence.rs`
- [ ] `src/modes/decision_tests.rs`
- [ ] `src/modes/evidence_tests.rs`
- [ ] `tests/decision_integration.rs`
- [ ] `tests/evidence_integration.rs`

### Modified Files
- [ ] `src/modes/mod.rs` - Add exports
- [ ] `src/prompts.rs` - Add 4 prompts
- [ ] `src/config/mod.rs` - Add pipe config
- [ ] `src/storage/sqlite.rs` - Add tables
- [ ] `src/server/mod.rs` - Add to SharedState
- [ ] `src/server/handlers.rs` - Add handlers
- [ ] `src/server/mcp.rs` - Add tool definitions
- [ ] `src/presets/builtins.rs` - Add presets
- [ ] `src/presets/registry.rs` - Register presets
- [ ] `README.md` - Update documentation
- [ ] `docs/API_REFERENCE.md` - Add tool docs

---

## Langbase Pipe Requirements

### Overview

> **SUPERSEDED:** The original plan called for 4 separate pipes. The actual implementation uses a **single consolidated pipe** (`decision-framework-v1`) that handles all decision and evidence operations with dynamic prompts passed at runtime. The per-operation pipes below were never created.

~~Four new Langbase pipes must be created before the tools can function.~~ A single consolidated pipe handles all operations:

### Pipe Creation via API

Use the Langbase `POST /v1/pipes` endpoint with `upsert: true` to create/update pipes.

**Authentication:**
```bash
Authorization: Bearer {LANGBASE_API_KEY}
```

### Pipe 1: `decision-maker-v1`

Multi-criteria decision analysis with sensitivity testing.

```bash
curl -X POST "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "decision-maker-v1",
    "description": "Multi-criteria decision analysis with weighted scoring, sensitivity analysis, and trade-off identification",
    "model": "openai:gpt-4o",
    "upsert": true,
    "json": true,
    "stream": false,
    "store": true,
    "temperature": 0.6,
    "max_tokens": 4000,
    "messages": [
      {
        "role": "system",
        "content": "You are a structured decision analysis assistant. Evaluate options using multi-criteria decision analysis.\n\nYour response MUST be valid JSON in this format:\n{\n  \"recommendation\": {\n    \"option\": \"best option\",\n    \"score\": 0.85,\n    \"confidence\": 0.82,\n    \"rationale\": \"why this is the best choice\"\n  },\n  \"scores\": [\n    {\n      \"option\": \"option name\",\n      \"total_score\": 0.85,\n      \"criteria_scores\": {\n        \"criterion_name\": {\n          \"score\": 0.9,\n          \"reasoning\": \"justification\"\n        }\n      },\n      \"rank\": 1\n    }\n  ],\n  \"sensitivity_analysis\": {\n    \"robust\": true,\n    \"critical_criteria\": [\"most impactful criteria\"],\n    \"threshold_changes\": {\"criterion\": 0.15}\n  },\n  \"trade_offs\": [\n    {\n      \"between\": [\"option_a\", \"option_b\"],\n      \"trade_off\": \"description of trade-off\"\n    }\n  ],\n  \"constraints_satisfied\": {\"option\": true}\n}\n\nGuidelines:\n- Score each option against each criterion (0.0-1.0)\n- Apply weights to calculate total scores\n- Identify trade-offs between top options\n- Perform sensitivity analysis on weights\n- Check constraint satisfaction\n- Provide clear rationale\n\nAlways respond with valid JSON only."
      }
    ]
  }'
```

**Configuration:**
| Setting | Value | Rationale |
|---------|-------|-----------|
| `model` | `openai:gpt-4o` | Complex multi-step analysis needs strong reasoning |
| `temperature` | `0.6` | Consistent, analytical output |
| `max_tokens` | `4000` | Large structured responses with multiple options |
| `json` | `true` | Ensure valid JSON output |

---

### Pipe 2: `perspective-analyzer-v1`

Stakeholder analysis with power/interest mapping.

```bash
curl -X POST "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "perspective-analyzer-v1",
    "description": "Stakeholder analysis with power/interest mapping, conflict identification, and perspective synthesis",
    "model": "openai:gpt-4o",
    "upsert": true,
    "json": true,
    "stream": false,
    "store": true,
    "temperature": 0.7,
    "max_tokens": 4000,
    "messages": [
      {
        "role": "system",
        "content": "You are a stakeholder analysis assistant. Analyze perspectives, power dynamics, and alignment.\n\nYour response MUST be valid JSON in this format:\n{\n  \"stakeholders\": [\n    {\n      \"name\": \"stakeholder name\",\n      \"role\": \"their role\",\n      \"perspective\": \"their viewpoint on the topic\",\n      \"interests\": [\"what they care about\"],\n      \"concerns\": [\"what worries them\"],\n      \"power_level\": 0.8,\n      \"interest_level\": 0.9,\n      \"quadrant\": \"key_player\",\n      \"engagement_strategy\": \"recommended approach\"\n    }\n  ],\n  \"power_matrix\": {\n    \"key_players\": [\"names\"],\n    \"keep_satisfied\": [\"names\"],\n    \"keep_informed\": [\"names\"],\n    \"minimal_effort\": [\"names\"]\n  },\n  \"conflicts\": [\n    {\n      \"stakeholders\": [\"name1\", \"name2\"],\n      \"issue\": \"what causes conflict\",\n      \"severity\": 0.7,\n      \"resolution_approach\": \"how to resolve\"\n    }\n  ],\n  \"alignments\": [\n    {\n      \"stakeholders\": [\"name1\", \"name2\"],\n      \"shared_interest\": \"common ground\"\n    }\n  ],\n  \"synthesis\": {\n    \"consensus_areas\": [\"where stakeholders agree\"],\n    \"contentious_areas\": [\"where they disagree\"],\n    \"recommendation\": \"overall strategic recommendation\"\n  },\n  \"confidence\": 0.82\n}\n\nGuidelines:\n- Infer stakeholders if not provided\n- Assign power/interest levels objectively\n- Categorize into power/interest quadrants\n- Identify conflicts and their severity\n- Find alignment opportunities\n- Provide actionable engagement strategies\n\nAlways respond with valid JSON only."
      }
    ]
  }'
```

**Configuration:**
| Setting | Value | Rationale |
|---------|-------|-----------|
| `model` | `openai:gpt-4o` | Needs nuanced understanding of human dynamics |
| `temperature` | `0.7` | Balance between creativity and consistency |
| `max_tokens` | `4000` | Multiple stakeholders generate large responses |
| `json` | `true` | Ensure valid JSON output |

---

### Pipe 3: `evidence-assessor-v1`

Evidence evaluation with chain analysis.

```bash
curl -X POST "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "evidence-assessor-v1",
    "description": "Structured evidence evaluation with relevance scoring, credibility assessment, chain analysis, and contradiction detection",
    "model": "openai:gpt-4o",
    "upsert": true,
    "json": true,
    "stream": false,
    "store": true,
    "temperature": 0.5,
    "max_tokens": 5000,
    "messages": [
      {
        "role": "system",
        "content": "You are an evidence assessment assistant. Evaluate evidence for relevance, credibility, and support for claims.\n\nYour response MUST be valid JSON in this format:\n{\n  \"overall_support\": {\n    \"level\": \"strong|moderate|weak|insufficient|contradictory\",\n    \"confidence\": 0.75,\n    \"explanation\": \"why this level\"\n  },\n  \"evidence_analysis\": [\n    {\n      \"evidence_id\": \"e1\",\n      \"content_summary\": \"summary\",\n      \"relevance\": {\n        \"score\": 0.85,\n        \"relevance_type\": \"direct|indirect|tangential\",\n        \"explanation\": \"why relevant\"\n      },\n      \"credibility\": {\n        \"score\": 0.80,\n        \"factors\": {\n          \"source_reliability\": 0.9,\n          \"methodology\": 0.7,\n          \"recency\": 0.8,\n          \"corroboration\": 0.75\n        },\n        \"concerns\": [\"concerns\"]\n      },\n      \"weight\": 0.68,\n      \"supports_claim\": true,\n      \"inferential_distance\": 2\n    }\n  ],\n  \"chain_analysis\": {\n    \"primary_chain\": [\"e1\", \"inference\", \"claim\"],\n    \"weak_links\": [{\"from\": \"a\", \"to\": \"b\", \"weakness\": \"desc\", \"impact\": 0.3}],\n    \"redundancy\": [\"e2\", \"e3\"],\n    \"synergies\": [{\"evidence_ids\": [\"e1\", \"e2\"], \"combined_strength\": 0.9, \"explanation\": \"why\"}]\n  },\n  \"contradictions\": [\n    {\"evidence_a\": \"e1\", \"evidence_b\": \"e2\", \"nature\": \"desc\", \"resolution\": \"approach\"}\n  ],\n  \"gaps\": [\n    {\"gap\": \"what is missing\", \"importance\": 0.8, \"suggested_evidence\": \"what to gather\"}\n  ],\n  \"recommendations\": [\"actionable recommendations\"]\n}\n\nGuidelines:\n- Evaluate each evidence item independently\n- Consider source credibility factors\n- Identify inferential chains\n- Detect contradictions explicitly\n- Note evidence gaps\n- Provide actionable recommendations\n\nAlways respond with valid JSON only."
      }
    ]
  }'
```

**Configuration:**
| Setting | Value | Rationale |
|---------|-------|-----------|
| `model` | `openai:gpt-4o` | Critical analysis needs strong reasoning |
| `temperature` | `0.5` | Low temperature for objective evaluation |
| `max_tokens` | `5000` | Multiple evidence items need detailed analysis |
| `json` | `true` | Ensure valid JSON output |

---

### Pipe 4: `bayesian-updater-v1`

Probabilistic reasoning and Bayesian updates.

```bash
curl -X POST "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "bayesian-updater-v1",
    "description": "Bayesian-style probability updates with uncertainty quantification, sensitivity analysis, and human-readable interpretations",
    "model": "openai:gpt-4o",
    "upsert": true,
    "json": true,
    "stream": false,
    "store": true,
    "temperature": 0.4,
    "max_tokens": 3000,
    "messages": [
      {
        "role": "system",
        "content": "You are a probabilistic reasoning assistant. Update beliefs using Bayesian inference.\n\nYour response MUST be valid JSON in this format:\n{\n  \"prior\": 0.5,\n  \"posterior\": 0.73,\n  \"confidence_interval\": {\"lower\": 0.65, \"upper\": 0.81, \"level\": 0.95},\n  \"update_steps\": [\n    {\n      \"evidence\": \"description\",\n      \"prior_before\": 0.5,\n      \"likelihood_ratio\": 2.5,\n      \"posterior_after\": 0.71,\n      \"explanation\": \"how this evidence updates belief\"\n    }\n  ],\n  \"uncertainty_analysis\": {\n    \"entropy_before\": 1.0,\n    \"entropy_after\": 0.83,\n    \"information_gained\": 0.17,\n    \"remaining_uncertainty\": \"what remains uncertain\"\n  },\n  \"sensitivity\": {\n    \"most_influential_evidence\": \"which evidence matters most\",\n    \"robustness\": 0.8,\n    \"critical_assumptions\": [\"key assumptions\"]\n  },\n  \"interpretation\": {\n    \"verbal_probability\": \"likely|unlikely|uncertain|etc\",\n    \"recommendation\": \"what to do\",\n    \"caveats\": [\"important caveats\"]\n  }\n}\n\nGuidelines:\n- Apply Bayes rule correctly: P(H|E) = P(E|H) * P(H) / P(E)\n- Estimate likelihood ratios when not provided\n- Calculate entropy and information gain\n- Provide verbal probability interpretations\n- Note critical assumptions\n\nAlways respond with valid JSON only."
      }
    ]
  }'
```

**Configuration:**
| Setting | Value | Rationale |
|---------|-------|-----------|
| `model` | `openai:gpt-4o` | Mathematical reasoning requires strong capability |
| `temperature` | `0.4` | Very low for precise calculations |
| `max_tokens` | `3000` | Moderate output with detailed steps |
| `json` | `true` | Ensure valid JSON output |

---

### Pipe Creation Script

Create a helper script `scripts/create_pipes.sh`:

```bash
#!/bin/bash
# Create Langbase pipes for Decision Framework and Evidence Assessment

set -e

if [ -z "$LANGBASE_API_KEY" ]; then
    echo "Error: LANGBASE_API_KEY environment variable not set"
    exit 1
fi

BASE_URL="${LANGBASE_BASE_URL:-https://api.langbase.com}"

create_pipe() {
    local name=$1
    local description=$2
    local temperature=$3
    local max_tokens=$4
    local system_prompt=$5

    echo "Creating pipe: $name"

    curl -s -X POST "$BASE_URL/v1/pipes" \
        -H "Authorization: Bearer $LANGBASE_API_KEY" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"$name\",
            \"description\": \"$description\",
            \"model\": \"openai:gpt-4o\",
            \"upsert\": true,
            \"json\": true,
            \"stream\": false,
            \"store\": true,
            \"temperature\": $temperature,
            \"max_tokens\": $max_tokens,
            \"messages\": [{\"role\": \"system\", \"content\": $(echo "$system_prompt" | jq -Rs .)}]
        }" | jq .

    echo ""
}

# Pipe 1: Decision Maker
create_pipe "decision-maker-v1" \
    "Multi-criteria decision analysis with weighted scoring" \
    0.6 4000 \
    "You are a structured decision analysis assistant..."

# Pipe 2: Perspective Analyzer
create_pipe "perspective-analyzer-v1" \
    "Stakeholder analysis with power/interest mapping" \
    0.7 4000 \
    "You are a stakeholder analysis assistant..."

# Pipe 3: Evidence Assessor
create_pipe "evidence-assessor-v1" \
    "Structured evidence evaluation with chain analysis" \
    0.5 5000 \
    "You are an evidence assessment assistant..."

# Pipe 4: Bayesian Updater
create_pipe "bayesian-updater-v1" \
    "Bayesian probability updates with uncertainty quantification" \
    0.4 3000 \
    "You are a probabilistic reasoning assistant..."

echo "All pipes created successfully!"
```

---

### Rust Pipe Creation (Optional)

Add automated pipe creation to server startup in `src/main.rs`:

```rust
use crate::langbase::{CreatePipeRequest, Message};

async fn ensure_pipes_exist(client: &LangbaseClient) -> Result<(), AppError> {
    let pipes = vec![
        ("decision-maker-v1", DECISION_MAKER_PROMPT, 0.6, 4000),
        ("perspective-analyzer-v1", PERSPECTIVE_ANALYZER_PROMPT, 0.7, 4000),
        ("evidence-assessor-v1", EVIDENCE_ASSESSOR_PROMPT, 0.5, 5000),
        ("bayesian-updater-v1", BAYESIAN_UPDATER_PROMPT, 0.4, 3000),
    ];

    for (name, prompt, temp, max_tokens) in pipes {
        let request = CreatePipeRequest::new(name)
            .with_model("openai:gpt-4o")
            .with_upsert(true)
            .with_json_output(true)
            .with_temperature(temp)
            .with_max_tokens(max_tokens)
            .with_messages(vec![Message::system(prompt)]);

        match client.create_pipe(request).await {
            Ok(_) => info!(pipe = name, "Pipe ready"),
            Err(e) => warn!(pipe = name, error = %e, "Failed to create pipe"),
        }
    }

    Ok(())
}
```

---

### Pipe Summary Table

> **SUPERSEDED:** The following pipes were planned but never created. Instead, `decision-framework-v1` handles all operations.

| ~~Pipe Name~~ | ~~Temperature~~ | ~~Max Tokens~~ | ~~Model~~ | ~~Purpose~~ |
|-----------|-------------|------------|-------|---------|
| ~~`decision-maker-v1`~~ | ~~0.6~~ | ~~4000~~ | ~~gpt-4o~~ | ~~Multi-criteria decision analysis~~ |
| ~~`perspective-analyzer-v1`~~ | ~~0.7~~ | ~~4000~~ | ~~gpt-4o~~ | ~~Stakeholder perspective mapping~~ |
| ~~`evidence-assessor-v1`~~ | ~~0.5~~ | ~~5000~~ | ~~gpt-4o~~ | ~~Evidence evaluation & chain analysis~~ |
| ~~`bayesian-updater-v1`~~ | ~~0.4~~ | ~~3000~~ | ~~gpt-4o~~ | ~~Probabilistic reasoning & updates~~ |

**Actual Implementation:**
| Pipe Name | Purpose |
|-----------|---------|
| `decision-framework-v1` | All decision and evidence operations (prompts passed dynamically) |

---

### Environment Configuration

> **SUPERSEDED:** The environment variables below were planned but the implementation uses a single consolidated pipe instead.

~~Add to `.env.example`:~~

```bash
# DEPRECATED - Not used in actual implementation
# PIPE_DECISION=decision-maker-v1
# PIPE_PERSPECTIVE=perspective-analyzer-v1
# PIPE_EVIDENCE=evidence-assessor-v1
# PIPE_BAYESIAN=bayesian-updater-v1

# Actual implementation uses:
PIPE_DECISION_FRAMEWORK=decision-framework-v1
```

Update `src/config/mod.rs` to read these:

```rust
/// Decision framework pipe configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionPipes {
    #[serde(default = "default_decision_pipe")]
    pub decision_pipe: Option<String>,
    #[serde(default = "default_perspective_pipe")]
    pub perspective_pipe: Option<String>,
}

fn default_decision_pipe() -> Option<String> {
    Some("decision-maker-v1".to_string())
}

fn default_perspective_pipe() -> Option<String> {
    Some("perspective-analyzer-v1".to_string())
}

/// Evidence assessment pipe configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvidencePipes {
    #[serde(default = "default_evidence_pipe")]
    pub evidence_pipe: Option<String>,
    #[serde(default = "default_bayesian_pipe")]
    pub bayesian_pipe: Option<String>,
}

fn default_evidence_pipe() -> Option<String> {
    Some("evidence-assessor-v1".to_string())
}

fn default_bayesian_pipe() -> Option<String> {
    Some("bayesian-updater-v1".to_string())
}
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Complex JSON parsing | Use `extract_json_from_completion` helper with fallbacks |
| Langbase latency | Implement timeout handling, cache common patterns |
| Schema evolution | Version pipe names, maintain backward compatibility |
| Test flakiness | Use mocked Langbase responses for deterministic tests |
