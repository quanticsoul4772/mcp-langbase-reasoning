# mcp-langbase-reasoning

MCP server providing structured reasoning tools via Langbase Pipes.

## Overview

This server delegates generative reasoning to versioned Langbase Pipes while maintaining local state, schemas, and orchestration. It provides the same tool interface as unified-thinking but externalizes prompt logic for better versioning, auditing, and experimentation.

## Features

- MCP compliant: Drop-in replacement for existing MCP clients (Claude Desktop, etc.)
- Langbase backend: Versioned prompts, structured JSON outputs, lifecycle management
- Multiple reasoning modes: Linear, tree, divergent, reflection, backtracking, auto, and Graph-of-Thoughts
- Local state: SQLite persistence for sessions, thoughts, branches, checkpoints, and graphs
- Async I/O: Non-blocking stdio communication using Tokio
- Retry logic: Configurable retries with exponential backoff for API calls
- Structured errors: Comprehensive error types with proper JSON-RPC error codes

## Quick Start

### 1. Prerequisites

- Rust 1.70+ (with cargo)
- Langbase account with API key

### 2. Setup

```bash
# Clone and enter directory
git clone <repository-url>
cd mcp-langbase-reasoning

# Create environment file
cp .env.example .env

# Edit .env with your API key
# LANGBASE_API_KEY=your_key_here

# Build
cargo build --release
```

### 3. Configure MCP Client

Add to your Claude Desktop configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "langbase-reasoning": {
      "command": "path/to/mcp-langbase-reasoning",
      "args": [],
      "env": {
        "LANGBASE_API_KEY": "your_api_key"
      }
    }
  }
}
```

### 4. Use

The reasoning tools are now available in Claude:

```
Use the reasoning_linear tool to analyze the trade-offs
between microservices and monolithic architectures.
```

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LANGBASE_API_KEY` | Yes | - | Your Langbase API key |
| `LANGBASE_BASE_URL` | No | `https://api.langbase.com` | API endpoint |
| `DATABASE_PATH` | No | `./data/reasoning.db` | SQLite database path |
| `DATABASE_MAX_CONNECTIONS` | No | `5` | Connection pool size |
| `LOG_LEVEL` | No | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `LOG_FORMAT` | No | `pretty` | `pretty` or `json` |
| `REQUEST_TIMEOUT_MS` | No | `30000` | HTTP timeout (ms) |
| `MAX_RETRIES` | No | `3` | Max retry attempts |
| `RETRY_DELAY_MS` | No | `1000` | Initial retry delay (ms) |

### Pipe Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `PIPE_LINEAR` | `linear-reasoning-v1` | Linear reasoning pipe |
| `PIPE_TREE` | `tree-reasoning-v1` | Tree reasoning pipe |
| `PIPE_DIVERGENT` | `divergent-reasoning-v1` | Divergent reasoning pipe |
| `PIPE_REFLECTION` | `reflection-v1` | Reflection pipe |
| `PIPE_AUTO` | `mode-router-v1` | Auto mode router pipe |
| `PIPE_BACKTRACKING` | `backtracking-reasoning-v1` | Backtracking pipe |
| `PIPE_GOT_GENERATE` | `got-generate-v1` | GoT generate pipe |
| `PIPE_GOT_SCORE` | `got-score-v1` | GoT score pipe |
| `PIPE_GOT_AGGREGATE` | `got-aggregate-v1` | GoT aggregate pipe |
| `PIPE_GOT_REFINE` | `got-refine-v1` | GoT refine pipe |

## MCP Tools

### Core Reasoning Tools

| Tool | Description |
|------|-------------|
| `reasoning_linear` | Single-pass sequential reasoning with session continuity |
| `reasoning_tree` | Branching exploration with multiple reasoning paths |
| `reasoning_tree_focus` | Focus on a specific branch |
| `reasoning_tree_list` | List all branches in a session |
| `reasoning_tree_complete` | Mark a branch as completed or abandoned |
| `reasoning_divergent` | Creative reasoning with novel perspectives |
| `reasoning_reflection` | Meta-cognitive analysis of reasoning quality |
| `reasoning_reflection_evaluate` | Evaluate session reasoning quality |
| `reasoning_auto` | Automatic mode selection based on content |

### Checkpoint Tools

| Tool | Description |
|------|-------------|
| `reasoning_checkpoint_create` | Create a checkpoint for later backtracking |
| `reasoning_checkpoint_list` | List available checkpoints |
| `reasoning_backtrack` | Restore from checkpoint and explore alternatives |

### Graph-of-Thoughts Tools

