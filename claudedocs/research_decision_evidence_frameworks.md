# Research Report: Decision Framework & Evidence Assessment Implementation

## Executive Summary

This research explores implementation patterns for two high-priority feature additions to the mcp-langbase-reasoning server:

1. **Decision Framework** (Score: 0.78) - Multi-criteria decision analysis with stakeholder perspectives
2. **Evidence Assessment** (Score: 0.73) - Structured evidence evaluation with probabilistic reasoning

Both features align with the existing architecture and can leverage established patterns from the codebase.

---

## 1. Decision Framework Analysis

### 1.1 Core Concepts from Research

Based on industry best practices and academic research:

**Multi-Criteria Decision Analysis (MCDA) Methods:**
- **AHP (Analytic Hierarchy Process)** - Pairwise comparisons for priority scales
- **TOPSIS** - Distance from ideal solutions
- **Weighted Sum Model** - Simple additive weighting
- **ELECTRE** - Outranking relations

**AI-Powered Decision Making Features:**
- Predictive analytics for stakeholder reactions
- NLP for sentiment analysis on stakeholder positions
- Power/Interest matrix generation
- Multi-perspective synthesis

### 1.2 Proposed Tools

#### `reasoning_make_decision`

**Purpose:** Structured multi-criteria decision making with weighted scoring.

**Input Schema:**
```json
{
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
        }
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
}
```

**Response Schema:**
```json
{
  "decision_id": "uuid",
  "session_id": "uuid",
  "question": "string",
  "recommendation": {
    "option": "string",
    "score": 0.85,
    "confidence": 0.82,
    "rationale": "string"
  },
  "scores": [
    {
      "option": "string",
      "total_score": 0.85,
      "criteria_scores": {
        "criterion_name": {
          "score": 0.9,
          "reasoning": "string"
        }
      },
      "rank": 1
    }
  ],
  "sensitivity_analysis": {
    "robust": true,
    "critical_criteria": ["string"],
    "threshold_changes": {
      "criterion_name": 0.15
    }
  },
  "trade_offs": [
    {
      "between": ["option_a", "option_b"],
      "trade_off": "string"
    }
  ],
  "constraints_satisfied": {
    "option": true
  }
}
```

#### `reasoning_analyze_perspectives`

**Purpose:** Stakeholder analysis with power/interest mapping and perspective synthesis.

**Input Schema:**
```json
{
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
          "interests": { "type": "array", "items": { "type": "string" } }
        }
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
}
```

**Response Schema:**
```json
{
  "analysis_id": "uuid",
  "session_id": "uuid",
  "topic": "string",
  "stakeholders": [
    {
      "name": "string",
      "role": "string",
      "perspective": "string",
      "interests": ["string"],
      "concerns": ["string"],
      "power_level": 0.8,
      "interest_level": 0.9,
      "quadrant": "key_player",
      "engagement_strategy": "string"
    }
  ],
  "power_matrix": {
    "key_players": ["string"],
    "keep_satisfied": ["string"],
    "keep_informed": ["string"],
    "minimal_effort": ["string"]
  },
  "conflicts": [
    {
      "stakeholders": ["string", "string"],
      "issue": "string",
      "severity": 0.7,
      "resolution_approach": "string"
    }
  ],
  "alignments": [
    {
      "stakeholders": ["string", "string"],
      "shared_interest": "string"
    }
  ],
  "synthesis": {
    "consensus_areas": ["string"],
    "contentious_areas": ["string"],
    "recommendation": "string"
  },
  "confidence": 0.82
}
```

---

## 2. Evidence Assessment Analysis

### 2.1 Core Concepts from Research

**Probabilistic Reasoning Frameworks:**
- **Bayesian Networks** - Representing epistemic relationships in uncertain evidence
- **Likelihood Ratios** - Measuring inferential force of evidence
- **Evidence-Based Reasoning Framework** - Mapping claims, premises, rules, evidence, data
- **Elementary Probabilistic Operations (EPO)** - Joint, marginal, conditional probabilities

**Key Principles:**
- Evidence assessment requires structured relevance and credibility analysis
- Chains of reasoning create multi-stage inferential patterns
- Source credibility affects evidence weight
- Contradictory evidence requires explicit handling

### 2.2 Proposed Tools

#### `reasoning_assess_evidence`

**Purpose:** Structured evidence evaluation with credibility and relevance scoring.

**Input Schema:**
```json
{
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
}
```

