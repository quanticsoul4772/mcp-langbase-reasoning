-- Phase 10 migration: Reasoning Time Machine support
-- Creates tables for timelines, MCTS nodes, and counterfactual analysis

-- Timelines table: top-level reasoning paths through time
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
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_timelines_session ON timelines(session_id);
CREATE INDEX IF NOT EXISTS idx_timelines_state ON timelines(state);
CREATE INDEX IF NOT EXISTS idx_timelines_created ON timelines(created_at);

-- Timeline branches: extended branch data for timeline navigation
-- Links to existing branches table with additional MCTS/timeline metadata
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
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (timeline_id) REFERENCES timelines(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_timeline_branches_timeline ON timeline_branches(timeline_id);
CREATE INDEX IF NOT EXISTS idx_timeline_branches_depth ON timeline_branches(depth);
CREATE INDEX IF NOT EXISTS idx_timeline_branches_ucb ON timeline_branches(ucb_score);
CREATE INDEX IF NOT EXISTS idx_timeline_branches_visits ON timeline_branches(visit_count);

-- MCTS nodes: for Monte Carlo Tree Search exploration
CREATE TABLE IF NOT EXISTS mcts_nodes (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    timeline_id TEXT,
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
    FOREIGN KEY (timeline_id) REFERENCES timelines(id) ON DELETE SET NULL,
    FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_node_id) REFERENCES mcts_nodes(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_mcts_nodes_session ON mcts_nodes(session_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_timeline ON mcts_nodes(timeline_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_branch ON mcts_nodes(branch_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_parent ON mcts_nodes(parent_node_id);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_ucb ON mcts_nodes(ucb_score DESC);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_visits ON mcts_nodes(visit_count DESC);
CREATE INDEX IF NOT EXISTS idx_mcts_nodes_terminal ON mcts_nodes(is_terminal);

-- Counterfactual analyses: "What if?" reasoning results
CREATE TABLE IF NOT EXISTS counterfactual_analyses (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    timeline_id TEXT,
    original_branch_id TEXT NOT NULL,
    question TEXT NOT NULL,
    intervention_type TEXT NOT NULL,  -- change, remove, replace, inject
    intervention TEXT NOT NULL,
    target_thought_id TEXT,
    counterfactual_branch_id TEXT NOT NULL,
    outcome_delta REAL DEFAULT 0.0,
    causal_attribution REAL DEFAULT 0.0,
    confidence REAL DEFAULT 0.0,
    comparison TEXT NOT NULL,  -- JSON with detailed comparison
    created_at TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (timeline_id) REFERENCES timelines(id) ON DELETE SET NULL,
    FOREIGN KEY (original_branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (target_thought_id) REFERENCES thoughts(id) ON DELETE SET NULL,
    FOREIGN KEY (counterfactual_branch_id) REFERENCES branches(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_counterfactual_session ON counterfactual_analyses(session_id);
CREATE INDEX IF NOT EXISTS idx_counterfactual_timeline ON counterfactual_analyses(timeline_id);
CREATE INDEX IF NOT EXISTS idx_counterfactual_original ON counterfactual_analyses(original_branch_id);
CREATE INDEX IF NOT EXISTS idx_counterfactual_type ON counterfactual_analyses(intervention_type);
CREATE INDEX IF NOT EXISTS idx_counterfactual_created ON counterfactual_analyses(created_at);
