# API Reference

Complete API reference for mcp-langbase-reasoning MCP server.

## MCP Tools

### reasoning_linear

Single-pass sequential reasoning. Process a thought and get a logical continuation or analysis.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The thought content to process"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context continuity"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold (0.0-1.0)"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Reasoning output text",
  "confidence": 0.85,
  "previous_thought": "uuid | null"
}
```

---

### reasoning_tree

Branching exploration with multiple reasoning paths. Explores 2-4 distinct approaches and recommends the most promising one.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The thought content to explore"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "branch_id": {
      "type": "string",
      "description": "Optional branch ID to continue from"
    },
    "max_branches": {
      "type": "integer",
      "minimum": 2,
      "maximum": 10,
      "description": "Maximum branches to explore (default: 4)"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "branch_id": "uuid",
  "content": "Reasoning output",
  "confidence": 0.85,
  "branches_explored": 3,
  "recommended_branch": "uuid"
}
```

---

### reasoning_tree_focus

Focus on a specific branch, making it the active branch for the session.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    },
    "branch_id": {
      "type": "string",
      "description": "Branch ID to focus on"
    }
  },
  "required": ["session_id", "branch_id"]
}
```

---

### reasoning_tree_list

List all branches in a session.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    }
  },
  "required": ["session_id"]
}
```

---

### reasoning_tree_complete

Mark a branch as completed or abandoned.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    },
    "branch_id": {
      "type": "string",
      "description": "Branch ID to complete"
    },
    "state": {
      "type": "string",
      "enum": ["completed", "abandoned"],
      "description": "New state for the branch"
    }
  },
  "required": ["session_id", "branch_id", "state"]
}
```

---

### reasoning_divergent

Creative reasoning that generates novel perspectives and unconventional solutions. Challenges assumptions and synthesizes diverse viewpoints.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The topic or problem to explore creatively"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "num_perspectives": {
      "type": "integer",
      "minimum": 2,
      "maximum": 10,
      "description": "Number of perspectives to generate (default: 3)"
    },
    "constraints": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Optional constraints to apply"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Synthesized creative output",
  "confidence": 0.85,
  "perspectives": [
    {
      "id": "uuid",
      "viewpoint": "Perspective description",
      "novelty_score": 0.8
    }
  ],
  "synthesis": "Integrated insight from all perspectives"
}
```

---

### reasoning_reflection

Meta-cognitive reasoning that analyzes and improves reasoning quality. Evaluates strengths, weaknesses, and provides recommendations.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "Content to reflect on (if not using thought_id)"
    },
    "thought_id": {
      "type": "string",
      "description": "Specific thought ID to evaluate"
    },
    "session_id": {
      "type": "string",
      "description": "Session ID for context"
    },
    "focus_areas": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Areas to focus reflection on"
    },
    "max_iterations": {
      "type": "integer",
      "minimum": 1,
      "maximum": 5,
      "description": "Maximum reflection iterations (default: 1)"
    }
  }
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Reflection analysis",
  "confidence": 0.85,
  "strengths": ["..."],
  "weaknesses": ["..."],
  "recommendations": ["..."],
  "improved_reasoning": "Enhanced version of original"
}
```

---

### reasoning_reflection_evaluate

Evaluate a session's overall reasoning quality, coherence, and provide recommendations.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID to evaluate"
    }
  },
  "required": ["session_id"]
}
```

---

### reasoning_auto

Automatically select the most appropriate reasoning mode based on content analysis.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "Content to analyze for mode selection"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "hints": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Optional hints about the problem type"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "recommended_mode": "tree",
  "confidence": 0.85,
  "rationale": "Content requires exploring multiple options",
  "complexity": 0.6,
  "alternative_modes": [
    {
      "mode": "divergent",
      "confidence": 0.5,
      "rationale": "Could also use creative exploration"
    }
  ]
}
```

---

### reasoning_checkpoint_create

Create a checkpoint at the current reasoning state for later backtracking.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    },
    "name": {
      "type": "string",
      "description": "Checkpoint name"
    },
    "description": {
      "type": "string",
      "description": "Optional description"
    }
  },
  "required": ["session_id", "name"]
}
```

---

### reasoning_checkpoint_list

