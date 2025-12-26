# mcp-langbase-reasoning

A Model Context Protocol (MCP) server providing structured reasoning capabilities powered by Langbase Pipes. Written in Rust with SQLite persistence.

## Features

- **9 Reasoning Modes** - Linear, tree, divergent, reflection, backtracking, auto-selection, Graph-of-Thoughts, decision framework, and evidence assessment
- **5 Workflow Presets** - Code review, debugging, architecture decisions, strategic decisions, and evidence-based conclusions
- **Cognitive Analysis** - Bias detection and logical fallacy identification
- **Self-Improvement** - Autonomous monitoring and optimization loop
- **Session Persistence** - SQLite storage for sessions, thoughts, branches, and checkpoints
- **Production Ready** - 1900+ tests, async I/O, retry logic, structured error handling

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

## Architecture

```
┌─────────────┐     stdio      ┌──────────────────┐     HTTPS     ┌─────────────────┐
│ MCP Client  │◄──────────────►│ mcp-langbase-    │◄─────────────►│ Langbase Pipes  │
│ (Claude)    │   JSON-RPC     │   reasoning      │               │ (8 pipes)       │
└─────────────┘                └────────┬─────────┘               └─────────────────┘
                                        │
                                        ▼
                               ┌──────────────────┐
                               │     SQLite       │
                               │  sessions,       │
                               │  thoughts,       │
                               │  graphs          │
                               └──────────────────┘
```

## Development

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo clippy -- -D warnings  # Lint
```

### Self-Improvement CLI

```bash
cargo run -- self-improve status      # System status
cargo run -- self-improve history     # Action history
cargo run -- self-improve enable      # Enable system
cargo run -- self-improve disable     # Disable system
```

## Project Structure

```
src/
├── main.rs           # Entry point
├── config/           # Environment configuration
├── error/            # Structured error types
├── langbase/         # Langbase API client
├── modes/            # 9 reasoning implementations
├── presets/          # Workflow preset system
├── self_improvement/ # Autonomous optimization
├── server/           # MCP protocol handling
└── storage/          # SQLite persistence

docs/
├── API_REFERENCE.md  # Complete tool documentation
├── ARCHITECTURE.md   # Technical design
└── LANGBASE_API.md   # Pipe integration
```

## License

MIT
