# Architecture

Technical architecture documentation for mcp-langbase-reasoning.

## System Overview

```
                              MCP Client
                         (Claude Desktop, etc.)
                                  |
                                  | stdio (JSON-RPC 2.0)
                                  v
+-------------------------------------------------------------------------+
|                        mcp-langbase-reasoning                            |
|  +-------------+  +---------------+  +-----------+  +-----------------+ |
|  |  MCP Layer  |  |    Modes      |  |  Storage  |  | Langbase Client | |
|  |             |  |               |  |           |  |                 | |
|  | - Protocol  |  | - Linear      |  | - Sessions|  | - HTTP Client   | |
|  | - Routing   |  | - Tree        |  | - Thoughts|  | - Retry Logic   | |
|  | - Schema    |  | - Divergent   |  | - Branches|  | - Response Parse| |
|  |             |  | - Reflection  |  | - Checks  |  |                 | |
|  |             |  | - Backtrack   |  | - Graphs  |  |                 | |
|  |             |  | - Auto        |  | - Invoc.  |  |                 | |
|  |             |  | - GoT         |  | - Timel.  |  |                 | |
|  |             |  | - Detection   |  | - MCTS    |  |                 | |
|  |             |  | - Decision    |  | - Counter.|  |                 | |
|  |             |  | - Evidence    |  |           |  |                 | |
|  |             |  | - Timeline    |  |           |  |                 | |
|  |             |  | - MCTS        |  |           |  |                 | |
|  |             |  | - Counterfact.|  |           |  |                 | |
|  +-------------+  +---------------+  +-----------+  +-----------------+ |
|                                                                         |
|  +-------------------------------------------------------------------+ |
|  |                          Presets Module                            | |
|  |  - Registry (built-ins)   - Executor (workflow orchestration)      | |
|  |  - Types (preset/step)    - Builtins (code-review, debug, etc.)    | |
|  +-------------------------------------------------------------------+ |
|                                                                         |
|  +-------------------------------------------------------------------+ |
|  |                    Self-Improvement System                         | |
|  |  ┌─────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐             | |
|  |  │ Monitor │─▶│ Analyzer │─▶│ Executor │─▶│ Learner │──┐          | |
|  |  └────┬────┘  └──────────┘  └──────────┘  └─────────┘  │          | |
|  |       └────────────────────────────────────────────────┘          | |
|  |  - Circuit Breaker   - Action Allowlist   - Auto-Rollback         | |
|  +-------------------------------------------------------------------+ |
|                           |                |                  |         |
|                           v                v                  |         |
|                    +---------------------------+              |         |
|                    |         SQLite DB          |              |         |
|                    | (sessions, thoughts,       |              |         |
|                    |  branches, checkpoints,    |              |         |
|                    |  graphs, invocations,      |              |         |
|                    |  metrics, actions,         |              |         |
|                    |  timelines, mcts_nodes,    |              |         |
|                    |  counterfactual_analyses)  |              |         |
|                    +---------------------------+              |         |
+------------------------------------------------------------- | --------+
                                                               |
                                                               | HTTPS
                                                               v
                                                 +-------------------------+
                                                 |     Langbase API        |
                                                 |                         |
                                                 |  +---------------------+  |
                                                 |  | linear-reasoning-v1 |  |
                                                 |  | tree-reasoning-v1   |  |
                                                 |  | divergent-v1        |  |
                                                 |  | reflection-v1       |  |
                                                 |  | mode-router-v1      |  |
                                                 |  | got-reasoning-v1    |  |
                                                 |  | detection-v1        |  |
                                                 |  | decision-framework  |  |
                                                 |  +---------------------+  |
                                                 |    (8 consolidated pipes) |
                                                 +---------------------------+
```

## Module Structure