**Response Schema:**
```json
{
  "assessment_id": "uuid",
  "session_id": "uuid",
  "claim": "string",
  "overall_support": {
    "level": "strong|moderate|weak|insufficient|contradictory",
    "confidence": 0.75,
    "explanation": "string"
  },
  "evidence_analysis": [
    {
      "evidence_id": "uuid",
      "content_summary": "string",
      "relevance": {
        "score": 0.85,
        "type": "direct|indirect|tangential",
        "explanation": "string"
      },
      "credibility": {
        "score": 0.80,
        "factors": {
          "source_reliability": 0.9,
          "methodology": 0.7,
          "recency": 0.8,
          "corroboration": 0.75
        },
        "concerns": ["string"]
      },
      "weight": 0.68,
      "supports_claim": true,
      "inferential_distance": 2
    }
  ],
  "chain_analysis": {
    "primary_chain": ["evidence_id", "inference", "claim"],
    "weak_links": [
      {
        "from": "string",
        "to": "string",
        "weakness": "string",
        "impact": 0.3
      }
    ],
    "redundancy": ["evidence_id", "evidence_id"],
    "synergies": [
      {
        "evidence_ids": ["string", "string"],
        "combined_strength": 0.9,
        "explanation": "string"
      }
    ]
  },
  "contradictions": [
    {
      "evidence_a": "string",
      "evidence_b": "string",
      "nature": "string",
      "resolution": "string"
    }
  ],
  "gaps": [
    {
      "gap": "string",
      "importance": 0.8,
      "suggested_evidence": "string"
    }
  ],
  "recommendations": ["string"]
}
```

#### `reasoning_probabilistic`

**Purpose:** Bayesian-style probability updates and uncertainty quantification.

**Input Schema:**
```json
{
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
}
```

**Response Schema:**
```json
{
  "update_id": "uuid",
  "session_id": "uuid",
  "hypothesis": "string",
  "prior": 0.5,
  "posterior": 0.73,
  "confidence_interval": {
    "lower": 0.65,
    "upper": 0.81,
    "level": 0.95
  },
  "update_steps": [
    {
      "evidence": "string",
      "prior_before": 0.5,
      "likelihood_ratio": 2.5,
      "posterior_after": 0.71,
      "explanation": "string"
    }
  ],
  "uncertainty_analysis": {
    "entropy_before": 1.0,
    "entropy_after": 0.83,
    "information_gained": 0.17,
    "remaining_uncertainty": "string"
  },
  "sensitivity": {
    "most_influential_evidence": "string",
    "robustness": 0.8,
    "critical_assumptions": ["string"]
  },
  "interpretation": {
    "verbal_probability": "likely",
    "recommendation": "string",
    "caveats": ["string"]
  }
}
```

---

## 3. Implementation Architecture

### 3.1 Integration with Existing Patterns

The new tools follow established patterns from the codebase:

**Mode Structure (like `src/modes/divergent.rs`):**
```rust
// src/modes/decision.rs
pub struct DecisionMode {
    langbase: Arc<LangbaseClient>,
    storage: Arc<Storage>,
    config: Arc<Config>,
}

impl DecisionMode {
    pub async fn make_decision(&self, params: DecisionParams) -> Result<DecisionResponse, ModeError> {
        // 1. Validate input
        // 2. Get/create session
        // 3. Build Langbase prompt
        // 4. Call pipe
        // 5. Parse response
        // 6. Persist to storage
        // 7. Return response
    }

    pub async fn analyze_perspectives(&self, params: PerspectiveParams) -> Result<PerspectiveResponse, ModeError> {
        // Similar pattern
    }
}
```

**Handler Integration (like `src/server/handlers.rs`):**
```rust
// Add to handle_tool_call match
"reasoning_make_decision" => handle_make_decision(state, arguments).await,
"reasoning_analyze_perspectives" => handle_analyze_perspectives(state, arguments).await,
"reasoning_assess_evidence" => handle_assess_evidence(state, arguments).await,
"reasoning_probabilistic" => handle_probabilistic(state, arguments).await,
```

### 3.2 Storage Schema Extensions

**New Tables:**
```sql
-- decisions table
CREATE TABLE IF NOT EXISTS decisions (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id),
    question TEXT NOT NULL,
    options TEXT NOT NULL,  -- JSON array
    criteria TEXT,          -- JSON array
    recommendation TEXT,    -- JSON object
    scores TEXT,            -- JSON array
    confidence REAL,
    method TEXT,
    created_at TEXT NOT NULL
);

-- evidence_assessments table
CREATE TABLE IF NOT EXISTS evidence_assessments (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id),
    claim TEXT NOT NULL,
    evidence TEXT NOT NULL,        -- JSON array
    overall_support TEXT,          -- JSON object
    evidence_analysis TEXT,        -- JSON array
    chain_analysis TEXT,           -- JSON object
    confidence REAL,
    created_at TEXT NOT NULL
);

-- probability_updates table
CREATE TABLE IF NOT EXISTS probability_updates (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id),
    hypothesis TEXT NOT NULL,
    prior REAL NOT NULL,
    posterior REAL NOT NULL,
    update_steps TEXT,             -- JSON array
    created_at TEXT NOT NULL
);
```

