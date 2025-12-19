-- Add indexes for pipe metrics queries
-- These indexes improve performance for aggregation and filtering on invocations table

-- Index for pipe name lookups and GROUP BY
CREATE INDEX IF NOT EXISTS idx_invocations_pipe_name ON invocations(pipe_name);

-- Index for success/failure filtering
CREATE INDEX IF NOT EXISTS idx_invocations_success ON invocations(success);

-- Composite index for common filter combinations
CREATE INDEX IF NOT EXISTS idx_invocations_pipe_success ON invocations(pipe_name, success);

-- Index for tool name filtering
CREATE INDEX IF NOT EXISTS idx_invocations_tool_name ON invocations(tool_name);