List all checkpoints available for a session.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    }
  },
  "required": ["session_id"]
}
```

---

### reasoning_backtrack

Restore from a checkpoint and explore alternative reasoning paths.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "checkpoint_id": {
      "type": "string",
      "description": "Checkpoint ID to restore from"
    },
    "new_direction": {
      "type": "string",
      "description": "Optional new direction to explore"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID verification"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold"
    }
  },
  "required": ["checkpoint_id"]
}
```

---

### reasoning_got_init

Initialize a new Graph-of-Thoughts reasoning graph with a root node.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "Initial thought content for root node"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "problem": {
      "type": "string",
      "description": "Problem description for context"
    },
    "config": {
      "type": "object",
      "properties": {
        "max_depth": { "type": "integer" },
        "max_branches": { "type": "integer" },
        "prune_threshold": { "type": "number" }
      },
      "description": "Graph configuration"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "graph_id": "uuid",
  "session_id": "uuid",
  "root_node_id": "uuid",
  "content": "Root node content",
  "confidence": 0.8
}
```

---

### reasoning_got_generate

Generate k diverse continuations from a node in the reasoning graph.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_id": {
      "type": "string",
      "description": "Node ID to generate from"
    },
    "k": {
      "type": "integer",
      "minimum": 1,
      "maximum": 10,
      "description": "Number of continuations to generate (default: 3)"
    },
    "problem": {
      "type": "string",
      "description": "Problem context"
    }
  },
  "required": ["graph_id", "node_id"]
}
```

---

### reasoning_got_score

Score a node's quality based on relevance, validity, depth, and novelty.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_id": {
      "type": "string",
      "description": "Node ID to score"
    },
    "problem": {
      "type": "string",
      "description": "Problem context for scoring"
    }
  },
  "required": ["graph_id", "node_id"]
}
```

---

### reasoning_got_aggregate

Merge multiple reasoning nodes into a unified insight.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_ids": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Node IDs to aggregate"
    },
    "problem": {
      "type": "string",
      "description": "Problem context"
    }
  },
  "required": ["graph_id", "node_ids"]
}
```

---

### reasoning_got_refine

Improve a reasoning node through self-critique and refinement.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_id": {
      "type": "string",
      "description": "Node ID to refine"
    },
    "problem": {
      "type": "string",
      "description": "Problem context"
    }
  },
  "required": ["graph_id", "node_id"]
}
```

---

### reasoning_got_prune

Remove low-scoring nodes from the reasoning graph.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "threshold": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Score threshold for pruning (default: 0.3)"
    }
  },
  "required": ["graph_id"]
}
```

---

### reasoning_got_finalize

Mark terminal nodes and retrieve final conclusions from the reasoning graph.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "terminal_node_ids": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Node IDs to mark as terminal"
    }
  },
  "required": ["graph_id"]
}
```

---

### reasoning_got_state

Get the current state of the reasoning graph including node counts and structure.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    }
  },
  "required": ["graph_id"]
}
```

#### Response

```json
{
  "graph_id": "uuid",
  "session_id": "uuid",
  "node_count": 15,
  "edge_count": 20,
  "max_depth": 4,
  "active_nodes": 5,
  "terminal_nodes": 2,
  "has_cycle": false
}
```

---

### reasoning_detect_biases

Analyze content for cognitive biases such as confirmation bias, anchoring, availability heuristic, sunk cost fallacy, and others. Returns detected biases with severity, confidence, explanation, and remediation suggestions.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The content to analyze for cognitive biases"
    },
    "thought_id": {
      "type": "string",
      "description": "ID of an existing thought to analyze (alternative to content)"
    },
    "session_id": {
      "type": "string",
      "description": "Session ID for context and persistence"
    },
    "check_types": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Specific bias types to check (optional, checks all if not specified)"
    }
  }
}
```

#### Response

```json
{
  "session_id": "uuid",
  "thought_id": "uuid",
  "detections": [
    {
      "bias_type": "confirmation_bias",
      "severity": 4,
      "confidence": 0.85,
      "explanation": "The argument only considers evidence that supports the conclusion",
      "remediation": "Consider evidence that might contradict your conclusion",
      "excerpt": "This proves our hypothesis is correct"
    }
  ],
  "reasoning_quality": 0.7,
  "overall_assessment": "Minor bias detected in reasoning"
}
```

#### Common Bias Types

| Bias Type | Description |
|-----------|-------------|
| `confirmation_bias` | Favoring information that confirms existing beliefs |
| `anchoring_bias` | Over-reliance on first piece of information |
| `availability_heuristic` | Overweighting easily recalled information |
| `sunk_cost_fallacy` | Continuing based on past investment |
| `hindsight_bias` | Believing past events were predictable |
| `bandwagon_effect` | Following what others do |
| `dunning_kruger` | Overestimating one's competence |
| `negativity_bias` | Giving more weight to negative experiences |

---

### reasoning_detect_fallacies

Analyze content for logical fallacies including ad hominem, straw man, false dichotomy, appeal to authority, circular reasoning, and others. Returns detected fallacies with severity, confidence, explanation, and remediation suggestions.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The content to analyze for logical fallacies"
    },
    "thought_id": {
      "type": "string",
      "description": "ID of an existing thought to analyze (alternative to content)"
    },
    "session_id": {
      "type": "string",
      "description": "Session ID for context and persistence"
    },
    "check_formal": {
      "type": "boolean",
      "description": "Check for formal logical fallacies (default: true)"
    },
    "check_informal": {
      "type": "boolean",
      "description": "Check for informal logical fallacies (default: true)"
    }
  }
}
```

