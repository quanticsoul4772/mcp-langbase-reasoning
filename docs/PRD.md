Product Requirements Document

Project: mcp-langbase-reasoning
Version: 1.0
Status: Draft
Owner: —
Audience: Engineers building or integrating MCP servers

1. Purpose

mcp-langbase-reasoning is an MCP-compliant server that provides structured reasoning tools by delegating LLM-based cognition to Langbase Pipes, while retaining deterministic state, schemas, and orchestration locally.

The server exists to externalize reasoning logic into versioned, auditable Langbase prompts while preserving MCP's strengths: tool execution, control flow, and integration with client agents.

2. Problem Statement

Current MCP servers typically embed prompts directly in code, which creates the following issues:

Prompt logic is tightly coupled to implementation

Prompt changes require code changes and redeploys

Reasoning behavior is difficult to version, audit, or experiment with

Complex reasoning modes (tree, reflection, GoT) become unmaintainable

Meanwhile, Langbase provides:

Versioned, deployable prompt APIs ("Pipes")

Deterministic structured outputs

Prompt lifecycle management

But Langbase alone does not provide:

Tool execution

State management

Agent control flow

MCP compatibility

This project bridges that gap.

3. Goals
Primary Goals

Provide a drop-in MCP server exposing reasoning tools

Delegate all generative reasoning to Langbase Pipes

Preserve strict JSON schemas suitable for downstream automation

Support multiple reasoning modes with parity to unified-thinking

Maintain local state for sessions, branches, checkpoints, and graphs

Non-Goals

Building an autonomous agent

Managing workflows or task queues

Replacing MCP clients (Claude, Cursor, Windsurf, etc.)

Providing UI or visualization

Executing external tools beyond MCP scope

4. Target Users

Engineers building MCP-based agent systems

Researchers experimenting with structured reasoning modes

Teams wanting reproducible, versioned reasoning behavior

Developers migrating from prompt-in-code to prompt-as-service

5. High-Level Architecture
[MCP Client]
     |
     |  MCP tool call
     v
[mcp-langbase-reasoning]
     |
     |  HTTP (pipes.run)
     v
[Langbase Pipe]
     |
     |  JSON completion
     v
[mcp-langbase-reasoning]
     |
     |  MCP response
     v
[MCP Client]

Architectural Principles

Thin MCP tools: no reasoning logic embedded

Langbase as reasoning backend

Local persistence for state and traceability

Deterministic contracts at all boundaries

6. Core Features
6.1 Reasoning Modes (v1 Parity Target)

Each mode maps to one or more Langbase Pipes.

Mode	Description
linear	Single-pass reasoning
tree	Branching reasoning paths
divergent	Idea generation / exploration
reflection	Self-critique and improvement
backtracking	Re-evaluation with corrections
auto	Mode selection router
graph-of-thoughts	Node/edge-based reasoning with scoring
6.2 MCP Tools (Initial Set)

Each tool:

Has a strict input schema

Returns strict JSON

Is side-effect free except local state

Examples:

reasoning.linear

reasoning.tree

reasoning.divergent

reasoning.reflect

reasoning.backtrack

reasoning.auto

reasoning.got.run

State tools:

session.create

session.restore

history.get

checkpoint.create

checkpoint.restore

7. Langbase Integration Requirements
7.1 Pipe Usage

Each reasoning mode corresponds to a named Langbase Pipe

Pipes must:

Accept structured variables

Return JSON-only output when required

Be versioned independently of server code

7.2 API Contract

Use POST /v1/pipes/run

Authentication via environment variable

Support messages, variables, optional threadId

7.3 Failure Handling

Network or pipe errors must be normalized

MCP tool returns a structured error object

No partial or malformed responses

8. State Management
Local Persistence (Required)

SQLite-backed storage

Session history

Branch trees

GoT graphs

Tool invocation logs

Explicit Design Choice

Langbase thread history is not the source of truth.
Local state is canonical.

9. Technology Requirements
Language

Rust (stable)

Key Dependencies

MCP Rust SDK (spec-compliant)

reqwest + serde

tokio

sqlx (sqlite)

tracing

Configuration

All secrets via environment variables

No secrets embedded in prompts

Pipe API keys configurable per mode

10. Security & Safety

No dynamic code execution

No shell access

No filesystem writes outside configured storage

Bounded request size

Timeouts on all external calls

Deterministic output enforcement where required

11. Observability

Structured logs for:

Tool invocation

Langbase calls

Errors

Optional trace IDs per session

Debug mode with full request/response capture (off by default)

12. Success Metrics

MCP clients can reliably call reasoning tools

Langbase pipes can be swapped without code changes

Reasoning outputs are reproducible and schema-valid

Feature parity achieved with unified-thinking (v1)

13. Release Plan
v0.1

MCP server bootstrap

One reasoning tool (reasoning.linear)

SQLite state

One Langbase pipe

v0.2

Tree + divergent modes

Checkpoints

Auto router

v1.0

Full reasoning mode parity

Graph-of-Thoughts

Import/export compatibility

14. Open Questions (Explicit)

Exact GoT schema standardization

Optional Neo4j support vs SQLite-only

Whether to expose Langbase memory bindings directly
