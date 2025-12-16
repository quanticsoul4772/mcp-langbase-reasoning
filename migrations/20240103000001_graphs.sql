-- Phase 3 migration: Graph-of-Thoughts support for advanced reasoning
-- Creates tables for GoT nodes, edges, and state snapshots for backtracking

-- Graph nodes: GoT vertices for graph-based reasoning
CREATE TABLE IF NOT EXISTS graph_nodes (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    content TEXT NOT NULL,
    node_type TEXT DEFAULT 'thought',  -- thought, hypothesis, conclusion, aggregation
    score REAL,
    depth INTEGER DEFAULT 0,
    is_terminal INTEGER DEFAULT 0,
    is_root INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    created_at TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Indexes for graph_nodes
CREATE INDEX IF NOT EXISTS idx_graph_nodes_session ON graph_nodes(session_id);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_type ON graph_nodes(node_type);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_active ON graph_nodes(is_active);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_terminal ON graph_nodes(is_terminal);

-- Graph edges: GoT connections between nodes
CREATE TABLE IF NOT EXISTS graph_edges (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    from_node TEXT NOT NULL,
    to_node TEXT NOT NULL,
    edge_type TEXT DEFAULT 'generates',  -- generates, refines, aggregates, supports, contradicts
    weight REAL DEFAULT 1.0,
    created_at TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (from_node) REFERENCES graph_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (to_node) REFERENCES graph_nodes(id) ON DELETE CASCADE
);

-- Indexes for graph_edges
CREATE INDEX IF NOT EXISTS idx_graph_edges_session ON graph_edges(session_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_from ON graph_edges(from_node);
CREATE INDEX IF NOT EXISTS idx_graph_edges_to ON graph_edges(to_node);
CREATE INDEX IF NOT EXISTS idx_graph_edges_type ON graph_edges(edge_type);

-- State snapshots: for backtracking support beyond checkpoints
CREATE TABLE IF NOT EXISTS state_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    snapshot_type TEXT NOT NULL,  -- full, incremental, branch
    state_data TEXT NOT NULL,  -- JSON blob of serialized state
    parent_snapshot_id TEXT,
    created_at TEXT NOT NULL,
    description TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_snapshot_id) REFERENCES state_snapshots(id) ON DELETE SET NULL
);

-- Indexes for state_snapshots
CREATE INDEX IF NOT EXISTS idx_snapshots_session ON state_snapshots(session_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_parent ON state_snapshots(parent_snapshot_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_type ON state_snapshots(snapshot_type);