```
src/
├── main.rs              # Entry point, runtime setup
├── lib.rs               # Public API exports
├── config/
│   └── mod.rs           # Configuration loading from env
├── error/
│   └── mod.rs           # Error types (App, Storage, Langbase, MCP, Tool)
├── langbase/
│   ├── mod.rs           # Module exports
│   ├── client.rs        # HTTP client with retry logic
│   └── types.rs         # Request/response types
├── modes/
│   ├── mod.rs           # Mode exports, ReasoningMode enum
│   ├── core.rs          # ModeCore shared infrastructure
│   ├── linear.rs        # Linear reasoning implementation
│   ├── tree.rs          # Tree/branching reasoning
│   ├── divergent.rs     # Divergent/creative reasoning
│   ├── reflection.rs    # Meta-cognitive reflection
│   ├── backtracking.rs  # Checkpoint and backtrack
│   ├── auto.rs          # Automatic mode selection
│   ├── got.rs           # Graph-of-Thoughts operations
│   ├── detection.rs     # Bias and fallacy detection
│   ├── decision.rs      # Multi-criteria decision framework
│   ├── evidence.rs      # Evidence assessment and Bayesian updates
│   ├── timeline.rs      # Timeline-based temporal navigation
│   ├── mcts.rs          # Monte Carlo Tree Search exploration
│   └── counterfactual.rs # "What if?" counterfactual analysis
├── presets/
│   ├── mod.rs           # Module exports
│   ├── types.rs         # WorkflowPreset, PresetStep types
│   ├── registry.rs      # Preset registration and lookup
│   ├── builtins.rs      # Built-in preset definitions
│   └── executor.rs      # Workflow execution engine
├── prompts.rs           # Centralized system prompts
├── server/
│   ├── mod.rs           # AppState, SharedState
│   ├── mcp.rs           # JSON-RPC protocol handling
│   └── handlers.rs      # Tool call routing
└── storage/
    ├── mod.rs           # Storage trait, domain types
    └── sqlite.rs        # SQLite implementation

self_improvement/
├── mod.rs               # Module exports and re-exports
├── system.rs            # Main orchestrator (4-phase loop)
├── monitor.rs           # Phase 1: Metrics collection and anomaly detection
├── analyzer.rs          # Phase 2: Root cause diagnosis via Langbase
├── executor.rs          # Phase 3: Safe action execution with rollback
├── learner.rs           # Phase 4: Reward calculation and learning
├── types.rs             # Core types (Diagnosis, Action, Reward, etc.)
├── config.rs            # Configuration from environment
├── allowlist.rs         # Action validation and bounds
├── baseline.rs          # EMA + rolling baseline calculation
├── circuit_breaker.rs   # Failure protection pattern
├── pipes.rs             # Langbase pipe integrations
├── storage.rs           # SQLite persistence for actions
└── cli.rs               # CLI command handlers

tests/
├── config_env_test.rs   # Configuration tests
├── integration_test.rs  # Mode integration tests
├── langbase_test.rs     # HTTP client tests with mocks
├── mcp_protocol_test.rs # JSON-RPC compliance tests
├── modes_test.rs        # Mode-specific tests
├── storage_test.rs      # SQLite integration tests
├── self_improvement_test.rs           # Core self-improvement tests
├── self_improvement_types_test.rs     # Type definition tests
├── self_improvement_pipes_test.rs     # Pipe integration tests
└── self_improvement_integration_test.rs # Full loop tests

migrations/
├── 20240101000001_initial_schema.sql       # Sessions, thoughts, invocations
├── 20240102000001_branches_checkpoints.sql # Branches, checkpoints, snapshots
├── 20240103000001_graphs.sql               # Graph nodes and edges
└── 20240110000001_time_machine.sql         # Timelines, MCTS nodes, counterfactuals
```

## Component Details

### MCP Layer (server/mcp.rs)

Handles JSON-RPC 2.0 protocol over async stdio.

Responsibilities:
- Parse incoming JSON-RPC requests
- Route to appropriate handlers
- Serialize responses
- Handle protocol-level errors
- Handle notifications (no response for notifications per JSON-RPC 2.0)

Key Types:
```rust
pub struct McpServer { state: SharedState }
pub struct JsonRpcRequest { jsonrpc, id, method, params }
pub struct JsonRpcResponse { jsonrpc, id, result, error }
```

Supported Methods:
| Method | Description |
|--------|-------------|
| `initialize` | MCP handshake |
| `initialized` | Acknowledge init (notification) |
| `tools/list` | List available tools |
| `tools/call` | Execute a tool |
| `ping` | Health check |

### Tool Handlers (server/handlers.rs)

Routes tool calls to mode implementations.

