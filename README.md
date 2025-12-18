# mcp-langbase-reasoning

A Model Context Protocol (MCP) server that provides structured reasoning capabilities powered by Langbase Pipes. Written in Rust with SQLite persistence for session management.

## Features

- **9 Reasoning Modes**: Linear, tree, divergent, reflection, backtracking, auto-selection, Graph-of-Thoughts, decision framework, and evidence assessment
- **Workflow Presets**: Composable multi-step reasoning workflows for code review, debugging, and architecture decisions
- **Decision & Evidence Tools**: Multi-criteria decision analysis, stakeholder perspectives, evidence assessment, and Bayesian probability updates
- **Cognitive Analysis**: Bias detection and logical fallacy identification tools
- **Session Persistence**: SQLite storage for sessions, thoughts, branches, checkpoints, and graphs
- **MCP Compliant**: Works with Claude Desktop and any MCP-compatible client
- **Production Ready**: Async I/O, retry logic with exponential backoff, structured error handling

## Quick Start

### Prerequisites

- Rust 1.70+
- Langbase account with API key

### Installation

```bash
git clone <repository-url>
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

### Usage

Once configured, reasoning tools are available in Claude:

```
Use reasoning_linear to analyze the trade-offs between microservices and monoliths.
```

```
Use reasoning_tree to explore multiple approaches to this authentication problem.
```

```
Run the code-review preset on this function to check for issues.
```

## Available Tools

### Core Reasoning

| Tool | Description |
|------|-------------|
| `reasoning_linear` | Sequential step-by-step reasoning with session continuity |
| `reasoning_tree` | Branching exploration generating 2-4 alternative paths |
| `reasoning_divergent` | Creative reasoning with multiple novel perspectives |
| `reasoning_reflection` | Meta-cognitive analysis and iterative quality improvement |
| `reasoning_auto` | Automatic mode selection based on content analysis |

### Tree Navigation

| Tool | Description |
|------|-------------|
| `reasoning_tree_focus` | Focus on a specific branch for continued exploration |
| `reasoning_tree_list` | List all branches in a session |
| `reasoning_tree_complete` | Mark a branch as completed or abandoned |

### Checkpoints & Backtracking

| Tool | Description |
|------|-------------|
| `reasoning_checkpoint_create` | Save current reasoning state |
| `reasoning_checkpoint_list` | List available checkpoints |
| `reasoning_backtrack` | Restore from checkpoint and explore alternatives |

### Graph-of-Thoughts

| Tool | Description |
|------|-------------|
| `reasoning_got_init` | Initialize a reasoning graph with root node |
| `reasoning_got_generate` | Generate k diverse continuations from a node |
| `reasoning_got_score` | Score node quality (relevance, validity, depth, novelty) |
| `reasoning_got_aggregate` | Merge multiple nodes into unified insight |
| `reasoning_got_refine` | Improve node through self-critique |
| `reasoning_got_prune` | Remove low-scoring nodes |
| `reasoning_got_finalize` | Mark terminal nodes and extract conclusions |
| `reasoning_got_state` | Get current graph structure and statistics |

### Decision Framework

| Tool | Description |
|------|-------------|
| `reasoning_make_decision` | Multi-criteria decision analysis using weighted scoring, pairwise comparison, or TOPSIS |
| `reasoning_analyze_perspectives` | Stakeholder power/interest matrix analysis with conflict/alignment identification |

### Evidence Assessment

| Tool | Description |
|------|-------------|
| `reasoning_assess_evidence` | Evidence quality assessment with source credibility and corroboration tracking |
| `reasoning_probabilistic` | Bayesian probability updates for belief revision with entropy metrics |

### Cognitive Analysis

| Tool | Description |
|------|-------------|
| `reasoning_detect_biases` | Identify cognitive biases (confirmation, anchoring, availability, etc.) |
| `reasoning_detect_fallacies` | Detect logical fallacies (ad hominem, straw man, false dichotomy, etc.) |

### Workflow Presets

| Tool | Description |
|------|-------------|
| `reasoning_preset_list` | List available presets by category |
| `reasoning_preset_run` | Execute a multi-step workflow preset |

**Built-in Presets:**
- `code-review` - Divergent analysis + bias/fallacy detection + reflection
- `debug-analysis` - Linear analysis + tree exploration + checkpointing + reflection
- `architecture-decision` - Multi-perspective analysis for architectural choices
- `strategic-decision` - Multi-criteria decision analysis with stakeholder perspectives and bias detection
- `evidence-based-conclusion` - Evidence quality assessment with Bayesian probability updates and fallacy detection

## Configuration

### Required

| Variable | Description |
|----------|-------------|
| `LANGBASE_API_KEY` | Your Langbase API key |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `LANGBASE_BASE_URL` | `https://api.langbase.com` | API endpoint |
| `DATABASE_PATH` | `./data/reasoning.db` | SQLite database location |
| `DATABASE_MAX_CONNECTIONS` | `5` | Connection pool size |
| `LOG_LEVEL` | `info` | Logging verbosity (`trace`, `debug`, `info`, `warn`, `error`) |
| `LOG_FORMAT` | `pretty` | Log format (`pretty` or `json`) |
| `REQUEST_TIMEOUT_MS` | `30000` | HTTP timeout in milliseconds |
| `MAX_RETRIES` | `3` | API retry attempts |
| `RETRY_DELAY_MS` | `1000` | Initial retry delay (exponential backoff) |

