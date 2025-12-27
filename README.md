# mcp-langbase-reasoning

A Model Context Protocol (MCP) server providing structured reasoning capabilities powered by Langbase Pipes. Written in Rust with SQLite persistence.

## Features

- **12 Reasoning Modes** - Linear, tree, divergent, reflection, backtracking, auto-selection, Graph-of-Thoughts, decision framework, evidence assessment, timeline, MCTS, and counterfactual
- **Reasoning Time Machine** - Timeline-based exploration, MCTS-guided search, counterfactual "what if" analysis
- **5 Workflow Presets** - Code review, debugging, architecture decisions, strategic decisions, and evidence-based conclusions
- **Cognitive Analysis** - Bias detection and logical fallacy identification
- **Autonomous Self-Improvement** - 4-phase optimization loop with safety controls (Monitor → Analyzer → Executor → Learner)
- **Session Persistence** - SQLite storage for sessions, thoughts, branches, checkpoints, and timelines
- **Production Ready** - 2000+ tests, async I/O, retry logic, structured error handling

## Quick Start

### Prerequisites

- Rust 1.70+
- Langbase account with API key

### Installation

```bash
git clone https://github.com/quanticsoul4772/mcp-langbase-reasoning.git
cd mcp-langbase-reasoning

# Configure environment
cp .env.example .env
# Edit .env: set LANGBASE_API_KEY=your_key

# Build
cargo build --release
```

### Configure MCP Client

Add to Claude Desktop configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "langbase-reasoning": {
      "command": "/path/to/mcp-langbase-reasoning",
      "env": {
        "LANGBASE_API_KEY": "your_api_key"
      }
    }
  }
}
```

## Available Tools

### Core Reasoning

| Tool | Description |
|------|-------------|
| `reasoning_linear` | Sequential step-by-step reasoning |
| `reasoning_tree` | Branching exploration with 2-4 paths |
| `reasoning_divergent` | Creative multi-perspective reasoning |
| `reasoning_reflection` | Meta-cognitive analysis and improvement |
| `reasoning_auto` | Automatic mode selection |

### Tree Navigation

| Tool | Description |
|------|-------------|
| `reasoning_tree_focus` | Focus on a specific branch |
| `reasoning_tree_list` | List all branches |
| `reasoning_tree_complete` | Mark branch as completed/abandoned |

### Checkpoints

| Tool | Description |
|------|-------------|
| `reasoning_checkpoint_create` | Save reasoning state |
| `reasoning_checkpoint_list` | List checkpoints |
| `reasoning_backtrack` | Restore and explore alternatives |

### Graph-of-Thoughts

| Tool | Description |
|------|-------------|
| `reasoning_got_init` | Initialize reasoning graph |
| `reasoning_got_generate` | Generate diverse continuations |
| `reasoning_got_score` | Score node quality |
| `reasoning_got_aggregate` | Merge nodes into insight |
| `reasoning_got_refine` | Improve through self-critique |
| `reasoning_got_prune` | Remove low-scoring nodes |
| `reasoning_got_finalize` | Extract conclusions |
| `reasoning_got_state` | Get graph structure |

### Decision & Evidence

| Tool | Description |
|------|-------------|
| `reasoning_make_decision` | Multi-criteria decision analysis |
| `reasoning_analyze_perspectives` | Stakeholder analysis |
| `reasoning_assess_evidence` | Evidence quality assessment |
| `reasoning_probabilistic` | Bayesian probability updates |

### Time Machine

| Tool | Description |
|------|-------------|
| `reasoning_timeline_create` | Create a new reasoning timeline |
| `reasoning_timeline_branch` | Branch from any checkpoint |
| `reasoning_timeline_compare` | Compare outcomes across branches |
| `reasoning_timeline_merge` | Merge insights from multiple branches |
| `reasoning_mcts_explore` | MCTS-guided exploration with UCB balancing |
| `reasoning_auto_backtrack` | Self-backtracking with quality assessment |
| `reasoning_counterfactual` | "What if?" analysis on past reasoning |

### Cognitive Analysis

| Tool | Description |
|------|-------------|
| `reasoning_detect_biases` | Identify cognitive biases |
| `reasoning_detect_fallacies` | Detect logical fallacies |

### Workflow Presets

| Tool | Description |
|------|-------------|
| `reasoning_preset_list` | List available presets |
| `reasoning_preset_run` | Execute workflow preset |

**Built-in Presets:** `code-review`, `debug-analysis`, `architecture-decision`, `strategic-decision`, `evidence-based-conclusion`

## Configuration

### Required

| Variable | Description |
|----------|-------------|
| `LANGBASE_API_KEY` | Your Langbase API key |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_PATH` | `./data/reasoning.db` | SQLite location |
| `LOG_LEVEL` | `info` | Logging verbosity |
| `REQUEST_TIMEOUT_MS` | `30000` | HTTP timeout |
| `MAX_RETRIES` | `3` | API retry attempts |

## Self-Improvement System