```rust
pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value>
```

Tool routing:
- `reasoning_linear` -> LinearMode
- `reasoning_tree`, `reasoning_tree_*` -> TreeMode
- `reasoning_divergent` -> DivergentMode
- `reasoning_reflection`, `reasoning_reflection_*` -> ReflectionMode
- `reasoning_backtrack`, `reasoning_checkpoint_*` -> BacktrackingMode
- `reasoning_auto` -> AutoMode
- `reasoning_got_*` -> GotMode
- `reasoning_detect_biases`, `reasoning_detect_fallacies` -> DetectionMode
- `reasoning_make_decision`, `reasoning_analyze_perspectives` -> DecisionMode
- `reasoning_assess_evidence`, `reasoning_probabilistic` -> EvidenceMode
- `reasoning_preset_list`, `reasoning_preset_run` -> PresetRegistry/Executor
- `reasoning_timeline_*` -> TimelineMode
- `reasoning_mcts_explore`, `reasoning_auto_backtrack` -> MCTSMode
- `reasoning_counterfactual` -> CounterfactualMode

### Reasoning Modes (modes/)

Each mode implements a specific reasoning pattern.

#### Linear Mode (modes/linear.rs)
- Single-pass sequential reasoning
- Builds on previous thoughts in session
- Returns structured JSON output

```rust
pub struct LinearMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}
```

#### Tree Mode (modes/tree.rs)
- Branching exploration with multiple paths
- Branch management (focus, list, complete)
- Tracks branch state and confidence

```rust
pub struct TreeMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}
```

#### Divergent Mode (modes/divergent.rs)
- Creative reasoning with multiple perspectives
- Generates novel viewpoints
- Synthesizes diverse insights

#### Reflection Mode (modes/reflection.rs)
- Meta-cognitive analysis
- Evaluates reasoning quality
- Provides improvement recommendations

#### Backtracking Mode (modes/backtracking.rs)
- Checkpoint creation and management
- State restoration
- Alternative path exploration

#### Auto Mode (modes/auto.rs)
- Analyzes content for mode selection
- Local heuristics for common patterns
- Langbase-powered routing for complex cases

#### Graph-of-Thoughts Mode (modes/got.rs)
- Graph-based reasoning structure
- Node generation, scoring, aggregation
- Pruning and refinement operations
- Cycle detection and finalization

```rust
pub struct GotMode {
    core: ModeCore,  // Shared infrastructure
    generate_pipe: String,
    score_pipe: String,
    aggregate_pipe: String,
    refine_pipe: String,
}
```

#### Detection Mode (modes/detection.rs)
- Cognitive bias detection (confirmation, anchoring, availability, etc.)
- Logical fallacy detection (formal and informal)
- Severity scoring and remediation suggestions
- Persisted detection results for auditing

#### Decision Mode (modes/decision.rs)
- Multi-criteria decision analysis (weighted sum, pairwise, TOPSIS)
- Stakeholder perspective analysis with power/interest matrix
- Trade-off identification and sensitivity analysis
- Persisted decisions with rankings and rationale

#### Evidence Mode (modes/evidence.rs)
- Evidence quality assessment with source credibility
- Corroboration tracking across evidence items
- Bayesian probability updates with entropy metrics
- Persisted assessments and probability chains

#### Timeline Mode (modes/timeline.rs)
- Timeline-based temporal navigation through reasoning
- Branch creation from any checkpoint
- Multi-branch comparison and ranking
- Branch merging with multiple strategies (best_of, synthesize, consensus, weighted)
- Persisted timelines with branch tree structure

```rust
pub struct TimelineMode {
    core: ModeCore,
    pipe_name: String,
}
```

#### MCTS Mode (modes/mcts.rs)
- Monte Carlo Tree Search for intelligent path exploration
- UCB1 selection balancing exploration vs exploitation
- Multiple expansion strategies (best_first, breadth_first, random, diverse)
- Self-backtracking with quality threshold assessment
- Automatic alternative path generation

```rust
pub struct MCTSMode {
    core: ModeCore,
    exploration_constant: f64,  // Default: √2 ≈ 1.414
    max_depth: i32,
}
```