### Pipe Names

Override default Langbase pipe names:

| Variable | Default |
|----------|---------|
| `PIPE_LINEAR` | `linear-reasoning-v1` |
| `PIPE_TREE` | `tree-reasoning-v1` |
| `PIPE_DIVERGENT` | `divergent-reasoning-v1` |
| `PIPE_REFLECTION` | `reflection-v1` |
| `PIPE_AUTO` | `mode-router-v1` |
| `PIPE_BACKTRACKING` | `backtracking-reasoning-v1` |
| `PIPE_GOT_*` | `got-{operation}-v1` |
| `PIPE_DECISION` | `decision-maker-v1` |
| `PIPE_PERSPECTIVE` | `perspective-analyzer-v1` |
| `PIPE_EVIDENCE` | `evidence-assessor-v1` |
| `PIPE_BAYESIAN` | `bayesian-updater-v1` |

## Architecture

```
┌─────────────┐     stdio      ┌──────────────────────┐     HTTPS     ┌─────────────────┐
│ MCP Client  │◄──────────────►│ mcp-langbase-reason  │◄─────────────►│ Langbase Pipes  │
│ (Claude)    │   JSON-RPC     │         ing          │               │                 │
└─────────────┘                └──────────┬───────────┘               └─────────────────┘
                                          │
                                          ▼
                               ┌──────────────────────┐
                               │       SQLite         │
                               │ sessions, thoughts,  │
                               │ branches, checkpts,  │
                               │ graphs, invocations  │
                               └──────────────────────┘
```

**Components:**
- **Server**: JSON-RPC 2.0 over async stdio (tokio)
- **Modes**: Reasoning implementations with Langbase integration
- **Presets**: Multi-step workflow composition and execution
- **Storage**: SQLite with compile-time verified queries (sqlx)
- **Client**: HTTP client with retry logic and structured errors

## Development

### Build

```bash
cargo build              # Debug
cargo build --release    # Release (optimized)
```

### Test

```bash
cargo test               # All tests (591 unit + integration)
cargo test modes::       # Test specific module
```

### Lint

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

### Run Locally

```bash
LANGBASE_API_KEY=xxx cargo run
# Or with .env file configured:
cargo run
```

### Test MCP Protocol

```bash
# Initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run

# List tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | cargo run

# Call tool
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"reasoning_linear","arguments":{"content":"Analyze this problem"}}}' | cargo run
```

## Project Structure

```
src/
├── main.rs           # Entry point, server initialization
├── lib.rs            # Public API exports
├── config/           # Environment and settings
├── error/            # Structured error types
├── langbase/         # Langbase API client
├── modes/            # Reasoning mode implementations
│   ├── linear.rs     # Sequential reasoning
│   ├── tree.rs       # Branching exploration
│   ├── divergent.rs  # Creative multi-perspective
│   ├── reflection.rs # Meta-cognitive analysis
│   ├── backtracking.rs # Checkpoint/restore
│   ├── auto.rs       # Automatic mode selection
│   ├── got.rs        # Graph-of-Thoughts
│   └── decision.rs   # Decision framework & evidence
├── presets/          # Workflow preset system
│   ├── types.rs      # Preset data structures
│   ├── registry.rs   # Preset registration
│   ├── builtins.rs   # Built-in presets
│   └── executor.rs   # Preset execution engine
├── prompts.rs        # System prompts for Langbase
├── server/           # MCP protocol handling
└── storage/          # SQLite persistence layer
```

## Troubleshooting

**"LANGBASE_API_KEY is required"**
- Set the environment variable or create `.env` file

**"Pipe not found"**
- Create required pipes in Langbase dashboard
- Verify pipe names match configuration

**Connection timeout**
- Check network access to api.langbase.com
- Increase `REQUEST_TIMEOUT_MS`

**Database locked**
- Ensure only one server instance is running
- Check write permissions for database directory

**Debug logging**
```bash
LOG_LEVEL=debug cargo run
```

## License

MIT
