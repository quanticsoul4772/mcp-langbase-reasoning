# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Reasoning Time Machine** - Full temporal navigation through reasoning processes
  - `reasoning_timeline_create` - Create reasoning timelines with checkpoints
  - `reasoning_timeline_branch` - Branch from any checkpoint to explore alternatives
  - `reasoning_timeline_compare` - Compare outcomes across branches
  - `reasoning_timeline_merge` - Merge insights with multiple strategies (best_of, synthesize, consensus, weighted)
  - `reasoning_mcts_explore` - Monte Carlo Tree Search with UCB1 balancing
  - `reasoning_auto_backtrack` - Self-backtracking with quality assessment
  - `reasoning_counterfactual` - "What if?" analysis with Pearl's Ladder of Causation
- Database migration for timelines, MCTS nodes, and counterfactual analyses
- 82 new unit tests for Time Machine modes

### Changed

- Updated documentation to reflect Time Machine feature
- Mode count increased from 9 to 12 (timeline, mcts, counterfactual)

## [0.2.0] - 2025-12-26

### Added

- **Autonomous Self-Improvement System** - 4-phase optimization loop (Monitor → Analyzer → Executor → Learner)
  - Real-time metrics collection and baseline tracking
  - AI-assisted root cause diagnosis via Langbase pipes
  - Safe parameter adjustments with bounded allowlist
  - Reward-based learning and lesson synthesis
- **Safety Controls**
  - Circuit breaker pattern (stops after consecutive failures)
  - Action allowlist with parameter bounds and step limits
  - Rate limiting (max actions per hour)
  - Automatic rollback on regression detection
  - Cooldown periods between actions
  - AI validation (bias and fallacy detection on decisions)
- **CLI Commands** for self-improvement management
  - `self-improve status` - View system status
  - `self-improve history` - View action history
  - `self-improve enable/disable` - Toggle system
  - `self-improve pause --duration` - Temporary pause
  - `self-improve config` - View configuration
  - `self-improve check` - Force health check
  - `self-improve rollback` - Rollback last action
- Fallback transparency tracking for auto-mode routing
- Record skip tracking for database parse failures
- Timestamp reconstruction tracking for data integrity
- Strict mode for proper error propagation

### Changed

- Consolidated from 14 pipes to 8 optimized Langbase pipes
- Updated test coverage to 2100+ tests (83% coverage)
- Improved documentation with comprehensive self-improvement system docs
- Architecture diagrams updated to show self-improvement loop

### Fixed

- Resolved all 47 clippy warnings
- Fixed silent database parse failures with proper tracking

## [0.1.0] - 2025-12-26

### Added

- Initial release
- **9 Reasoning Modes**
  - Linear (sequential step-by-step)
  - Tree (branching exploration with 2-4 paths)
  - Divergent (creative multi-perspective)
  - Reflection (meta-cognitive analysis)
  - Backtracking (checkpoint and restore)
  - Auto (automatic mode selection)
  - Graph-of-Thoughts (GoT operations)
  - Detection (bias and fallacy identification)
  - Decision (multi-criteria framework)
- **5 Workflow Presets**
  - code-review
  - debug-analysis
  - architecture-decision
  - strategic-decision
  - evidence-based-conclusion
- **Cognitive Analysis**
  - Bias detection
  - Logical fallacy identification
- **Session Persistence**
  - SQLite storage for sessions, thoughts, branches, checkpoints
  - Graph storage for GoT operations
- **Production Features**
  - Async I/O with tokio
  - Retry logic with exponential backoff
  - Structured error handling with thiserror
  - Compile-time verified SQL with sqlx