#### Response

```json
{
  "session_id": "uuid",
  "thought_id": "uuid",
  "detections": [
    {
      "fallacy_type": "ad_hominem",
      "category": "informal",
      "severity": 4,
      "confidence": 0.9,
      "explanation": "Attacks the person rather than their argument",
      "remediation": "Focus on the argument itself, not the person making it",
      "excerpt": "You can't trust his argument because he's not an expert"
    }
  ],
  "argument_validity": 0.4,
  "overall_assessment": "Multiple fallacies detected affecting argument validity"
}
```

#### Common Fallacy Types

**Formal Fallacies** (errors in logical structure):

| Fallacy Type | Description |
|--------------|-------------|
| `affirming_consequent` | If P then Q; Q; therefore P |
| `denying_antecedent` | If P then Q; not P; therefore not Q |
| `undistributed_middle` | All A are B; all C are B; therefore all A are C |

**Informal Fallacies** (errors in reasoning content):

| Fallacy Type | Description |
|--------------|-------------|
| `ad_hominem` | Attacking the person instead of the argument |
| `straw_man` | Misrepresenting someone's argument |
| `false_dichotomy` | Presenting only two options when more exist |
| `appeal_to_authority` | Using authority as evidence without justification |
| `circular_reasoning` | Using the conclusion as a premise |
| `slippery_slope` | Claiming one event will lead to extreme consequences |
| `red_herring` | Introducing irrelevant information |
| `hasty_generalization` | Drawing broad conclusions from limited evidence |

---

### reasoning_make_decision

Multi-criteria decision analysis using weighted scoring, pairwise comparison, or TOPSIS methods. Evaluates alternatives against criteria with optional weights and provides ranked recommendations.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "question": {
      "type": "string",
      "description": "The decision question to analyze"
    },
    "alternatives": {
      "type": "array",
      "items": { "type": "string" },
      "minItems": 2,
      "description": "Options to evaluate (minimum 2)"
    },
    "criteria": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string", "description": "Criterion name" },
          "weight": { "type": "number", "minimum": 0, "maximum": 1, "description": "Importance weight (0-1)" },
          "description": { "type": "string", "description": "Optional criterion description" }
        },
        "required": ["name"]
      },
      "description": "Evaluation criteria with optional weights"
    },
    "method": {
      "type": "string",
      "enum": ["weighted_sum", "pairwise", "topsis"],
      "description": "Analysis method (default: weighted_sum)"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context persistence"
    },
    "context": {
      "type": "string",
      "description": "Additional context for the decision"
    }
  },
  "required": ["question", "alternatives"]
}
```

#### Response

```json
{
  "session_id": "uuid",
  "question": "Which cloud provider should we use?",
  "method": "weighted_sum",
  "recommendation": {
    "alternative": "AWS",
    "score": 0.85,
    "rank": 1,
    "rationale": "AWS scores highest on cost and scalability"
  },
  "rankings": [
    { "alternative": "AWS", "score": 0.85, "rank": 1 },
    { "alternative": "GCP", "score": 0.78, "rank": 2 },
    { "alternative": "Azure", "score": 0.72, "rank": 3 }
  ],
  "trade_offs": [
    { "between": ["AWS", "GCP"], "description": "AWS is cheaper but GCP has better ML tools" }
  ],
  "sensitivity_analysis": {
    "stable": true,
    "critical_criteria": ["cost"],
    "notes": "Recommendation changes if cost weight drops below 0.3"
  }
}
```

#### Decision Methods

| Method | Description |
|--------|-------------|
| `weighted_sum` | Simple additive weighting - sum of (weight × score) for each criterion |
| `pairwise` | AHP-style pairwise comparison between alternatives |
| `topsis` | Technique for Order Preference by Similarity to Ideal Solution |

---

### reasoning_analyze_perspectives

Stakeholder power/interest matrix analysis. Maps stakeholders to quadrants (KeyPlayer, KeepSatisfied, KeepInformed, MinimalEffort) and identifies conflicts, alignments, and strategic engagement recommendations.

#### Input Schema

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
          "name": { "type": "string", "description": "Stakeholder name" },
          "role": { "type": "string", "description": "Stakeholder role" },
          "interests": {
            "type": "array",
            "items": { "type": "string" },
            "description": "Key interests"
          },
          "power_level": {
            "type": "number",
            "minimum": 0,
            "maximum": 1,
            "description": "Power/influence level (0-1)"
          },
          "interest_level": {
            "type": "number",
            "minimum": 0,
            "maximum": 1,
            "description": "Interest/stake level (0-1)"
          }
        },
        "required": ["name"]
      },
      "description": "Stakeholders to consider (optional - will infer if not provided)"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context persistence"
    },
    "context": {
      "type": "string",
      "description": "Additional context for the analysis"
    }
  },
  "required": ["topic"]
}
```

