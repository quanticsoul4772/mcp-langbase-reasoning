# Reasoning Time Machine - Implementation Plan

## Executive Summary

The Reasoning Time Machine extends the existing backtracking and checkpoint system to provide
full temporal navigation through reasoning processes. This enables:

- **Timeline-based exploration**: Create, branch, compare, and merge reasoning timelines
- **Counterfactual analysis**: Ask "What if X?" about past reasoning decisions
- **MCTS-guided exploration**: Monte Carlo Tree Search for intelligent path selection
- **Self-backtracking**: Reward-model guided automatic backtracking

## Research Foundation

Based on research into state-of-the-art reasoning systems:

| Concept | Source | Implementation |
|---------|--------|----------------|
| **Tree-of-Thoughts** | Yao et al. 2023 | Branch at any reasoning step with backtracking |
| **Self-Backtracking** | Zhang et al. 2024 | 40%+ improvement when model decides when to backtrack |
| **MCTS in LLMs** | AlphaGo → reasoning | Selection → Expansion → Simulation → Backpropagation |
| **Counterfactual Reasoning** | Judea Pearl's Ladder | Association → Intervention → Counterfactuals |
| **OpenAI o1/o3** | Hidden reasoning tokens | Internal deliberation before output |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        REASONING TIME MACHINE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────┐    ┌──────────────────────┐    ┌────────────────┐ │
│  │    Timeline Layer   │    │    MCTS Explorer     │    │  Counterfactual │ │
│  │                     │    │                      │    │     Engine      │ │
│  │  - timeline_create  │    │  - UCB selection     │    │                 │ │
│  │  - timeline_branch  │←──→│  - expansion policy  │←──→│  - "what if"    │ │
│  │  - timeline_compare │    │  - rollout sim       │    │  - causal graph │ │
│  │  - timeline_merge   │    │  - backprop scores   │    │  - intervention │ │
│  └─────────────────────┘    └──────────────────────┘    └────────────────┘ │
│            ↓                          ↓                         ↓          │
│  ┌─────────────────────────────────────────────────────────────────────────┤
│  │                        CORE STORAGE LAYER                               │
│  │                                                                          │
│  │   ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────────────┐   │
│  │   │ timelines │  │ timeline_ │  │   mcts_   │  │ counterfactual_   │   │
│  │   │           │  │  branches │  │   nodes   │  │    analyses       │   │
│  │   └───────────┘  └───────────┘  └───────────┘  └───────────────────┘   │
│  │                                                                          │
│  │   Existing: sessions, thoughts, branches, checkpoints, state_snapshots  │
│  └─────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────────┤
│  │                        LANGBASE PIPES                                   │
│  │                                                                          │
│  │   timeline-reasoning-v1  │  mcts-reasoning-v1  │  counterfactual-v1    │
│  └─────────────────────────────────────────────────────────────────────────┤
└─────────────────────────────────────────────────────────────────────────────┘
```

## New Data Models

### Timeline (New)

Represents a complete reasoning path through time, containing multiple branches.

```rust
/// A reasoning timeline representing a complete path through reasoning space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    /// Unique timeline identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Human-readable timeline name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// The root branch of this timeline.
    pub root_branch_id: String,
    /// Currently active branch in this timeline.
    pub active_branch_id: String,
    /// Timeline state.
    pub state: TimelineState,
    /// Total number of branches in this timeline.
    pub branch_count: i32,
    /// Deepest branch depth.
    pub max_depth: i32,
    /// When the timeline was created.
    pub created_at: DateTime<Utc>,
    /// When the timeline was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineState {
    #[default]
    Active,
    Archived,
    Merged,
}
```

### TimelineBranch (Extended Branch)

Extends the existing Branch model with timeline-specific data.

```rust
/// Extended branch data for timeline-based reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranch {
    /// The underlying branch.
    pub branch: Branch,
    /// Parent timeline ID.
    pub timeline_id: String,
    /// Depth in the timeline tree (0 = root).
    pub depth: i32,
    /// MCTS visit count.
    pub visit_count: i32,
    /// MCTS total value/score.
    pub total_value: f64,
    /// UCB exploration score.
    pub ucb_score: Option<f64>,
    /// Counterfactual impact score.
    pub counterfactual_impact: Option<f64>,
    /// Whether this branch was auto-generated by MCTS.
    pub mcts_generated: bool,
    /// Alternative approaches explored from this point.
    pub alternatives_explored: i32,
}
```

### MCTSNode (New)

For Monte Carlo Tree Search exploration.

```rust
/// MCTS node for guided reasoning exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSNode {
    /// Unique node identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Associated timeline branch ID.
    pub branch_id: String,
    /// Parent MCTS node ID.
    pub parent_node_id: Option<String>,
    /// Node content/reasoning state.
    pub content: String,
    /// Visit count (N).
    pub visit_count: i32,
    /// Total value (W).
    pub total_value: f64,
    /// Prior probability from policy network.
    pub prior: f64,
    /// UCB score for selection.
    pub ucb_score: f64,
    /// Whether this node is fully expanded.
    pub is_expanded: bool,
    /// Whether this is a terminal node.
    pub is_terminal: bool,
    /// Simulation depth reached.
    pub simulation_depth: i32,
    /// When the node was created.
    pub created_at: DateTime<Utc>,
    /// When the node was last visited.
    pub last_visited: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