The server includes an autonomous self-improvement loop that monitors system health, diagnoses issues, executes safe optimizations, and learns from outcomes. See the [Architecture](#architecture) diagram for the system overview.

| Phase | Function |
|-------|----------|
| **Monitor** | Collects metrics (error rate, latency, quality), maintains baselines, detects anomalies |
| **Analyzer** | Uses Langbase pipes to diagnose root causes, generates action recommendations |
| **Executor** | Validates actions against allowlist, executes safely, monitors for regressions |
| **Learner** | Calculates rewards, tracks action effectiveness, synthesizes lessons |

### Safety Features

- **Disabled by Default** - Set `SELF_IMPROVEMENT_ENABLED=true` to activate
- **Circuit Breaker** - Stops after consecutive failures (default: 5)
- **Action Allowlist** - Only bounded, pre-approved parameter changes allowed
- **Rate Limiting** - Maximum actions per hour (default: 10)
- **Automatic Rollback** - Reverts changes that cause regression
- **Cooldown Periods** - Minimum time between actions (default: 60s)
- **AI Validation** - Bias and fallacy detection on decisions

### Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `SELF_IMPROVEMENT_ENABLED` | `false` | Enable the self-improvement loop |
| `SI_MAX_ACTIONS_PER_HOUR` | `10` | Maximum actions per hour |
| `SI_COOLDOWN_SECS` | `60` | Cooldown between actions |
| `SI_REQUIRE_APPROVAL` | `false` | Require manual approval |
| `SI_ROLLBACK_ON_REGRESSION` | `true` | Auto-rollback on degradation |
| `SI_CIRCUIT_BREAKER_THRESHOLD` | `5` | Failures before circuit opens |
| `SI_ERROR_RATE_THRESHOLD` | `0.1` | Error rate trigger (10%) |
| `SI_LATENCY_THRESHOLD_MS` | `5000` | P95 latency trigger |
| `SI_QUALITY_THRESHOLD` | `0.7` | Quality score minimum |

### Allowed Actions

The system can only adjust pre-defined safe parameters:

| Parameter | Range | Max Step |
|-----------|-------|----------|
| `REQUEST_TIMEOUT_MS` | 5,000-60,000 | 5,000 |
| `MAX_RETRIES` | 1-10 | 2 |
| `RETRY_DELAY_MS` | 100-5,000 | 500 |
| `REFLECTION_QUALITY_THRESHOLD` | 0.5-0.95 | 0.05 |
| `MAX_CONCURRENT_REQUESTS` | 1-20 | 2 |
| `CONNECTION_POOL_SIZE` | 1-50 | 5 |

### CLI Commands

```bash
# View system status
cargo run -- self-improve status

# View action history
cargo run -- self-improve history

# Enable/disable the system
cargo run -- self-improve enable
cargo run -- self-improve disable

# Pause temporarily
cargo run -- self-improve pause --duration 1h

# View current configuration
cargo run -- self-improve config

# Force a health check
cargo run -- self-improve check

# Rollback last action
cargo run -- self-improve rollback
```

### Example Output

```
$ cargo run -- self-improve status

Self-Improvement System Status
==============================
Enabled:           true
Circuit State:     closed
Consecutive Fails: 0
In Cooldown:       false
Actions This Hour: 3/10
Total Cycles:      47
Total Successes:   42
Total Rollbacks:   2
Last Cycle:        2025-12-26T10:30:00Z
```

## Architecture

```
┌─────────────┐     stdio      ┌──────────────────┐     HTTPS     ┌─────────────────┐
│ MCP Client  │◄──────────────►│ mcp-langbase-    │◄─────────────►│ Langbase Pipes  │
│ (Claude)    │   JSON-RPC     │   reasoning      │               │ (8 pipes)       │
└─────────────┘                └────────┬─────────┘               └─────────────────┘
                                        │                                  ▲
                                        │                                  │
                                        ▼                                  │
                               ┌──────────────────┐                        │
                               │     SQLite       │                        │
                               │  sessions,       │                        │
                               │  thoughts,       │                        │
                               │  graphs,         │                        │
                               │  metrics         │                        │
                               └────────┬─────────┘                        │
                                        │                                  │
                    ┌───────────────────┴───────────────────┐              │
                    │         Self-Improvement Loop         │              │
                    │  ┌─────────┐ ┌─────────┐ ┌─────────┐  │   diagnosis  │
                    │  │ Monitor │→│ Analyze │→│ Execute │──┼──────────────┘
                    │  └────┬────┘ └─────────┘ └────┬────┘  │
                    │       │      ┌─────────┐      │       │
                    │       └──────│  Learn  │◄─────┘       │
                    │              └─────────┘              │
                    └───────────────────────────────────────┘
```

## Development

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all 2000+ tests
cargo clippy -- -D warnings  # Lint (0 warnings)
cargo llvm-cov           # Generate coverage report
```

## Project Structure

```
src/
├── main.rs           # Entry point
├── config/           # Environment configuration
├── error/            # Structured error types
├── langbase/         # Langbase API client
├── modes/            # 12 reasoning implementations
│   ├── timeline.rs   # Timeline management
│   ├── mcts.rs       # MCTS exploration
│   └── counterfactual.rs  # "What if" analysis
├── presets/          # Workflow preset system
├── self_improvement/ # Autonomous optimization
├── server/           # MCP protocol handling
└── storage/          # SQLite persistence

docs/
├── API_REFERENCE.md  # Complete tool documentation
├── ARCHITECTURE.md   # Technical design
├── LANGBASE_API.md   # Pipe integration
└── REASONING_TIME_MACHINE.md  # Time Machine design
```

## License

MIT