UCB1 Formula:
```
UCB1(s,a) = Q(s,a) + c × √(ln(N_parent) / N(s,a))
```

#### Counterfactual Mode (modes/counterfactual.rs)
- "What if?" analysis on past reasoning decisions
- Multiple intervention types (change, remove, replace, inject)
- Causal attribution scoring with Pearl's Ladder of Causation
- Counterfactual outcome prediction with confidence
- Causal chain tracing and confounder identification

```rust
pub struct CounterfactualMode {
    core: ModeCore,
    pipe_name: String,
}
```

### ModeCore Architecture (modes/core.rs)

All reasoning modes share common infrastructure via composition:

```rust
pub struct ModeCore {
    storage: SqliteStorage,
    langbase: LangbaseClient,
}

impl ModeCore {
    pub fn storage(&self) -> &SqliteStorage;
    pub fn langbase(&self) -> &LangbaseClient;
}
```

Benefits:
- Eliminates field duplication across 13 mode structs
- Consistent storage and Langbase access patterns
- Simplified initialization and dependency injection
- Single point for cross-cutting concerns

### Presets Module (presets/)

Composable multi-step reasoning workflows.

```rust
pub struct WorkflowPreset {
    id: String,
    name: String,
    description: String,
    category: String,
    steps: Vec<PresetStep>,
    input_schema: HashMap<String, InputField>,
}

pub struct PresetStep {
    id: String,
    tool: String,           // MCP tool to invoke
    input_map: InputMap,    // Maps inputs to tool params
    store_as: Option<String>, // Key for result storage
    depends_on: Vec<String>,  // Step dependencies
    condition: Option<StepCondition>,
    optional: bool,
}
```

Built-in Presets:
| ID | Category | Steps | Description |
|----|----------|-------|-------------|
| `code-review` | code | 4 | Divergent → Bias → Fallacy → Reflection |
| `debug-analysis` | code | 4 | Linear → Tree → Checkpoint → Reflection |
| `architecture-decision` | architecture | 5 | Divergent → GoT workflow |
| `strategic-decision` | decision | 4 | Decision → Perspectives → Bias → Synthesis |
| `evidence-based-conclusion` | research | 4 | Evidence → Bayesian → Fallacy → Reflection |

### Langbase Client (langbase/client.rs)

HTTP client for Langbase Pipes API with retry logic.

Features:
- Configurable timeout and retries
- Exponential backoff
- Request/response logging
- Pipe creation and management

```rust
pub struct LangbaseClient {
    client: Client,
    base_url: String,
    api_key: String,
    request_config: RequestConfig,
}

impl LangbaseClient {
    pub async fn call_pipe(&self, request: PipeRequest) -> LangbaseResult<PipeResponse>;
    pub async fn create_pipe(&self, request: CreatePipeRequest) -> LangbaseResult<CreatePipeResponse>;
}
```

### Storage Layer (storage/)

SQLite-backed persistence with compile-time migrations.

Trait Definition:
```rust
#[async_trait]
pub trait Storage: Send + Sync {
    // Sessions
    async fn create_session(&self, session: &Session) -> StorageResult<()>;
    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>>;
    async fn update_session(&self, session: &Session) -> StorageResult<()>;
    async fn delete_session(&self, id: &str) -> StorageResult<()>;

    // Thoughts
    async fn create_thought(&self, thought: &Thought) -> StorageResult<()>;
    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>>;
    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>>;
    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>>;

    // Branches
    async fn create_branch(&self, branch: &Branch) -> StorageResult<()>;
    async fn get_branch(&self, id: &str) -> StorageResult<Option<Branch>>;
    async fn get_session_branches(&self, session_id: &str) -> StorageResult<Vec<Branch>>;
    async fn update_branch(&self, branch: &Branch) -> StorageResult<()>;

    // Checkpoints
    async fn create_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;
    async fn get_checkpoint(&self, id: &str) -> StorageResult<Option<Checkpoint>>;
    async fn get_session_checkpoints(&self, session_id: &str) -> StorageResult<Vec<Checkpoint>>;

    // Snapshots
    async fn create_snapshot(&self, snapshot: &StateSnapshot) -> StorageResult<()>;

    // Graph nodes and edges
    async fn create_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    async fn get_graph_node(&self, id: &str) -> StorageResult<Option<GraphNode>>;
    async fn get_graph_nodes(&self, graph_id: &str) -> StorageResult<Vec<GraphNode>>;
    async fn update_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    async fn delete_graph_node(&self, id: &str) -> StorageResult<()>;
    async fn create_graph_edge(&self, edge: &GraphEdge) -> StorageResult<()>;
    async fn get_graph_edges(&self, graph_id: &str) -> StorageResult<Vec<GraphEdge>>;

    // Invocations
    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;
}
```