#### Response

```json
{
  "session_id": "uuid",
  "topic": "Cloud migration decision",
  "power_matrix": {
    "key_player": [
      { "name": "CTO", "power": 0.9, "interest": 0.95, "engagement": "Collaborate closely" }
    ],
    "keep_satisfied": [
      { "name": "CFO", "power": 0.85, "interest": 0.4, "engagement": "Regular updates on cost" }
    ],
    "keep_informed": [
      { "name": "Developers", "power": 0.3, "interest": 0.8, "engagement": "Communicate decisions" }
    ],
    "minimal_effort": [
      { "name": "HR", "power": 0.2, "interest": 0.1, "engagement": "General awareness" }
    ]
  },
  "conflicts": [
    { "stakeholders": ["CTO", "CFO"], "issue": "Feature scope vs budget constraints" }
  ],
  "alignments": [
    { "stakeholders": ["CTO", "Developers"], "area": "Technical excellence" }
  ],
  "synthesis": "Focus on CTO buy-in while managing CFO cost concerns"
}
```

#### Quadrant Definitions

| Quadrant | Power | Interest | Strategy |
|----------|-------|----------|----------|
| KeyPlayer | High | High | Manage closely, collaborate on decisions |
| KeepSatisfied | High | Low | Keep satisfied, don't bore with details |
| KeepInformed | Low | High | Keep informed, address concerns |
| MinimalEffort | Low | Low | Monitor, minimal engagement |

---

### reasoning_assess_evidence