impl MCTSNode {
    /// Calculate UCB1 score for node selection.
    /// UCB1 = Q(s,a) + c * sqrt(ln(N_parent) / N(s,a))
    pub fn calculate_ucb(&self, parent_visits: i32, exploration_constant: f64) -> f64 {
        if self.visit_count == 0 {
            return f64::INFINITY; // Prioritize unvisited nodes
        }

        let exploitation = self.total_value / self.visit_count as f64;
        let exploration = exploration_constant
            * ((parent_visits as f64).ln() / self.visit_count as f64).sqrt();

        exploitation + exploration
    }
}
```

### CounterfactualAnalysis (New)

For "What if?" reasoning.

```rust
/// Counterfactual analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualAnalysis {
    /// Unique analysis identifier.
    pub id: String,
    /// Parent session ID.
    pub session_id: String,
    /// Original branch being analyzed.
    pub original_branch_id: String,
    /// The counterfactual question posed.
    pub question: String,
    /// Intervention type (change, remove, replace).
    pub intervention_type: InterventionType,
    /// The specific intervention made.
    pub intervention: String,
    /// Target thought/decision being modified.
    pub target_thought_id: Option<String>,
    /// Counterfactual branch ID (the "what if" path).
    pub counterfactual_branch_id: String,
    /// Predicted outcome difference.
    pub outcome_delta: f64,
    /// Causal attribution score.
    pub causal_attribution: f64,
    /// Confidence in the counterfactual analysis.
    pub confidence: f64,
    /// Detailed comparison results.
    pub comparison: serde_json::Value,
    /// When the analysis was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterventionType {
    /// Change a decision/thought to something else.
    #[default]
    Change,
    /// Remove a decision/thought entirely.
    Remove,
    /// Replace with alternative reasoning.
    Replace,
    /// Add new information at a point.
    Inject,
}
```

## New MCP Tools (7 tools)

### 1. reasoning_timeline_create

Create a new reasoning timeline with initial checkpoint.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCreateParams {
    /// Optional session ID (creates new if not provided).
    pub session_id: Option<String>,
    /// Timeline name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Initial content/problem statement.
    pub content: String,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCreateResult {
    /// Created timeline ID.
    pub timeline_id: String,
    /// Session ID.
    pub session_id: String,
    /// Root branch ID.
    pub root_branch_id: String,
    /// Initial checkpoint ID.
    pub checkpoint_id: String,
    /// Initial thought ID.
    pub thought_id: String,
}
```

### 2. reasoning_timeline_branch