Database Schema:
```sql
-- Sessions: reasoning context groupings
sessions (id, mode, created_at, updated_at, metadata, active_branch_id)

-- Thoughts: individual reasoning steps
thoughts (id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> thoughts.id (SET NULL)
  FK: branch_id -> branches.id (SET NULL)

-- Branches: tree mode exploration paths
branches (id, session_id, name, parent_id, state, confidence, priority, created_at, updated_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> branches.id (SET NULL)

-- Cross-references: branch relationships
cross_refs (id, from_branch, to_branch, ref_type, reason, strength, created_at)
  FK: from_branch -> branches.id (CASCADE DELETE)
  FK: to_branch -> branches.id (CASCADE DELETE)

-- Checkpoints: saved states for backtracking
checkpoints (id, session_id, branch_id, name, description, snapshot, created_at)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: branch_id -> branches.id (SET NULL)

-- State snapshots: detailed state captures
state_snapshots (id, session_id, parent_id, snapshot_type, description, data, created_at)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> state_snapshots.id (SET NULL)

-- Graph nodes: GoT reasoning nodes
graph_nodes (id, graph_id, session_id, content, node_type, score, depth, is_active, is_terminal, created_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)

-- Graph edges: GoT node relationships
graph_edges (id, graph_id, from_node, to_node, edge_type, weight, created_at, metadata)
  FK: from_node -> graph_nodes.id (CASCADE DELETE)
  FK: to_node -> graph_nodes.id (CASCADE DELETE)

-- Invocations: API call audit log
invocations (id, session_id, tool_name, input, output, pipe_name, latency_ms, success, error, created_at)
  FK: session_id -> sessions.id (SET NULL)

-- Self-improvement action records
si_actions (id, diagnosis_id, action_type, action_data, pre_metrics, post_metrics, outcome, reward, executed_at, verified_at)

-- Action effectiveness history
si_effectiveness (id, action_signature, successful_attempts, total_attempts, avg_reward, last_updated)
```

### Self-Improvement System (self_improvement/)

Autonomous 4-phase optimization loop with multiple safety layers.

#### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                    SelfImprovementSystem                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │ Monitor  │──│ Analyzer │──│ Executor │──│ Learner  │            │
│  │          │  │          │  │          │  │          │            │
│  │ - Metrics│  │ - Diag   │  │ - Valid  │  │ - Reward │            │
│  │ - Base   │  │ - Action │  │ - Execute│  │ - Effect │            │
│  │ - Trigger│  │ - Detect │  │ - Verify │  │ - Synth  │            │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘            │
│       │                                          │                  │
│       └──────────────────────────────────────────┘                  │
│                           │                                          │
│  ┌───────────────────────┴───────────────────────────────────┐     │
│  │                    Shared Components                       │     │
│  │  CircuitBreaker │ ActionAllowlist │ Storage │ Pipes       │     │
│  └────────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────┘
```

#### Phase 1: Monitor (monitor.rs)

Collects system metrics and detects anomalies.

```rust
pub struct Monitor {
    config: SelfImprovementConfig,
    metrics: Arc<RwLock<Vec<RawMetrics>>>,
    baselines: Arc<RwLock<BaselineCollection>>,
}

impl Monitor {
    pub async fn record_invocation(&self, event: InvocationEvent);
    pub async fn check_health(&self) -> Option<HealthReport>;
    pub async fn get_baselines(&self) -> Baselines;
}
```

Key Types:
- `RawMetrics`: Individual invocation metrics
- `AggregatedMetrics`: Windowed aggregations
- `HealthReport`: Current health with triggers
- `TriggerMetric`: ErrorRate, Latency, QualityScore variants

Baseline Calculation:
- Hybrid EMA (Exponential Moving Average) + rolling window
- Configurable warmup period before triggering
- Automatic threshold multipliers (warning: 1.5x, critical: 2x)

#### Phase 2: Analyzer (analyzer.rs)

Diagnoses root causes and recommends actions using Langbase pipes.

```rust
pub struct Analyzer {
    config: SelfImprovementConfig,
    pipes: Arc<SelfImprovementPipes>,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    pending: Arc<RwLock<Vec<SelfDiagnosis>>>,
}