Evidence quality assessment with source credibility analysis, corroboration tracking, and chain of custody evaluation. Returns credibility scores, confidence assessments, and evidence synthesis.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "claim": {
      "type": "string",
      "description": "The claim to assess evidence for"
    },
    "evidence": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "content": { "type": "string", "description": "Evidence content or description" },
          "source": { "type": "string", "description": "Source of the evidence" },
          "source_type": {
            "type": "string",
            "enum": ["primary", "secondary", "tertiary", "expert", "anecdotal"],
            "description": "Type of source"
          },
          "date": { "type": "string", "description": "Date of evidence (ISO format)" }
        },
        "required": ["content"]
      },
      "minItems": 1,
      "description": "Evidence items to assess"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context persistence"
    },
    "context": {
      "type": "string",
      "description": "Additional context for the assessment"
    }
  },
  "required": ["claim", "evidence"]
}
```

#### Response

```json
{
  "claim": "The new architecture improves performance by 50%",
  "overall_credibility": 0.75,
  "confidence": 0.8,
  "evidence_assessments": [
    {
      "content": "Benchmark tests showing 52% improvement",
      "source_credibility": 0.9,
      "relevance": 0.95,
      "corroboration": "Corroborated by production metrics",
      "notes": "Primary source, controlled conditions"
    },
    {
      "content": "User reports of faster load times",
      "source_credibility": 0.6,
      "relevance": 0.7,
      "corroboration": "Partially corroborated",
      "notes": "Anecdotal but consistent with benchmarks"
    }
  ],
  "synthesis": "Strong evidence supports the claim with high confidence from primary sources"
}
```

#### Source Types

| Type | Description | Typical Credibility |
|------|-------------|---------------------|
| `primary` | Original data, firsthand observation | High (0.8-1.0) |
| `secondary` | Analysis of primary sources | Medium-High (0.6-0.9) |
| `tertiary` | Summary of secondary sources | Medium (0.4-0.7) |
| `expert` | Expert opinion without primary data | Medium (0.5-0.8) |
| `anecdotal` | Individual reports, testimonials | Low-Medium (0.2-0.5) |

---

### reasoning_probabilistic

Bayesian probability updates for belief revision. Takes prior probabilities and new evidence to compute posterior probabilities with entropy and uncertainty metrics.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "hypothesis": {
      "type": "string",
      "description": "The hypothesis to evaluate"
    },
    "prior": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Prior probability (0-1)"
    },
    "evidence": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "description": { "type": "string", "description": "Evidence description" },
          "likelihood_if_true": {
            "type": "number",
            "minimum": 0,
            "maximum": 1,
            "description": "P(evidence|hypothesis true)"
          },
          "likelihood_if_false": {
            "type": "number",
            "minimum": 0,
            "maximum": 1,
            "description": "P(evidence|hypothesis false)"
          }
        },
        "required": ["description"]
      },
      "minItems": 1,
      "description": "Evidence items with likelihood ratios"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context persistence"
    }
  },
  "required": ["hypothesis", "prior", "evidence"]
}
```

#### Response

```json
{
  "hypothesis": "The bug is in the authentication module",
  "prior": 0.3,
  "posterior": 0.78,
  "entropy": 0.76,
  "update_steps": [
    {
      "evidence": "Error logs show authentication failures",
      "prior": 0.3,
      "posterior": 0.65,
      "likelihood_ratio": 4.0
    },
    {
      "evidence": "Bug reproduces only with certain credentials",
      "prior": 0.65,
      "posterior": 0.78,
      "likelihood_ratio": 2.0
    }
  ]
}
```

#### Bayesian Formula

```
P(H|E) = P(E|H) × P(H) / P(E)

where:
- P(H|E) = posterior probability
- P(E|H) = likelihood_if_true
- P(H) = prior
- P(E) = P(E|H) × P(H) + P(E|¬H) × P(¬H)
```

#### Entropy Calculation

Shannon entropy measures uncertainty:
```
H = -p × log2(p) - (1-p) × log2(1-p)
```

| Entropy | Interpretation |
|---------|----------------|
| 0 | Complete certainty (p=0 or p=1) |
| 0.5 | Moderate uncertainty |
| 1.0 | Maximum uncertainty (p=0.5) |

---

### reasoning_preset_list

List available workflow presets. Presets are composable multi-step reasoning workflows that combine existing tools into higher-level operations.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "category": {
      "type": "string",
      "description": "Filter by category (e.g., 'code', 'architecture', 'research')"
    }
  },
  "additionalProperties": false
}
```

#### Response

```json
{
  "presets": [
    {
      "id": "code-review",
      "name": "Code Review Workflow",
      "description": "Multi-step code analysis with bias and fallacy detection",
      "category": "code",
      "steps": 5,
      "input_schema": {
        "code": "string (required) - The code to review"
      }
    }
  ],
  "count": 5
}
```

#### Built-in Presets

| Preset ID | Category | Description |
|-----------|----------|-------------|
| `code-review` | code | 4-step code review: divergent analysis → bias detection → fallacy detection → reflection |
| `debug-analysis` | code | 4-step debugging: linear analysis → tree exploration → checkpoint save → reflection |
| `architecture-decision` | architecture | 5-step decision: divergent exploration → GoT init → GoT generate → GoT score → GoT finalize |
| `strategic-decision` | decision | 4-step decision: multi-criteria analysis → stakeholder perspectives → bias detection → synthesis |
| `evidence-based-conclusion` | research | 4-step conclusion: evidence assessment → Bayesian probability update → fallacy detection → reflection |

---

### reasoning_preset_run

Execute a workflow preset with custom inputs. Runs all steps in sequence, passing results between steps based on dependencies and input mappings.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "preset_id": {
      "type": "string",
      "description": "ID of the preset to run"
    },
    "inputs": {
      "type": "object",
      "description": "Input values for the preset (varies by preset)"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context continuity"
    }
  },
  "required": ["preset_id"],
  "additionalProperties": false
}
```