Branch from any checkpoint to explore alternatives.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranchParams {
    /// Timeline ID to branch from.
    pub timeline_id: String,
    /// Checkpoint ID to branch from.
    pub checkpoint_id: String,
    /// New branch name.
    pub branch_name: Option<String>,
    /// New direction to explore.
    pub new_direction: String,
    /// Optional: specific intervention for counterfactual.
    pub intervention: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranchResult {
    /// New branch ID.
    pub branch_id: String,
    /// Timeline ID.
    pub timeline_id: String,
    /// Parent branch ID.
    pub parent_branch_id: String,
    /// Depth in timeline tree.
    pub depth: i32,
    /// Initial thought on new branch.
    pub thought: TimelineBranchThought,
    /// Checkpoint created at branch point.
    pub checkpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranchThought {
    pub id: String,
    pub content: String,
    pub confidence: f64,
}
```

### 3. reasoning_timeline_compare

Compare outcomes across different branches.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCompareParams {
    /// Timeline ID.
    pub timeline_id: String,
    /// Branch IDs to compare (2-5 branches).
    pub branch_ids: Vec<String>,
    /// Comparison criteria.
    pub criteria: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCompareResult {
    /// Timeline ID.
    pub timeline_id: String,
    /// Comparison summary.
    pub summary: String,
    /// Per-branch analysis.
    pub branches: Vec<BranchAnalysis>,
    /// Ranking of branches by quality.
    pub ranking: Vec<BranchRanking>,
    /// Key differences identified.
    pub differences: Vec<BranchDifference>,
    /// Recommended branch.
    pub recommendation: BranchRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchAnalysis {
    pub branch_id: String,
    pub branch_name: Option<String>,
    pub thought_count: i32,
    pub avg_confidence: f64,
    pub key_insights: Vec<String>,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRanking {
    pub branch_id: String,
    pub rank: i32,
    pub score: f64,
    pub criteria_scores: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchDifference {
    pub aspect: String,
    pub branches: Vec<BranchValue>,
    pub significance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchValue {
    pub branch_id: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRecommendation {
    pub branch_id: String,
    pub reasoning: String,
    pub confidence: f64,
}
```

### 4. reasoning_timeline_merge

Merge insights from multiple branches.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineMergeParams {
    /// Timeline ID.
    pub timeline_id: String,
    /// Branch IDs to merge (2-5 branches).
    pub branch_ids: Vec<String>,
    /// Merge strategy.
    pub strategy: MergeStrategy,
    /// Target branch name for merged result.
    pub target_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Take the best insights from each branch.
    #[default]
    BestOf,
    /// Synthesize a new conclusion from all branches.
    Synthesize,
    /// Create a consensus from overlapping insights.
    Consensus,
    /// Weight by confidence scores.
    WeightedConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineMergeResult {
    /// New merged branch ID.
    pub merged_branch_id: String,
    /// Timeline ID.
    pub timeline_id: String,
    /// Source branches that were merged.
    pub source_branches: Vec<String>,
    /// Merged thought content.
    pub merged_content: String,
    /// Synthesis confidence.
    pub confidence: f64,
    /// Insights taken from each source.
    pub contributions: Vec<BranchContribution>,
    /// Conflicts that were resolved.
    pub resolved_conflicts: Vec<ConflictResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchContribution {
    pub branch_id: String,
    pub insights: Vec<String>,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolution {
    pub conflict_type: String,
    pub branches_involved: Vec<String>,
    pub resolution: String,
    pub confidence: f64,
}
```

### 5. reasoning_counterfactual

"What if X?" analysis on existing reasoning.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualParams {
    /// Session ID.
    pub session_id: String,
    /// Branch ID to analyze.
    pub branch_id: String,
    /// The counterfactual question.
    pub question: String,
    /// Target thought ID to modify (optional - infers if not provided).
    pub target_thought_id: Option<String>,
    /// Specific intervention to apply.
    pub intervention: Option<String>,
    /// Intervention type.
    pub intervention_type: Option<InterventionType>,
    /// Whether to create a new branch for the counterfactual.
    pub create_branch: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualResult {
    /// Analysis ID.
    pub analysis_id: String,
    /// Session ID.
    pub session_id: String,
    /// Original branch analyzed.
    pub original_branch_id: String,
    /// Counterfactual branch (if created).
    pub counterfactual_branch_id: Option<String>,
    /// The question answered.
    pub question: String,
    /// Original outcome summary.
    pub original_outcome: String,
    /// Counterfactual outcome.
    pub counterfactual_outcome: String,
    /// Predicted difference.
    pub outcome_delta: OutcomeDelta,
    /// Causal analysis.
    pub causal_analysis: CausalAnalysis,
    /// Overall confidence.
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeDelta {
    /// Direction of change.
    pub direction: DeltaDirection,
    /// Magnitude of change (0-1).
    pub magnitude: f64,
    /// Key changes identified.
    pub key_changes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaDirection {
    Better,
    Worse,
    Different,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalAnalysis {
    /// Causal attribution score (how much did the intervention cause the change).
    pub attribution: f64,
    /// Identified causal chain.
    pub causal_chain: Vec<String>,
    /// Confounding factors.
    pub confounders: Vec<String>,
    /// Robustness of the analysis.
    pub robustness: f64,
}
```

### 6. reasoning_mcts_explore

MCTS-guided exploration with UCB balancing.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSExploreParams {
    /// Session ID.
    pub session_id: String,
    /// Timeline ID (creates new if not provided).
    pub timeline_id: Option<String>,
    /// Starting branch ID (uses active if not provided).
    pub branch_id: Option<String>,
    /// Problem to explore.
    pub problem: String,
    /// Number of MCTS iterations.
    pub iterations: Option<i32>,
    /// Exploration constant (c in UCB1).
    pub exploration_constant: Option<f64>,
    /// Maximum simulation depth.
    pub max_depth: Option<i32>,
    /// Expansion strategy.
    pub expansion_strategy: Option<ExpansionStrategy>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionStrategy {
    /// Expand most promising unexplored paths.
    #[default]
    BestFirst,
    /// Expand breadth-first.
    BreadthFirst,
    /// Expand with random selection.
    Random,
    /// Expand based on diversity of approaches.
    Diverse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSExploreResult {
    /// Session ID.
    pub session_id: String,
    /// Timeline ID.
    pub timeline_id: String,
    /// Exploration summary.
    pub summary: String,
    /// Total iterations performed.
    pub iterations_performed: i32,
    /// Nodes explored.
    pub nodes_explored: i32,
    /// Best path found.
    pub best_path: MCTSPath,
    /// Alternative promising paths.
    pub alternative_paths: Vec<MCTSPath>,
    /// Tree statistics.
    pub statistics: MCTSStatistics,
    /// Recommended next action.
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSPath {
    /// Path ID.
    pub path_id: String,
    /// Branch ID.
    pub branch_id: String,
    /// Sequence of node IDs in path.
    pub node_ids: Vec<String>,
    /// Total value.
    pub total_value: f64,
    /// Average value.
    pub avg_value: f64,
    /// Depth reached.
    pub depth: i32,
    /// Path summary.
    pub summary: String,
    /// Key insights from this path.
    pub insights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSStatistics {
    /// Total nodes in tree.
    pub total_nodes: i32,
    /// Maximum depth reached.
    pub max_depth: i32,
    /// Average value across nodes.
    pub avg_value: f64,
    /// Exploration/exploitation ratio.
    pub exploration_ratio: f64,
    /// Branches created.
    pub branches_created: i32,
}
```

### 7. reasoning_autobacktrack

Self-backtracking with reward model guidance.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBacktrackParams {
    /// Session ID.
    pub session_id: String,
    /// Timeline ID.
    pub timeline_id: Option<String>,
    /// Current content to evaluate.
    pub content: String,
    /// Threshold for triggering backtrack.
    pub backtrack_threshold: Option<f64>,
    /// Maximum backtrack depth.
    pub max_backtrack_depth: Option<i32>,
    /// Whether to automatically explore alternatives.
    pub auto_explore: Option<bool>,
    /// Number of alternatives to explore if backtracking.
    pub alternatives_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBacktrackResult {
    /// Session ID.
    pub session_id: String,
    /// Whether backtracking was triggered.
    pub backtracked: bool,
    /// Reason for decision.
    pub decision_reason: String,
    /// Quality assessment of current reasoning.
    pub quality_assessment: QualityAssessment,
    /// If backtracked: the new branch.
    pub new_branch: Option<BacktrackBranch>,
    /// If backtracked: alternatives explored.
    pub alternatives: Vec<BacktrackAlternative>,
    /// Recommended next step.
    pub recommendation: String,
    /// Confidence in the decision.
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAssessment {
    /// Overall quality score (0-1).
    pub score: f64,
    /// Coherence score.
    pub coherence: f64,
    /// Progress score.
    pub progress: f64,
    /// Confidence score.
    pub confidence: f64,
    /// Issues identified.
    pub issues: Vec<String>,
    /// Strengths identified.
    pub strengths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktrackBranch {
    pub branch_id: String,
    pub checkpoint_restored: String,
    pub depth_backtracked: i32,
    pub new_direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktrackAlternative {
    pub branch_id: String,
    pub direction: String,
    pub predicted_value: f64,
    pub summary: String,
}
```

## Database Migrations

New migration file: `20240110000001_time_machine.sql`

```sql
-- Phase 10 migration: Reasoning Time Machine support
-- Creates tables for timelines, MCTS nodes, and counterfactual analysis

-- Timelines table: top-level reasoning paths
CREATE TABLE IF NOT EXISTS timelines (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    root_branch_id TEXT NOT NULL,
    active_branch_id TEXT NOT NULL,
    state TEXT DEFAULT 'active',  -- active, archived, merged
    branch_count INTEGER DEFAULT 1,
    max_depth INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (root_branch_id) REFERENCES branches(id) ON DELETE SET NULL,
    FOREIGN KEY (active_branch_id) REFERENCES branches(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_timelines_session ON timelines(session_id);
CREATE INDEX IF NOT EXISTS idx_timelines_state ON timelines(state);

-- Timeline branches: extended branch data for timeline navigation
CREATE TABLE IF NOT EXISTS timeline_branches (
    branch_id TEXT PRIMARY KEY NOT NULL,
    timeline_id TEXT NOT NULL,
    depth INTEGER DEFAULT 0,
    visit_count INTEGER DEFAULT 0,
    total_value REAL DEFAULT 0.0,
    ucb_score REAL,
    counterfactual_impact REAL,
    mcts_generated INTEGER DEFAULT 0,
    alternatives_explored INTEGER DEFAULT 0,
    FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (timeline_id) REFERENCES timelines(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_timeline_branches_timeline ON timeline_branches(timeline_id);
CREATE INDEX IF NOT EXISTS idx_timeline_branches_depth ON timeline_branches(depth);
CREATE INDEX IF NOT EXISTS idx_timeline_branches_ucb ON timeline_branches(ucb_score);

-- MCTS nodes: for Monte Carlo Tree Search exploration
CREATE TABLE IF NOT EXISTS mcts_nodes (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    parent_node_id TEXT,
    content TEXT NOT NULL,
    visit_count INTEGER DEFAULT 0,
    total_value REAL DEFAULT 0.0,
    prior REAL DEFAULT 0.5,
    ucb_score REAL DEFAULT 0.0,
    is_expanded INTEGER DEFAULT 0,
    is_terminal INTEGER DEFAULT 0,
    simulation_depth INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    last_visited TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_node_id) REFERENCES mcts_nodes(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_mcts_nodes_session ON mcts_nodes(session_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_branch ON mcts_nodes(branch_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_parent ON mcts_nodes(parent_node_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_ucb ON mcts_nodes(ucb_score);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_visits ON mcts_nodes(visit_count);

-- Counterfactual analyses: "What if?" reasoning results
CREATE TABLE IF NOT EXISTS counterfactual_analyses (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    original_branch_id TEXT NOT NULL,
    question TEXT NOT NULL,
    intervention_type TEXT NOT NULL,  -- change, remove, replace, inject
    intervention TEXT NOT NULL,
    target_thought_id TEXT,
    counterfactual_branch_id TEXT NOT NULL,
    outcome_delta REAL DEFAULT 0.0,
    causal_attribution REAL DEFAULT 0.0,
    confidence REAL DEFAULT 0.0,
    comparison TEXT NOT NULL,  -- JSON
    created_at TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (original_branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (target_thought_id) REFERENCES thoughts(id) ON DELETE SET NULL,
    FOREIGN KEY (counterfactual_branch_id) REFERENCES branches(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_counterfactual_session ON counterfactual_analyses(session_id);
CREATE INDEX IF NOT EXISTS idx_counterfactual_original ON counterfactual_analyses(original_branch_id);
CREATE INDEX IF NOT EXISTS idx_counterfactual_type ON counterfactual_analyses(intervention_type);
```

## Langbase Pipe Mapping (Using Existing 8 Pipes)

No new pipes required. The Time Machine functionality maps to existing pipes:

### Existing Pipes Available

| Pipe | Purpose | Time Machine Usage |
|------|---------|-------------------|
| `linear-reasoning-v1` | Sequential reasoning | Timeline continuation |
| `tree-reasoning-v1` | Branching exploration | **Timeline branching, MCTS expansion** |
| `divergent-reasoning-v1` | Creative multi-perspective | **Alternative path generation** |
| `reflection-v1` | Meta-cognitive analysis | **Counterfactual analysis, auto-backtrack quality assessment** |
| `mode-router-v1` | Automatic mode selection | Timeline operation routing |
| `got-reasoning-v1` | Graph-of-Thoughts operations | **Timeline comparison, merge synthesis** |
| `detection-v1` | Bias and fallacy detection | Path quality validation |
| `decision-framework-v1` | Decision, perspective, evidence, Bayesian | **MCTS scoring, branch ranking, causal attribution** |

### Tool → Pipe Mapping

| Time Machine Tool | Primary Pipe | Secondary Pipe |
|-------------------|--------------|----------------|
| `reasoning_timeline_create` | `tree-reasoning-v1` | - |
| `reasoning_timeline_branch` | `tree-reasoning-v1` | `divergent-reasoning-v1` |
| `reasoning_timeline_compare` | `got-reasoning-v1` | `decision-framework-v1` |
| `reasoning_timeline_merge` | `got-reasoning-v1` | `reflection-v1` |
| `reasoning_counterfactual` | `reflection-v1` | `decision-framework-v1` |
| `reasoning_mcts_explore` | `tree-reasoning-v1` | `decision-framework-v1` |
| `reasoning_autobacktrack` | `reflection-v1` | `divergent-reasoning-v1` |

### Pipe Enhancement Strategy

Rather than creating new pipes, we enhance existing pipe prompts with context:

#### 1. tree-reasoning-v1 Enhancement

**Additional Context for Timeline/MCTS:**
```json
{
  "operation": "timeline_branch|mcts_expand",
  "timeline_context": {
    "timeline_id": "...",
    "current_branch": {...},
    "checkpoint_state": {...},
    "mcts_params": {
      "exploration_constant": 1.414,
      "visit_counts": {...}
    }
  }
}
```

**Expected Output Extensions:**
```json
{
  "branches": [...],
  "mcts_evaluation": {
    "prior": 0.5,
    "predicted_value": 0.7
  },
  "timeline_metadata": {
    "branch_depth": 2,
    "alternatives_generated": 3
  }
}
```

#### 2. reflection-v1 Enhancement

**Additional Context for Counterfactual:**
```json
{
  "operation": "counterfactual_analysis|quality_assessment",
  "counterfactual_context": {
    "original_reasoning": [...],
    "intervention": {
      "type": "change|remove|replace|inject",
      "target": "thought_id",
      "modification": "..."
    },
    "question": "What if X?"
  }
}
```

**Expected Output Extensions:**
```json
{
  "counterfactual_outcome": "...",
  "causal_analysis": {
    "attribution": 0.8,
    "causal_chain": [...],
    "confounders": [...]
  },
  "outcome_delta": {
    "direction": "better|worse|different",
    "magnitude": 0.6
  }
}
```

#### 3. got-reasoning-v1 Enhancement

**Additional Context for Timeline Comparison/Merge:**
```json
{
  "operation": "timeline_compare|timeline_merge",
  "timeline_context": {
    "branches_to_compare": [...],
    "merge_strategy": "best_of|synthesize|consensus|weighted",
    "comparison_criteria": [...]
  }
}
```

**Expected Output Extensions:**
```json
{
  "comparison": {
    "ranking": [...],
    "differences": [...],
    "recommendation": {...}
  },
  "merged_insight": "...",
  "contributions": [...]
}
```

#### 4. decision-framework-v1 Enhancement

**Additional Context for MCTS Scoring:**
```json
{
  "operation": "mcts_score|branch_rank|causal_attribute",
  "scoring_context": {
    "node_content": "...",
    "parent_visits": 10,
    "exploration_constant": 1.414,
    "criteria": [...]
  }
}
```

**Expected Output Extensions:**
```json
{
  "mcts_score": {
    "value": 0.75,
    "ucb": 1.2,
    "exploitation": 0.7,
    "exploration": 0.5
  },
  "ranking": {
    "position": 1,
    "criteria_scores": {...}
  }
}
```

### Recommended Prompt Modifications

The following prompts in `src/prompts.rs` should be enhanced to support Time Machine operations:

#### 1. TREE_REASONING_PROMPT Enhancement

**Current focus:** Exploring 2-4 reasoning branches
**Enhanced for:** Timeline branching + MCTS expansion

```rust
pub const TREE_REASONING_PROMPT: &str = r#"You are a structured reasoning assistant that explores multiple reasoning paths.

Your response MUST be valid JSON in this format:
{
  "branches": [
    {
      "thought": "reasoning branch content",
      "confidence": 0.8,
      "rationale": "why this branch was explored",
      "mcts_prior": 0.5,
      "predicted_value": 0.7
    }
  ],
  "recommended_branch": 0,
  "timeline_metadata": {
    "branch_depth": 0,
    "alternatives_generated": 3,
    "exploration_strategy": "ucb1|best_first|diverse"
  },
  "metadata": {}
}

Guidelines:
- Explore 2-4 distinct reasoning paths
- Evaluate each branch's viability
- Recommend the most promising branch
- Maintain logical consistency within each branch
- For MCTS operations: provide mcts_prior (policy network estimate) and predicted_value (value estimate)
- For timeline operations: track branch_depth and exploration_strategy"#;
```

#### 2. REFLECTION_PROMPT Enhancement

**Current focus:** Meta-cognitive analysis of reasoning quality
**Enhanced for:** Counterfactual analysis + auto-backtrack quality assessment

```rust
pub const REFLECTION_PROMPT: &str = r#"You are a meta-cognitive reasoning assistant that analyzes and improves reasoning quality.

Your response MUST be valid JSON in this format:
{
  "analysis": "assessment of the reasoning process",
  "strengths": ["identified strengths"],
  "weaknesses": ["identified weaknesses"],
  "recommendations": ["improvement suggestions"],
  "confidence": 0.8,
  "counterfactual_analysis": {
    "question": "the what-if question being analyzed",
    "original_outcome": "summary of original reasoning path",
    "counterfactual_outcome": "predicted outcome with intervention",
    "outcome_delta": {
      "direction": "better|worse|different|unchanged",
      "magnitude": 0.6,
      "key_changes": ["specific differences"]
    },
    "causal_attribution": 0.8,
    "causal_chain": ["step1", "step2", "outcome"],
    "confounders": ["potential confounding factors"]
  },
  "quality_assessment": {
    "score": 0.7,
    "coherence": 0.8,
    "progress": 0.6,
    "should_backtrack": false,
    "backtrack_reason": null,
    "recommended_checkpoint": null
  },
  "metadata": {}
}

Guidelines:
- Evaluate reasoning quality objectively
- Identify logical gaps or biases
- Suggest concrete improvements
- Consider alternative approaches
- For counterfactual analysis: trace causal chains and identify confounders
- For auto-backtrack: assess if current path should be abandoned"#;
```

#### 3. GOT_AGGREGATE_PROMPT Enhancement

**Current focus:** Synthesizing multiple GoT nodes
**Enhanced for:** Timeline comparison + branch merging

```rust
pub const GOT_AGGREGATE_PROMPT: &str = r#"You are a Graph-of-Thoughts synthesizer. Aggregate multiple thought nodes into a unified insight.

Your response MUST be valid JSON in this format:
{
  "aggregated_thought": "synthesized thought combining inputs",
  "confidence": 0.8,
  "sources_used": ["node_id_1", "node_id_2"],
  "synthesis_approach": "how the thoughts were combined",
  "conflicts_resolved": ["any contradictions that were addressed"],
  "timeline_comparison": {
    "branches_analyzed": ["branch_id_1", "branch_id_2"],
    "ranking": [
      {"branch_id": "id", "rank": 1, "score": 0.85, "rationale": "why ranked here"}
    ],
    "differences": [
      {"aspect": "approach", "values": {"branch1": "x", "branch2": "y"}, "significance": 0.7}
    ],
    "recommendation": {
      "branch_id": "best_branch",
      "reasoning": "why this branch is recommended",
      "confidence": 0.8
    }
  },
  "merge_result": {
    "strategy_used": "best_of|synthesize|consensus|weighted",
    "contributions": [
      {"branch_id": "id", "insights": ["insight1"], "weight": 0.6}
    ],
    "conflicts_resolved": [
      {"conflict": "description", "resolution": "how resolved", "confidence": 0.7}
    ]
  },
  "metadata": {}
}

Guidelines:
- Identify common themes across input nodes
- Resolve any contradictions or conflicts
- Create a higher-level synthesis
- Maintain logical consistency
- Preserve valuable insights from each source
- For timeline comparison: rank branches and identify key differences
- For merge operations: specify strategy and track contributions"#;
```

#### 4. DECISION_MAKER_PROMPT Enhancement

**Current focus:** Multi-criteria decision analysis
**Enhanced for:** MCTS node scoring + UCB calculation

```rust
// Add to existing DECISION_MAKER_PROMPT JSON format:
  "mcts_scoring": {
    "node_value": 0.75,
    "visit_count_impact": 0.1,
    "ucb_components": {
      "exploitation": 0.7,
      "exploration": 0.5,
      "ucb_score": 1.2
    },
    "is_leaf": false,
    "expansion_priority": 0.8,
    "simulation_estimate": 0.65
  },
  "branch_ranking": {
    "branches": [
      {
        "branch_id": "id",
        "total_score": 0.85,
        "criteria_scores": {"quality": 0.9, "novelty": 0.7},
        "rank": 1
      }
    ],
    "selection_rationale": "why top branch was selected"
  }

// Add to guidelines:
- For MCTS scoring: calculate UCB1 = Q(s,a) + c * sqrt(ln(N_parent) / N(s,a))
- For branch ranking: apply weighted criteria and provide clear rationale
```

#### 5. BACKTRACKING_PROMPT Enhancement

**Current focus:** Checkpoint restoration
**Enhanced for:** Timeline branching with full state

```rust
// Add to existing BACKTRACKING_PROMPT JSON format:
  "timeline_context": {
    "timeline_id": "associated timeline",
    "branch_created": "new_branch_id",
    "branch_depth": 2,
    "parent_branch": "parent_branch_id",
    "alternatives_at_point": 3
  },
  "mcts_state": {
    "visit_count": 0,
    "initial_prior": 0.5,
    "exploration_potential": 0.8
  }

// Add to guidelines:
- When creating timeline branches: track timeline_id and depth
- Initialize MCTS state for new branches
- Record number of alternatives explored at branch point
```

### Runtime Context Injection Pattern

For operations that need extended context without modifying base prompts:

```rust
/// Build messages with Time Machine context
fn build_timeline_messages(
    base_prompt: &str,
    operation: TimelineOperation,
    context: &TimelineContext,
) -> Vec<Message> {
    let mut messages = vec![Message::system(base_prompt)];

    // Add operation-specific context as user message
    let operation_context = match operation {
        TimelineOperation::Branch { checkpoint, direction } => format!(
            "TIMELINE BRANCH OPERATION\n\
             Timeline: {}\n\
             Branching from checkpoint: {}\n\
             Checkpoint state:\n{}\n\
             New direction: {}\n\n\
             Create a new reasoning branch exploring this direction.",
            context.timeline_id,
            checkpoint.id,
            serde_json::to_string_pretty(&checkpoint.snapshot).unwrap(),
            direction
        ),
        TimelineOperation::MCTSExpand { node, k } => format!(
            "MCTS EXPANSION OPERATION\n\
             Generate {} diverse continuations from this node.\n\
             Current node: {}\n\
             Parent visits: {}\n\
             Exploration constant (c): {}\n\
             Current UCB scores in tree:\n{}\n\n\
             For each continuation, provide mcts_prior and predicted_value.",
            k,
            node.content,
            context.parent_visits,
            context.exploration_constant,
            serde_json::to_string_pretty(&context.ucb_scores).unwrap()
        ),
        TimelineOperation::Compare { branches, criteria } => format!(
            "TIMELINE COMPARISON OPERATION\n\
             Compare these {} reasoning branches:\n{}\n\
             Evaluation criteria: {:?}\n\n\
             Rank branches, identify differences, and recommend the best path.",
            branches.len(),
            branches.iter().map(|b| format!("- {}: {}", b.id, b.name.as_deref().unwrap_or("unnamed")))
                .collect::<Vec<_>>().join("\n"),
            criteria
        ),
        TimelineOperation::Counterfactual { question, intervention } => format!(
            "COUNTERFACTUAL ANALYSIS OPERATION\n\
             Question: {}\n\
             Intervention: {:?}\n\
             Original reasoning path:\n{}\n\n\
             Analyze what would have happened differently.",
            question,
            intervention,
            serde_json::to_string_pretty(&context.original_reasoning).unwrap()
        ),
        TimelineOperation::AutoBacktrack { content, threshold } => format!(
            "AUTO-BACKTRACK QUALITY ASSESSMENT\n\
             Current reasoning:\n{}\n\
             Backtrack threshold: {}\n\n\
             Assess reasoning quality. If score < threshold, recommend backtracking.",
            content,
            threshold
        ),
    };

    messages.push(Message::user(operation_context));
    messages
}

## Implementation Phases

### Phase 1: Core Timeline Infrastructure (Week 1)
1. Add new data models to `src/storage/mod.rs`
2. Create database migration
3. Implement Timeline CRUD operations in `src/storage/sqlite.rs`
4. Create `src/modes/timeline.rs` module

### Phase 2: Timeline Tools (Week 2)
1. Implement `reasoning_timeline_create`
2. Implement `reasoning_timeline_branch`
3. Implement `reasoning_timeline_compare`
4. Implement `reasoning_timeline_merge`
5. Create timeline-reasoning-v1 Langbase pipe

### Phase 3: MCTS Integration (Week 3)
1. Implement MCTSNode operations in storage
2. Create `src/modes/mcts.rs` module
3. Implement `reasoning_mcts_explore`
4. Create mcts-reasoning-v1 Langbase pipe

### Phase 4: Counterfactual Engine (Week 4)
1. Implement CounterfactualAnalysis storage
2. Create `src/modes/counterfactual.rs` module
3. Implement `reasoning_counterfactual`
4. Implement `reasoning_autobacktrack`
5. Create counterfactual-reasoning-v1 Langbase pipe

### Phase 5: Integration & Testing (Week 5)
1. Integration tests for all new tools
2. Performance optimization
3. Documentation updates
4. API reference updates

## Integration with Existing System

### Leveraging Existing Components

1. **Session Management**: Timelines are session-scoped
2. **Branch System**: TimelineBranch extends existing Branch
3. **Checkpoint System**: Reuses existing checkpoint infrastructure
4. **StateSnapshot**: Used for timeline state preservation
5. **Thought System**: All reasoning captured as Thoughts

### Backward Compatibility

All existing tools continue to work unchanged:
- `reasoning_checkpoint_create` / `reasoning_backtrack` still work
- `reasoning_tree` can be used alongside timelines
- `reasoning_got_*` tools operate independently

### Cross-Tool Integration

- MCTS can create branches that are navigable via timeline tools
- Counterfactual analysis creates branches visible in timeline comparison
- Auto-backtrack leverages checkpoint system

## Testing Strategy

### Unit Tests
- Timeline CRUD operations
- MCTS UCB calculation
- Counterfactual intervention types
- Merge strategy implementations

### Integration Tests
- End-to-end timeline workflows
- MCTS exploration with real Langbase calls
- Counterfactual analysis accuracy
- Auto-backtrack decision quality

### Performance Tests
- Large timeline navigation (100+ branches)
- MCTS with high iteration counts
- Concurrent timeline operations

## Success Metrics

| Metric | Target |
|--------|--------|
| Timeline branch latency | < 500ms |
| MCTS iteration throughput | > 10/sec |
| Counterfactual accuracy | > 85% |
| Auto-backtrack precision | > 80% |
| API response time (p95) | < 2s |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Langbase latency for complex operations | Medium | Batch operations, caching |
| MCTS state explosion | High | Pruning, depth limits |
| Counterfactual accuracy | Medium | Confidence thresholds, validation |
| Storage growth | Low | Archiving old timelines, cleanup |

## Future Enhancements

1. **Visual Timeline Explorer**: Web UI for timeline navigation
2. **Collaborative Timelines**: Multi-user timeline editing
3. **Timeline Templates**: Pre-built reasoning patterns
4. **MCTS Training**: Learn from successful explorations
5. **Causal Graph Visualization**: Interactive causal diagrams