impl Analyzer {
    pub async fn analyze(&self, report: &HealthReport) -> Result<AnalysisResult, AnalysisBlocked>;
    pub async fn pending_count(&self) -> usize;
}
```

Key Types:
- `SelfDiagnosis`: Root cause with suggested action
- `SuggestedAction`: AdjustParam, ToggleFeature, ScaleResource, ClearCache, NoOp
- `Severity`: Info, Warning, High, Critical

Uses Langbase Pipes:
- `reflection-v1`: Root cause analysis
- `decision-framework-v1`: Action selection
- `detection-v1`: Bias/fallacy validation

#### Phase 3: Executor (executor.rs)

Validates and executes actions with safety controls.

```rust
pub struct Executor {
    config: SelfImprovementConfig,
    allowlist: ActionAllowlist,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    pending: Arc<RwLock<Option<ExecutionResult>>>,
    history: Arc<RwLock<Vec<ExecutionResult>>>,
}

impl Executor {
    pub async fn execute(&self, diagnosis: &SelfDiagnosis, metrics: &MetricsSnapshot)
        -> Result<ExecutionResult, ExecutionBlocked>;
    pub async fn verify_and_complete(&self, metrics: &MetricsSnapshot, baselines: &Baselines)
        -> Option<ExecutionResult>;
    pub async fn force_rollback(&self, reason: &str) -> Option<ExecutionResult>;
}
```

Key Types:
- `ExecutionResult`: Full execution record with pre/post state
- `ExecutionBlocked`: CircuitOpen, NotAllowed, CooldownActive, RateLimitExceeded, NoOpAction
- `ConfigState`: Snapshot of current configuration

Safety Mechanisms:
- Allowlist validation (bounds, step limits)
- Rate limiting (max actions per hour)
- Cooldown enforcement
- Automatic rollback on regression

#### Phase 4: Learner (learner.rs)

Calculates rewards and tracks action effectiveness.

```rust
pub struct Learner {
    config: SelfImprovementConfig,
    pipes: Arc<SelfImprovementPipes>,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    effectiveness_history: Arc<RwLock<HashMap<String, ActionEffectiveness>>>,
}

impl Learner {
    pub async fn learn(&self, execution: &ExecutionResult, diagnosis: &SelfDiagnosis,
        post_metrics: &MetricsSnapshot, baselines: &Baselines)
        -> Result<LearningOutcome, LearningBlocked>;
    pub async fn get_effectiveness_for_action(&self, action: &SuggestedAction) -> Option<f64>;
}
```

Key Types:
- `NormalizedReward`: Combined score from error, latency, quality deltas
- `RewardBreakdown`: Individual component contributions
- `LearningOutcome`: Reward, effectiveness, synthesis
- `ActionEffectiveness`: Historical success rate

Reward Calculation:
```
reward = w_error × Δ_error + w_latency × Δ_latency + w_quality × Δ_quality
```
Where Δ values are normalized improvements (-1 to +1).

#### Circuit Breaker (circuit_breaker.rs)

Failure protection pattern implementation.

```rust
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitState,
    failure_count: u32,
    last_failure: Option<DateTime<Utc>>,
    opened_at: Option<DateTime<Utc>>,
}

pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Blocking all actions
    HalfOpen,  // Testing recovery
}
```

Behavior:
- Opens after N consecutive failures
- Stays open for configurable duration
- Half-open allows single test action
- Success in half-open closes circuit

#### Action Allowlist (allowlist.rs)

Validates actions against pre-defined bounds.

```rust
pub struct ActionAllowlist {
    adjustable_params: HashMap<String, ParamBounds>,
    toggleable_features: HashSet<String>,
    scalable_resources: HashMap<ResourceType, ResourceBounds>,
}