#### Example: Running Code Review

```json
{
  "preset_id": "code-review",
  "inputs": {
    "code": "function calculate(a, b) { return a + b; }"
  }
}
```

#### Response

```json
{
  "preset_id": "code-review",
  "status": "completed",
  "session_id": "uuid",
  "steps_completed": 5,
  "total_steps": 5,
  "results": {
    "analysis": { "thought_id": "uuid", "content": "..." },
    "bias_check": { "detections": [], "reasoning_quality": 0.9 },
    "fallacy_check": { "detections": [], "argument_validity": 0.95 },
    "reflection": { "thought_id": "uuid", "content": "..." },
    "final": { "thought_id": "uuid", "content": "..." }
  },
  "execution_time_ms": 2500
}
```

#### Preset Step Features

Steps can have:

| Feature | Description |
|---------|-------------|
| `depends_on` | Array of step IDs that must complete first |
| `condition` | Condition to evaluate before running (gt, gte, lt, lte, eq, neq, contains, exists) |
| `optional` | If true, failures don't stop the workflow |
| `store_as` | Key to store result for use by later steps |
| `input_map` | Maps preset inputs or step results to tool parameters |

#### Error Handling

If a non-optional step fails, execution stops and returns partial results:

```json
{
  "preset_id": "code-review",
  "status": "failed",
  "steps_completed": 2,
  "total_steps": 5,
  "error": "Step 'reflection' failed: Langbase API unavailable",
  "results": {
    "analysis": { ... },
    "bias_check": { ... }
  }
}
```

#### Example: Running Strategic Decision

```json
{
  "preset_id": "strategic-decision",
  "inputs": {
    "question": "Should we migrate to cloud or stay on-premise?",
    "alternatives": ["AWS Migration", "Azure Migration", "On-premise Upgrade"]
  }
}
```

#### Response

```json
{
  "preset_id": "strategic-decision",
  "status": "completed",
  "session_id": "uuid",
  "steps_completed": 4,
  "total_steps": 4,
  "results": {
    "decision": {
      "ranking": [
        { "alternative": "AWS Migration", "score": 0.85 },
        { "alternative": "Azure Migration", "score": 0.78 },
        { "alternative": "On-premise Upgrade", "score": 0.62 }
      ],
      "trade_offs": [...]
    },
    "perspectives": {
      "stakeholders": [...],
      "power_matrix": {...}
    },
    "biases": { "detections": [], "reasoning_quality": 0.88 },
    "synthesis": { "thought_id": "uuid", "content": "..." }
  },
  "execution_time_ms": 3200
}
```

#### Example: Running Evidence-Based Conclusion

```json
{
  "preset_id": "evidence-based-conclusion",
  "inputs": {
    "claim": "The new feature improves user engagement by 20%",
    "evidence": [
      { "content": "A/B test results showing 22% improvement", "source_type": "primary" },
      { "content": "User feedback surveys indicating satisfaction", "source_type": "survey" }
    ],
    "prior": 0.5
  }
}
```

#### Response

```json
{
  "preset_id": "evidence-based-conclusion",
  "status": "completed",
  "session_id": "uuid",
  "steps_completed": 4,
  "total_steps": 4,
  "results": {
    "assessment": {
      "overall_credibility": 0.82,
      "evidence_ratings": [...]
    },
    "probability": {
      "prior": 0.5,
      "posterior": 0.78,
      "steps": [...]
    },
    "fallacies": { "detections": [], "argument_validity": 0.92 },
    "conclusion": { "thought_id": "uuid", "content": "..." }
  },
  "execution_time_ms": 2800
}
```

---

## Data Types

### Session

Represents a reasoning context that groups related thoughts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique session identifier (UUID) |
| `mode` | `string` | Reasoning mode (`linear`, `tree`, etc.) |
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `updated_at` | `datetime` | ISO 8601 last update timestamp |
| `metadata` | `object?` | Optional arbitrary metadata |
| `active_branch_id` | `string?` | Currently active branch (tree mode) |

### Thought