### 3.3 Langbase Pipe Requirements

**New Pipes Needed:**
| Pipe Name | Purpose |
|-----------|---------|
| `decision-maker-v1` | Multi-criteria decision analysis |
| `perspective-analyzer-v1` | Stakeholder perspective synthesis |
| `evidence-assessor-v1` | Evidence evaluation and chain analysis |
| `bayesian-updater-v1` | Probabilistic reasoning and updates |

### 3.4 Module Structure

```
src/
├── modes/
│   ├── mod.rs           # Add decision, evidence exports
│   ├── decision.rs      # NEW: Decision framework implementation
│   └── evidence.rs      # NEW: Evidence assessment implementation
├── server/
│   ├── handlers.rs      # Add 4 new handlers
│   └── mcp.rs           # Add 4 tool definitions
└── storage/
    └── mod.rs           # Add new table schemas
```

---

## 4. Implementation Priorities

### Phase 1: Decision Framework (2-3 days)
1. Create `src/modes/decision.rs` with `DecisionMode` struct
2. Implement `reasoning_make_decision`
3. Implement `reasoning_analyze_perspectives`
4. Add storage tables for decisions
5. Add MCP tool definitions
6. Write tests

### Phase 2: Evidence Assessment (2-3 days)
1. Create `src/modes/evidence.rs` with `EvidenceMode` struct
2. Implement `reasoning_assess_evidence`
3. Implement `reasoning_probabilistic`
4. Add storage tables for evidence
5. Add MCP tool definitions
6. Write tests

### Phase 3: Integration & Presets (1-2 days)
1. Create `decision-analysis` preset combining tools
2. Create `evidence-review` preset
3. Integration tests
4. Documentation updates

---

## 5. Synergies with Existing Tools

### Preset Opportunities

**`strategic-decision` Preset:**
```
1. reasoning_divergent → Generate options
2. reasoning_analyze_perspectives → Stakeholder analysis
3. reasoning_make_decision → Multi-criteria scoring
4. reasoning_detect_biases → Check for decision biases
5. reasoning_reflection → Final synthesis
```

**`evidence-based-conclusion` Preset:**
```
1. reasoning_linear → Initial analysis
2. reasoning_assess_evidence → Evidence evaluation
3. reasoning_probabilistic → Update confidence
4. reasoning_detect_fallacies → Check reasoning
5. reasoning_reflection → Synthesize conclusion
```

### Tool Composition

The new tools integrate naturally with existing capabilities:

| Existing Tool | New Tool | Synergy |
|---------------|----------|---------|
| `reasoning_divergent` | `reasoning_analyze_perspectives` | Perspectives feed into stakeholder analysis |
| `reasoning_tree` | `reasoning_make_decision` | Tree branches become decision options |
| `reasoning_reflection` | `reasoning_assess_evidence` | Evidence assessment informs reflection |
| `reasoning_got_score` | `reasoning_probabilistic` | Graph node scoring with probability updates |

---

## 6. Success Metrics

### Decision Framework
- [ ] `reasoning_make_decision` returns ranked options with scores
- [ ] `reasoning_analyze_perspectives` identifies 3+ stakeholders
- [ ] Sensitivity analysis detects criteria threshold changes
- [ ] Power/interest matrix correctly categorizes stakeholders

### Evidence Assessment
- [ ] `reasoning_assess_evidence` evaluates relevance and credibility
- [ ] Chain analysis identifies weak inferential links
- [ ] `reasoning_probabilistic` correctly applies Bayes' rule
- [ ] Confidence intervals reflect uncertainty

### Integration
- [ ] All 4 tools accessible via MCP
- [ ] Results persist to SQLite
- [ ] Works in workflow presets
- [ ] All tests pass

---

## Sources

- [AI-powered stakeholder analysis - Dart AI](https://www.dartai.com/blog/ai-powered-stakeholder-analysis)
- [Stakeholder Analysis with AI - InformIT](https://www.informit.com/articles/article.aspx?p=3192418&seqNum=3)
- [AI in Project Decision Making - Digital PM](https://thedigitalprojectmanager.com/project-management/ai-in-project-decision-making/)
- [Bayesian Networks for Evidence - ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0379073805005402)
- [Evidence-Based Reasoning Framework - ResearchGate](https://www.researchgate.net/publication/233120155_The_Evidence-Based_Reasoning_Framework_Assessing_Scientific_Reasoning)
- [Elementary Probabilistic Operations - Taylor & Francis](https://www.tandfonline.com/doi/full/10.1080/13546783.2023.2259541)