| Tool | Description |
|------|-------------|
| `reasoning_got_init` | Initialize a reasoning graph with root node |
| `reasoning_got_generate` | Generate k diverse continuations from a node |
| `reasoning_got_score` | Score node quality (relevance, validity, depth, novelty) |
| `reasoning_got_aggregate` | Merge multiple nodes into unified insight |
| `reasoning_got_refine` | Improve node through self-critique |
| `reasoning_got_prune` | Remove low-scoring nodes |
| `reasoning_got_finalize` | Mark terminal nodes and get conclusions |
| `reasoning_got_state` | Get current graph state and structure |

## Architecture

```
MCP Client -----> mcp-langbase-reasoning -----> Langbase Pipes
 (Claude)   stdio      |                  HTTPS
                       |
                    SQLite
              (sessions, thoughts,
               branches, checkpoints,
               graphs, invocations)
```

Components:
- MCP Layer: JSON-RPC 2.0 over async stdio
- Modes: Reasoning implementations (linear, tree, divergent, reflection, auto, got)
- Storage: SQLite with embedded migrations
- Langbase Client: HTTP client with retry logic

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed technical documentation.

## Development

### Build

```bash
cargo build              # Debug build
cargo build --release    # Release build
```

### Test

```bash
cargo test               # Run all tests
cargo test --test storage_test    # Run specific test file
```

### Lint

```bash
cargo fmt --check        # Check formatting
cargo clippy -- -D warnings   # Lint checks
```

### Run Locally

```bash
# With environment variables
LANGBASE_API_KEY=xxx cargo run

# Or with .env file
cargo run
```

### Test MCP Protocol

```bash
# Send initialize request
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run

# List tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | cargo run

# Call tool
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"reasoning_linear","arguments":{"content":"Test"}}}' | cargo run
```

## Project Structure

```
mcp-langbase-reasoning/
├── src/
│   ├── main.rs              # Entry point
│   ├── lib.rs               # Public API
│   ├── config/              # Configuration loading
│   ├── error/               # Error types
│   ├── langbase/            # Langbase API client
│   ├── modes/               # Reasoning mode implementations
│   │   ├── mod.rs           # Mode exports and ReasoningMode enum
│   │   ├── linear.rs        # Linear reasoning
│   │   ├── tree.rs          # Tree/branching reasoning
│   │   ├── divergent.rs     # Divergent/creative reasoning
│   │   ├── reflection.rs    # Meta-cognitive reflection
│   │   ├── backtracking.rs  # Checkpoint and backtrack
│   │   ├── auto.rs          # Auto mode selection
│   │   └── got.rs           # Graph-of-Thoughts
│   ├── prompts.rs           # Centralized system prompts
│   ├── server/              # MCP protocol handling
│   └── storage/             # SQLite persistence
├── tests/                   # Integration tests
├── migrations/              # SQLite migrations
├── docs/
│   ├── API_REFERENCE.md     # Complete API documentation
│   ├── ARCHITECTURE.md      # Technical architecture
│   ├── LANGBASE_API.md      # Langbase integration details
│   └── PRD.md               # Product requirements
├── .env.example             # Environment template
├── Cargo.toml               # Dependencies
└── README.md                # This file
```

## Documentation

| Document | Description |
|----------|-------------|
| [API Reference](docs/API_REFERENCE.md) | Complete tool and type documentation |
| [Architecture](docs/ARCHITECTURE.md) | Technical design and data flow |
| [Langbase API](docs/LANGBASE_API.md) | Langbase integration reference |
| [Implementation Plan](IMPLEMENTATION_PLAN.md) | Development roadmap |
| [Environment Setup](ENVIRONMENT_SETUP.md) | Development setup guide |

## Version History

| Version | Status | Features |
|---------|--------|----------|
| v0.1 | Complete | MCP bootstrap, linear mode, SQLite |
| v0.2 | Complete | Tree, divergent, reflection modes |
| v0.3 | Complete | Backtracking, auto, Graph-of-Thoughts |
| v1.0 | Planned | Production polish, full test coverage |

## Troubleshooting

### Common Issues

**"LANGBASE_API_KEY is required"**
- Ensure the environment variable is set or `.env` file exists

**"Pipe not found"**
- Create the required pipe in Langbase dashboard
- Or use the Langbase API to create pipes programmatically

**Connection timeout**
- Check network connectivity to api.langbase.com
- Increase `REQUEST_TIMEOUT_MS` if needed

**Database locked**
- Ensure only one instance is running
- Check `DATABASE_PATH` permissions

**Tool not found**
- Verify MCP client configuration
- Check server logs for initialization errors

### Debug Logging

```bash
LOG_LEVEL=debug cargo run
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make changes with tests
4. Run `cargo fmt` and `cargo clippy`
5. Submit a pull request

## License

MIT