Represents a single reasoning step within a session.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique thought identifier (UUID) |
| `session_id` | `string` | Parent session ID |
| `content` | `string` | Reasoning content text |
| `confidence` | `number` | Confidence score (0.0-1.0) |
| `mode` | `string` | Reasoning mode used |
| `parent_id` | `string?` | Parent thought ID (for branching) |
| `branch_id` | `string?` | Branch ID (for tree mode) |
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `metadata` | `object?` | Optional arbitrary metadata |

### Branch

Represents a reasoning branch in tree mode.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique branch identifier (UUID) |
| `session_id` | `string` | Parent session ID |
| `name` | `string?` | Optional branch name |
| `parent_id` | `string?` | Parent branch ID |
| `state` | `string` | `active`, `completed`, or `abandoned` |
| `confidence` | `number` | Branch confidence score |
| `priority` | `integer` | Branch priority |
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `updated_at` | `datetime` | ISO 8601 last update timestamp |

### Checkpoint

Represents a saved reasoning state for backtracking.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique checkpoint identifier (UUID) |
| `session_id` | `string` | Parent session ID |
| `branch_id` | `string?` | Associated branch ID |
| `name` | `string` | Checkpoint name |
| `description` | `string?` | Optional description |
| `snapshot` | `object` | Serialized state data |
| `created_at` | `datetime` | ISO 8601 creation timestamp |

### GraphNode

Represents a node in a Graph-of-Thoughts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique node identifier (UUID) |
| `graph_id` | `string` | Parent graph ID |
| `content` | `string` | Node content |
| `node_type` | `string` | `thought`, `hypothesis`, `conclusion`, `aggregation`, `root`, `refinement`, `terminal` |
| `score` | `number` | Quality score (0.0-1.0) |
| `depth` | `integer` | Depth in graph |
| `is_active` | `boolean` | Whether node is active |
| `is_terminal` | `boolean` | Whether node is terminal |
| `created_at` | `datetime` | ISO 8601 creation timestamp |

### GraphEdge

Represents an edge between nodes in a Graph-of-Thoughts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique edge identifier (UUID) |
| `graph_id` | `string` | Parent graph ID |
| `from_node` | `string` | Source node ID |
| `to_node` | `string` | Target node ID |
| `edge_type` | `string` | `generates`, `refines`, `aggregates`, `supports`, `contradicts` |
| `weight` | `number` | Edge weight (0.0-1.0) |
| `created_at` | `datetime` | ISO 8601 creation timestamp |

### Invocation

Logs API calls for debugging and auditing.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique invocation identifier |
| `session_id` | `string?` | Associated session ID |
| `tool_name` | `string` | Tool that was invoked |
| `input` | `object` | Input parameters |
| `output` | `object?` | Response data |
| `pipe_name` | `string?` | Langbase pipe used |
| `latency_ms` | `integer?` | Request latency |
| `success` | `boolean` | Whether invocation succeeded |
| `error` | `string?` | Error message if failed |
| `created_at` | `datetime` | ISO 8601 timestamp |

### Detection

Represents a detected cognitive bias or logical fallacy.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique detection identifier (UUID) |
| `session_id` | `string?` | Associated session ID |
| `thought_id` | `string?` | Associated thought ID |
| `detection_type` | `string` | `bias` or `fallacy` |
| `name` | `string` | Specific bias/fallacy name |
| `severity` | `integer` | Severity level (1-5) |
| `confidence` | `number` | Detection confidence (0.0-1.0) |
| `explanation` | `string` | Detailed explanation |
| `remediation` | `string?` | Suggested fix |
| `excerpt` | `string?` | Relevant text excerpt |
| `created_at` | `datetime` | ISO 8601 timestamp |
| `metadata` | `object?` | Optional additional metadata |

### WorkflowPreset

Represents a composable reasoning workflow.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique preset identifier |
| `name` | `string` | Human-readable preset name |
| `description` | `string` | Description of what the preset does |
| `category` | `string` | Category (code, architecture, research, etc.) |
| `steps` | `PresetStep[]` | Ordered list of steps to execute |
| `input_schema` | `object` | JSON Schema for required inputs |

### PresetStep

Represents a single step in a workflow preset.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique step identifier within preset |
| `tool` | `string` | MCP tool name to invoke |
| `description` | `string?` | Step description |
| `input_map` | `object?` | Maps inputs to tool parameters |
| `store_as` | `string?` | Key to store result for later steps |
| `depends_on` | `string[]?` | Step IDs that must complete first |
| `optional` | `boolean` | If true, failures don't stop workflow |
| `condition` | `StepCondition?` | Condition to evaluate before running |