pub struct ParamBounds {
    current_value: ParamValue,
    min: ParamValue,
    max: ParamValue,
    step: ParamValue,
    description: String,
}
```

Validations:
- Parameter within min/max range
- Step size within allowed increment
- Feature in toggleable set
- Resource within scaling bounds

#### Pipes Integration (pipes.rs)

Langbase pipe wrappers for AI-assisted analysis.

```rust
pub struct SelfImprovementPipes {
    langbase: Arc<LangbaseClient>,
    config: SelfImprovementPipeConfig,
}

impl SelfImprovementPipes {
    pub async fn generate_diagnosis(&self, report: &HealthReport, trigger: &TriggerMetric)
        -> Result<DiagnosisResponse, PipeError>;
    pub async fn select_action(&self, diagnosis: &SelfDiagnosis, allowlist: &ActionAllowlist)
        -> Result<ActionSelectionResponse, PipeError>;
    pub async fn validate_decision(&self, diagnosis: &SelfDiagnosis)
        -> Result<ValidationResponse, PipeError>;
    pub async fn synthesize_learning(&self, execution: &ExecutionResult, diagnosis: &SelfDiagnosis)
        -> Result<LearningResponse, PipeError>;
}
```

#### Storage (storage.rs)

SQLite persistence for self-improvement data.

```rust
pub struct SelfImprovementStorage {
    pool: SqlitePool,
}

impl SelfImprovementStorage {
    pub async fn save_action(&self, action: &ActionRecord) -> Result<(), StorageError>;
    pub async fn get_action_history(&self, limit: usize) -> Result<Vec<ActionRecord>, StorageError>;
    pub async fn save_effectiveness(&self, record: &ActionEffectivenessRecord) -> Result<(), StorageError>;
    pub async fn get_effectiveness(&self, signature: &str) -> Result<Option<ActionEffectivenessRecord>, StorageError>;
}
```

#### CLI (cli.rs)

Command-line interface for system management.

```rust
pub enum SelfImproveCommands {
    Status,
    History { limit: Option<usize> },
    Enable,
    Disable,
    Pause { duration: String },
    Resume,
    Config,
    Check,
    Rollback,
    Reset,
}

pub async fn execute_command(cmd: SelfImproveCommands, config: &Config) -> CliResult<String>;
```

### Prompts (prompts.rs)

Centralized system prompts for all reasoning modes.

```rust
pub const LINEAR_REASONING_PROMPT: &str = r#"..."#;
pub const TREE_REASONING_PROMPT: &str = r#"..."#;
pub const DIVERGENT_REASONING_PROMPT: &str = r#"..."#;
pub const REFLECTION_PROMPT: &str = r#"..."#;
pub const AUTO_ROUTER_PROMPT: &str = r#"..."#;
pub const BACKTRACKING_PROMPT: &str = r#"..."#;
pub const GOT_GENERATE_PROMPT: &str = r#"..."#;
pub const GOT_SCORE_PROMPT: &str = r#"..."#;
pub const GOT_AGGREGATE_PROMPT: &str = r#"..."#;
pub const GOT_REFINE_PROMPT: &str = r#"..."#;

pub fn get_prompt_for_mode(mode: &str) -> &'static str;
```

### Error Handling (error/mod.rs)

Hierarchical error types with conversions.

```
AppError (top-level)
├── Config { message }
├── Storage(StorageError)
│   ├── Connection { message }
│   ├── Query { message }
│   ├── SessionNotFound { session_id }
│   ├── ThoughtNotFound { thought_id }
│   ├── BranchNotFound { branch_id }
│   ├── CheckpointNotFound { checkpoint_id }
│   ├── Migration { message }
│   └── Sqlx(sqlx::Error)
├── Langbase(LangbaseError)
│   ├── Unavailable { message, retries }
│   ├── Api { status, message }
│   ├── InvalidResponse { message }
│   ├── Timeout { timeout_ms }
│   └── Http(reqwest::Error)
├── Mcp(McpError)
│   ├── InvalidRequest { message }
│   ├── UnknownTool { tool_name }
│   ├── InvalidParameters { tool_name, message }
│   ├── ExecutionFailed { message }
│   └── Json(serde_json::Error)
└── Internal { message }