### StepCondition

Condition for conditional step execution.

| Field | Type | Description |
|-------|------|-------------|
| `field` | `string` | Field path to check (e.g., "analysis.confidence") |
| `operator` | `string` | Comparison operator (gt, gte, lt, lte, eq, neq, contains, exists) |
| `value` | `any?` | Value to compare against (not needed for exists) |

---

## Error Handling

### JSON-RPC Error Codes

| Code | Name | Description |
|------|------|-------------|
| `-32700` | Parse Error | Invalid JSON received |
| `-32601` | Method Not Found | Unknown method |
| `-32602` | Invalid Params | Invalid method parameters |
| `-32603` | Internal Error | Server-side error |

### Application Errors

Errors are returned in the tool result with `isError: true`:

```json
{
  "content": [{
    "type": "text",
    "text": "Error: Validation failed: content - Content cannot be empty"
  }],
  "isError": true
}
```

#### Error Types

| Type | Description |
|------|-------------|
| `Validation` | Input validation failed |
| `SessionNotFound` | Referenced session does not exist |
| `ThoughtNotFound` | Referenced thought does not exist |
| `BranchNotFound` | Referenced branch does not exist |
| `CheckpointNotFound` | Referenced checkpoint does not exist |
| `GraphNotFound` | Referenced graph does not exist |
| `NodeNotFound` | Referenced node does not exist |
| `LangbaseUnavailable` | Langbase API unreachable after retries |
| `ApiError` | Langbase API returned error |
| `Timeout` | Request timed out |

---

## MCP Protocol

### Initialize

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "claude-desktop",
      "version": "1.0.0"
    }
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {
        "listChanged": false
      }
    },
    "serverInfo": {
      "name": "mcp-langbase-reasoning",
      "version": "0.1.0"
    }
  }
}
```

### List Tools

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list"
}
```

### Call Tool

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "reasoning_linear",
    "arguments": {
      "content": "Your reasoning prompt"
    }
  }
}
```

### Ping

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "ping"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {}
}
```

---

## Langbase Integration

### Pipe Request Format

The server sends requests to Langbase in this format:

```json
{
  "name": "linear-reasoning-v1",
  "messages": [
    {"role": "system", "content": "System prompt..."},
    {"role": "user", "content": "User input"}
  ],
  "stream": false,
  "threadId": "session-uuid"
}
```

### Expected Pipe Response

Langbase pipes should return JSON in this format:

```json
{
  "thought": "Reasoning output text",
  "confidence": 0.85,
  "metadata": {}
}
```

If the pipe returns non-JSON, the entire response is treated as the thought content with default confidence (0.8).

---

## Configuration Reference

### General Settings

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LANGBASE_API_KEY` | Yes | - | Langbase API key |
| `LANGBASE_BASE_URL` | No | `https://api.langbase.com` | API endpoint |
| `DATABASE_PATH` | No | `./data/reasoning.db` | SQLite database path |
| `DATABASE_MAX_CONNECTIONS` | No | `5` | Connection pool size |
| `LOG_LEVEL` | No | `info` | Logging level |
| `LOG_FORMAT` | No | `pretty` | Log format (`pretty`, `json`) |
| `REQUEST_TIMEOUT_MS` | No | `30000` | HTTP timeout (ms) |
| `MAX_RETRIES` | No | `3` | Max retry attempts |
| `RETRY_DELAY_MS` | No | `1000` | Initial retry delay |

### Pipe Names

Consolidated pipes (8 total, fits Langbase free tier):

| Variable | Default | Description |
|----------|---------|-------------|
| `PIPE_LINEAR` | `linear-reasoning-v1` | Linear reasoning pipe |
| `PIPE_TREE` | `tree-reasoning-v1` | Tree reasoning pipe |
| `PIPE_DIVERGENT` | `divergent-reasoning-v1` | Divergent reasoning pipe |
| `PIPE_REFLECTION` | `reflection-v1` | Reflection pipe |
| `PIPE_AUTO` | `mode-router-v1` | Auto mode router pipe |
| `PIPE_GOT` | `got-reasoning-v1` | Graph-of-Thoughts (all operations) |
| `PIPE_DETECTION` | `detection-v1` | Bias and fallacy detection |
| `PIPE_DECISION_FRAMEWORK` | `decision-framework-v1` | Decision, perspective, evidence, Bayesian |