ToolError (tool-specific)
├── Validation { field, reason }
├── Session(String)
├── Branch(String)
├── Checkpoint(String)
├── Graph(String)
└── Reasoning { message }
```

## Data Flow

### Tool Call Flow

```
1. MCP Client sends JSON-RPC request over stdio
2. McpServer::run() reads line asynchronously
3. Parse as JsonRpcRequest
4. Check if notification (no id) - if so, process without response
5. Route to handle_request() by method
6. For tools/call:
   a. Parse ToolCallParams
   b. Route to handle_tool_call() by tool name
   c. Deserialize arguments to mode-specific params
   d. Call mode.process()
7. Mode processing:
   a. Validate input
   b. Get/create session from storage
   c. Load previous thoughts/branches for context
   d. Build messages array
   e. Call Langbase pipe
   f. Parse reasoning response
   g. Store thought/branch/node in SQLite
   h. Log invocation
   i. Return structured result
8. Serialize result to JSON-RPC response
9. Write to stdout with newline
10. Flush stdout
```

### Session Continuity

```
First call (no session_id):
  ├── Create new Session with UUID
  ├── Store in SQLite
  └── Return session_id in response

Subsequent calls (with session_id):
  ├── Load Session from SQLite
  ├── Load previous Thoughts/Branches
  ├── Include in Langbase context
  └── Link new Thought to session
```

### Graph-of-Thoughts Flow

```
1. Initialize graph (got_init):
   ├── Create session if needed
   ├── Create root GraphNode
   └── Return graph_id

2. Generate continuations (got_generate):
   ├── Load source node
   ├── Call Langbase for k continuations
   ├── Create child GraphNodes
   ├── Create GraphEdges
   └── Return new node IDs

3. Score nodes (got_score):
   ├── Load node content
   ├── Call Langbase for quality assessment
   ├── Update node score
   └── Return breakdown

4. Aggregate nodes (got_aggregate):
   ├── Load source nodes
   ├── Call Langbase for synthesis
   ├── Create aggregation node
   └── Create aggregation edges

5. Refine node (got_refine):
   ├── Load node
   ├── Call Langbase for improvement
   ├── Create refinement node
   └── Create refine edge

6. Prune low-scoring (got_prune):
   ├── Load all nodes
   ├── Filter by threshold
   ├── Delete below-threshold nodes
   └── Return pruned count

7. Finalize (got_finalize):
   ├── Mark terminal nodes
   ├── Collect conclusions
   └── Return final insights
```

## Configuration

Configuration is loaded from environment variables at startup:

```rust
pub struct Config {
    pub langbase: LangbaseConfig,   // API key, base URL
    pub database: DatabaseConfig,    // Path, max connections
    pub logging: LoggingConfig,      // Level, format
    pub request: RequestConfig,      // Timeout, retries
    pub pipes: PipeConfig,           // Pipe names per mode
    pub got_pipes: GotPipeConfig,    // GoT-specific pipes
}
```

## Concurrency Model

- Single async Tokio runtime
- Async stdio for non-blocking I/O
- SQLite connection pool (default 5 connections)
- HTTP client with connection pooling
- Shared state via `Arc<AppState>`

## Security Considerations

- API key stored in environment variable only
- No secrets logged (tracing configured appropriately)
- SQLite file permissions follow filesystem defaults
- HTTPS required for Langbase API
- Input validation before processing

## Testing Strategy

| Layer | Test Type | Location |
|-------|-----------|----------|
| MCP Protocol | Unit/Integration | `tests/mcp_protocol_test.rs` |
| Storage | Integration | `tests/storage_test.rs` |
| Langbase Client | Integration (mocked) | `tests/langbase_test.rs` |
| Config | Unit | `tests/config_env_test.rs` |
| Modes | Unit/Integration | `tests/modes_test.rs`, `tests/integration_test.rs` |
| Presets | Unit/Integration | `src/presets/` (inline), `tests/` |
| Detection | Unit | `src/modes/detection.rs` (inline) |
| Decision/Evidence | Unit | `src/modes/{decision,evidence}.rs` (inline) |
| Prompts | Unit | `src/prompts.rs` (inline) |

Total test count: 2000+ tests across all modules.
