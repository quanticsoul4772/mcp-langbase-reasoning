//! MCTS reasoning mode - Monte Carlo Tree Search for reasoning exploration.
//!
//! This module provides MCTS-based reasoning for:
//! - Systematic exploration of reasoning paths using UCB1 selection
//! - Simulation-based evaluation of reasoning quality
//! - Backpropagation of rewards through the search tree
//! - Automatic backtracking based on reward signals

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::TREE_REASONING_PROMPT;
use crate::storage::{Invocation, MCTSNode, SqliteStorage, Storage};

/// Default exploration constant for UCB1 (sqrt(2))
const DEFAULT_EXPLORATION_CONSTANT: f64 = std::f64::consts::SQRT_2;

/// Default simulation depth
const DEFAULT_SIMULATION_DEPTH: i32 = 3;

/// Input parameters for MCTS exploration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSExploreParams {
    /// Content to explore
    pub content: String,
    /// Optional session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional timeline ID to associate with
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_id: Option<String>,
    /// Number of MCTS iterations to perform
    #[serde(default = "default_iterations")]
    pub iterations: usize,
    /// Exploration constant for UCB1
    #[serde(default = "default_exploration")]
    pub exploration_constant: f64,
    /// Maximum simulation depth
    #[serde(default = "default_sim_depth")]
    pub simulation_depth: i32,
}

fn default_iterations() -> usize {
    5
}

fn default_exploration() -> f64 {
    DEFAULT_EXPLORATION_CONSTANT
}

fn default_sim_depth() -> i32 {
    DEFAULT_SIMULATION_DEPTH
}

/// Input parameters for auto-backtracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBacktrackParams {
    /// Session ID to analyze
    pub session_id: String,
    /// Optional timeline ID to focus on
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_id: Option<String>,
    /// Confidence threshold below which to trigger backtracking
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Reward threshold below which to trigger backtracking
    #[serde(default = "default_reward_threshold")]
    pub reward_threshold: f64,
}

fn default_confidence_threshold() -> f64 {
    0.3
}

fn default_reward_threshold() -> f64 {
    0.2
}

/// Result of MCTS exploration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCTSExploreResult {
    /// Session ID
    pub session_id: String,
    /// Root node ID
    pub root_node_id: String,
    /// Best path found (sequence of node IDs)
    pub best_path: Vec<String>,
    /// Best path content
    pub best_path_content: Vec<String>,
    /// Total value of best path
    pub best_path_value: f64,
    /// Number of nodes explored
    pub nodes_explored: usize,
    /// Statistics per iteration
    pub iteration_stats: Vec<IterationStats>,
}

/// Statistics for one MCTS iteration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStats {
    /// Iteration number
    pub iteration: usize,
    /// Node selected for expansion
    pub selected_node: String,
    /// Value from simulation
    pub simulation_value: f64,
    /// Nodes visited in backpropagation
    pub backprop_nodes: usize,
}

/// Result of auto-backtracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBacktrackResult {
    /// Whether backtracking was triggered
    pub backtracked: bool,
    /// Reason for decision
    pub reason: String,
    /// Node backtracked to (if any)
    pub backtrack_to: Option<String>,
    /// Suggested alternative paths
    pub alternative_paths: Vec<AlternativePath>,
    /// Current confidence level
    pub current_confidence: f64,
    /// Current reward estimate
    pub current_reward: f64,
}

/// Alternative path suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativePath {
    /// Starting node ID
    pub from_node: String,
    /// Suggested direction
    pub direction: String,
    /// Expected improvement
    pub expected_improvement: f64,
}

/// MCTS mode handler for Monte Carlo Tree Search reasoning.
#[derive(Clone)]
pub struct MCTSMode {
    /// Core infrastructure
    core: ModeCore,
    /// Tree pipe for expansion
    tree_pipe: String,
    /// Decision pipe for evaluation
    decision_pipe: String,
    /// Divergent pipe for alternatives.
    /// Reserved for future diverse path generation in MCTS expansion.
    #[allow(dead_code)]
    divergent_pipe: String,
    /// Reflection pipe for quality analysis.
    /// Reserved for future auto-backtrack quality assessment.
    #[allow(dead_code)]
    reflection_pipe: String,
}

impl MCTSMode {
    /// Create a new MCTS mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        // Extract pipe names from config, with defaults
        let decision_pipe = config
            .pipes
            .decision
            .as_ref()
            .and_then(|c| c.pipe.clone())
            .unwrap_or_else(|| "decision-framework-v1".to_string());

        Self {
            core: ModeCore::new(storage, langbase),
            tree_pipe: config.pipes.tree.clone(),
            decision_pipe,
            divergent_pipe: config.pipes.divergent.clone(),
            reflection_pipe: config.pipes.reflection.clone(),
        }
    }

    /// Perform MCTS exploration
    pub async fn explore(&self, params: MCTSExploreParams) -> AppResult<MCTSExploreResult> {
        let start = Instant::now();

        // Validate input
        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Content cannot be empty".to_string(),
            }
            .into());
        }

        let iterations = params.iterations.clamp(1, 20);

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "mcts")
            .await?;
        debug!(session_id = %session.id, iterations = iterations, "Starting MCTS exploration");

        // Create or get a branch for MCTS
        let branches = self.core.storage().get_session_branches(&session.id).await?;
        let branch_id = if let Some(first) = branches.first() {
            first.id.clone()
        } else {
            let branch = crate::storage::Branch::new(&session.id).with_name("MCTS Root");
            self.core.storage().create_branch(&branch).await?;
            branch.id
        };

        // Create root MCTS node
        let mut root_node = MCTSNode::new(&session.id, &branch_id, &params.content)
            .with_prior(0.5);
        if let Some(ref timeline_id) = params.timeline_id {
            root_node = root_node.with_timeline(timeline_id);
        }
        self.core.storage().create_mcts_node(&root_node).await?;

        let mut iteration_stats = Vec::new();
        let mut nodes_explored = 1;

        // Run MCTS iterations
        for i in 0..iterations {
            // SELECTION: Find best node to expand using UCB
            let selected = self.select_node(&session.id, params.exploration_constant).await?;

            // EXPANSION: Generate child nodes
            let children = self
                .expand_node(&selected, &session.id, &branch_id, params.simulation_depth)
                .await?;
            nodes_explored += children.len();

            // SIMULATION: Evaluate the expansion
            let simulation_value = self.simulate(&selected, &children).await?;

            // BACKPROPAGATION: Update values along the path
            let backprop_nodes = self
                .backpropagate(&selected.id, simulation_value)
                .await?;

            iteration_stats.push(IterationStats {
                iteration: i + 1,
                selected_node: selected.id.clone(),
                simulation_value,
                backprop_nodes,
            });

            debug!(
                iteration = i + 1,
                selected_node = %selected.id,
                simulation_value = simulation_value,
                children = children.len(),
                "MCTS iteration complete"
            );
        }

        // Find best path
        let (best_path, best_path_content, best_value) = self.find_best_path(&session.id).await?;

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_mcts_explore",
            serialize_for_log(&params, "mcts_explore_params"),
        )
        .with_session(&session.id)
        .success(serde_json::json!({
            "nodes_explored": nodes_explored,
            "best_value": best_value
        }), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            session_id = %session.id,
            nodes_explored = nodes_explored,
            best_value = best_value,
            latency_ms = latency,
            "MCTS exploration complete"
        );

        Ok(MCTSExploreResult {
            session_id: session.id,
            root_node_id: root_node.id,
            best_path,
            best_path_content,
            best_path_value: best_value,
            nodes_explored,
            iteration_stats,
        })
    }

    /// Select the best node to expand using UCB1
    async fn select_node(
        &self,
        session_id: &str,
        _exploration_constant: f64,
    ) -> AppResult<MCTSNode> {
        // Get unexpanded nodes first
        let unexpanded = self
            .core
            .storage()
            .get_unexpanded_mcts_nodes(session_id)
            .await?;

        if !unexpanded.is_empty() {
            // Return first unexpanded node (highest UCB from unexpanded)
            return Ok(unexpanded.into_iter().next().unwrap());
        }

        // All nodes expanded, select by UCB score
        let nodes = self
            .core
            .storage()
            .get_mcts_nodes_by_ucb(session_id)
            .await?;

        nodes.into_iter().next().ok_or_else(|| {
            ToolError::Reasoning {
                message: "No nodes available for selection".to_string(),
            }
            .into()
        })
    }

    /// Expand a node by generating children
    async fn expand_node(
        &self,
        node: &MCTSNode,
        session_id: &str,
        branch_id: &str,
        _simulation_depth: i32,
    ) -> AppResult<Vec<MCTSNode>> {
        // Use tree pipe to generate alternatives
        let expand_prompt = format!(
            "Generate 2-3 alternative continuations for this reasoning:\n\n{}",
            node.content
        );

        let messages = vec![
            Message::system(TREE_REASONING_PROMPT),
            Message::user(expand_prompt),
        ];
        let request = PipeRequest::new(&self.tree_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let json_str = extract_json_from_completion(&response.completion)
            .map_err(|e| ToolError::Reasoning { message: e })?;
        let tree_response: ExpandResponse = serde_json::from_str(json_str).unwrap_or_else(|e| {
            warn!(error = %e, "Failed to parse expansion, using fallback");
            ExpandResponse {
                branches: vec![ExpandBranch {
                    thought: response.completion.clone(),
                    confidence: 0.5,
                }],
            }
        });

        // Create child nodes
        let mut children = Vec::new();
        for branch in tree_response.branches {
            let child = MCTSNode::new(session_id, branch_id, &branch.thought)
                .with_parent(&node.id)
                .with_prior(branch.confidence)
                .with_simulation_depth(node.simulation_depth + 1);
            self.core.storage().create_mcts_node(&child).await?;
            children.push(child);
        }

        // Mark parent as expanded
        let mut expanded_node = node.clone();
        expanded_node.is_expanded = true;
        self.core.storage().update_mcts_node(&expanded_node).await?;

        Ok(children)
    }

    /// Simulate to evaluate a node
    async fn simulate(&self, node: &MCTSNode, children: &[MCTSNode]) -> AppResult<f64> {
        // Use decision pipe to evaluate quality
        let content_to_evaluate = if children.is_empty() {
            node.content.clone()
        } else {
            children
                .iter()
                .map(|c| c.content.as_str())
                .collect::<Vec<_>>()
                .join("\n---\n")
        };

        let eval_prompt = format!(
            "Rate the quality of this reasoning on a scale of 0.0 to 1.0:\n\n{}",
            content_to_evaluate
        );

        let messages = vec![
            Message::system("Respond with JSON: {\"score\": 0.0-1.0, \"rationale\": \"...\"}"),
            Message::user(eval_prompt),
        ];
        let request = PipeRequest::new(&self.decision_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse score
        let json_str = extract_json_from_completion(&response.completion)
            .map_err(|e| ToolError::Reasoning { message: e })?;
        let eval: EvalResponse = serde_json::from_str(json_str).unwrap_or_else(|_| EvalResponse {
            score: 0.5,
            rationale: "Evaluation parsing failed, using neutral score".to_string(),
        });

        Ok(eval.score.clamp(0.0, 1.0))
    }

    /// Backpropagate value through the tree
    async fn backpropagate(&self, node_id: &str, value: f64) -> AppResult<usize> {
        let mut current_id = Some(node_id.to_string());
        let mut nodes_updated = 0;

        while let Some(id) = current_id {
            if let Some(mut node) = self.core.storage().get_mcts_node(&id).await? {
                // Update visit count and value
                node.visit_count += 1;
                node.total_value += value;

                // Recalculate UCB score
                let parent_visits = if let Some(ref parent_id) = node.parent_node_id {
                    self.core
                        .storage()
                        .get_mcts_node(parent_id)
                        .await?
                        .map(|p| p.visit_count)
                        .unwrap_or(1)
                } else {
                    1
                };

                node.ucb_score = node.calculate_ucb(parent_visits, DEFAULT_EXPLORATION_CONSTANT);
                node.update_last_visited();

                self.core.storage().update_mcts_node(&node).await?;
                nodes_updated += 1;

                current_id = node.parent_node_id;
            } else {
                break;
            }
        }

        Ok(nodes_updated)
    }

    /// Find the best path through the tree
    async fn find_best_path(
        &self,
        session_id: &str,
    ) -> AppResult<(Vec<String>, Vec<String>, f64)> {
        let nodes = self
            .core
            .storage()
            .get_session_mcts_nodes(session_id)
            .await?;

        // Find root
        let root = nodes.iter().find(|n| n.parent_node_id.is_none());

        if let Some(root) = root {
            let mut path_ids = vec![root.id.clone()];
            let mut path_content = vec![root.content.clone()];
            let mut current = root;
            let mut total_value = root.total_value;

            // Follow best children
            loop {
                let children: Vec<_> = nodes
                    .iter()
                    .filter(|n| n.parent_node_id.as_ref() == Some(&current.id))
                    .collect();

                if children.is_empty() {
                    break;
                }

                // Select child with highest average value
                let best_child = children
                    .into_iter()
                    .max_by(|a, b| {
                        let avg_a = if a.visit_count > 0 {
                            a.total_value / a.visit_count as f64
                        } else {
                            0.0
                        };
                        let avg_b = if b.visit_count > 0 {
                            b.total_value / b.visit_count as f64
                        } else {
                            0.0
                        };
                        avg_a.partial_cmp(&avg_b).unwrap()
                    })
                    .unwrap();

                path_ids.push(best_child.id.clone());
                path_content.push(best_child.content.clone());
                total_value += best_child.total_value;
                current = best_child;
            }

            Ok((path_ids, path_content, total_value))
        } else {
            Ok((Vec::new(), Vec::new(), 0.0))
        }
    }

    /// Auto-backtracking based on reward signals
    pub async fn auto_backtrack(
        &self,
        params: AutoBacktrackParams,
    ) -> AppResult<AutoBacktrackResult> {
        let start = Instant::now();

        // Get all nodes
        let nodes = self
            .core
            .storage()
            .get_session_mcts_nodes(&params.session_id)
            .await?;

        if nodes.is_empty() {
            return Ok(AutoBacktrackResult {
                backtracked: false,
                reason: "No MCTS nodes found in session".to_string(),
                backtrack_to: None,
                alternative_paths: Vec::new(),
                current_confidence: 0.0,
                current_reward: 0.0,
            });
        }

        // Find current position (most recently visited, non-terminal)
        let current = nodes
            .iter()
            .filter(|n| !n.is_terminal)
            .max_by_key(|n| &n.last_visited);

        let current = match current {
            Some(c) => c,
            None => {
                return Ok(AutoBacktrackResult {
                    backtracked: false,
                    reason: "All nodes are terminal".to_string(),
                    backtrack_to: None,
                    alternative_paths: Vec::new(),
                    current_confidence: 0.0,
                    current_reward: 0.0,
                });
            }
        };

        // Calculate current performance
        let current_reward = if current.visit_count > 0 {
            current.total_value / current.visit_count as f64
        } else {
            0.5
        };

        let current_confidence = current.prior;

        // Check if backtracking is needed
        let should_backtrack =
            current_confidence < params.confidence_threshold || current_reward < params.reward_threshold;

        let mut backtrack_to = None;
        let mut alternative_paths = Vec::new();
        let reason: String;

        if should_backtrack {
            // Find best ancestor to backtrack to
            let mut best_ancestor: Option<&MCTSNode> = None;
            let mut best_ancestor_value = 0.0;

            let mut current_id = current.parent_node_id.clone();
            while let Some(id) = current_id {
                if let Some(ancestor) = nodes.iter().find(|n| n.id == id) {
                    let ancestor_value = if ancestor.visit_count > 0 {
                        ancestor.total_value / ancestor.visit_count as f64
                    } else {
                        0.0
                    };

                    if ancestor_value > best_ancestor_value {
                        best_ancestor = Some(ancestor);
                        best_ancestor_value = ancestor_value;
                    }

                    current_id = ancestor.parent_node_id.clone();
                } else {
                    break;
                }
            }

            if let Some(ancestor) = best_ancestor {
                backtrack_to = Some(ancestor.id.clone());

                // Find alternative paths from this ancestor
                let siblings: Vec<_> = nodes
                    .iter()
                    .filter(|n| n.parent_node_id.as_ref() == Some(&ancestor.id) && n.id != current.id)
                    .collect();

                for sibling in siblings.iter().take(3) {
                    let sibling_value = if sibling.visit_count > 0 {
                        sibling.total_value / sibling.visit_count as f64
                    } else {
                        sibling.prior
                    };

                    alternative_paths.push(AlternativePath {
                        from_node: ancestor.id.clone(),
                        direction: sibling.content.chars().take(100).collect(),
                        expected_improvement: sibling_value - current_reward,
                    });
                }

                reason = format!(
                    "Backtracking triggered: current reward ({:.2}) below threshold ({:.2}) or confidence ({:.2}) below threshold ({:.2})",
                    current_reward,
                    params.reward_threshold,
                    current_confidence,
                    params.confidence_threshold
                );
            } else {
                reason = "Backtracking indicated but no better ancestor found".to_string();
            }
        } else {
            reason = format!(
                "No backtracking needed: reward ({:.2}) >= threshold ({:.2}) and confidence ({:.2}) >= threshold ({:.2})",
                current_reward,
                params.reward_threshold,
                current_confidence,
                params.confidence_threshold
            );
        }

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_autobacktrack",
            serialize_for_log(&params, "autobacktrack_params"),
        )
        .with_session(&params.session_id)
        .success(serde_json::json!({
            "backtracked": should_backtrack,
            "current_reward": current_reward
        }), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            session_id = %params.session_id,
            backtracked = should_backtrack,
            current_reward = current_reward,
            latency_ms = latency,
            "Auto-backtrack check complete"
        );

        Ok(AutoBacktrackResult {
            backtracked: should_backtrack,
            reason,
            backtrack_to,
            alternative_paths,
            current_confidence,
            current_reward,
        })
    }
}

// Internal response types for parsing

#[derive(Debug, Deserialize)]
struct ExpandResponse {
    branches: Vec<ExpandBranch>,
}

#[derive(Debug, Deserialize)]
struct ExpandBranch {
    thought: String,
    confidence: f64,
}

/// Response from evaluation pipe.
/// The `rationale` field is parsed from JSON but currently only `score` is used.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // rationale parsed for completeness but not currently used
struct EvalResponse {
    score: f64,
    rationale: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============================================================================
    // MCTSExploreParams Tests
    // ============================================================================

    #[test]
    fn test_mcts_explore_params_deserialize() {
        let json = json!({
            "content": "Test exploration",
            "session_id": "sess-123",
            "iterations": 10
        });
        let params: MCTSExploreParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.content, "Test exploration");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
        assert_eq!(params.iterations, 10);
    }

    #[test]
    fn test_mcts_explore_params_defaults() {
        let json = json!({
            "content": "Content"
        });
        let params: MCTSExploreParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.content, "Content");
        assert_eq!(params.iterations, 5); // default
        assert!((params.exploration_constant - std::f64::consts::SQRT_2).abs() < 0.001);
        assert_eq!(params.simulation_depth, 3); // default
        assert!(params.session_id.is_none());
        assert!(params.timeline_id.is_none());
    }

    #[test]
    fn test_mcts_explore_params_custom_values() {
        let json = json!({
            "content": "Test",
            "iterations": 15,
            "exploration_constant": 2.5,
            "simulation_depth": 5,
            "timeline_id": "tl-123"
        });
        let params: MCTSExploreParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.iterations, 15);
        assert_eq!(params.exploration_constant, 2.5);
        assert_eq!(params.simulation_depth, 5);
        assert_eq!(params.timeline_id, Some("tl-123".to_string()));
    }

    #[test]
    fn test_mcts_explore_params_serialize() {
        let params = MCTSExploreParams {
            content: "Test".to_string(),
            session_id: None,
            timeline_id: Some("tl-1".to_string()),
            iterations: 10,
            exploration_constant: 1.5,
            simulation_depth: 4,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["content"], "Test");
        assert_eq!(json["iterations"], 10);
        assert!(json.get("session_id").is_none()); // skip_serializing_if
    }

    #[test]
    fn test_default_iterations() {
        assert_eq!(default_iterations(), 5);
    }

    #[test]
    fn test_default_exploration() {
        let expected = DEFAULT_EXPLORATION_CONSTANT;
        assert!((default_exploration() - expected).abs() < 0.0001);
    }

    #[test]
    fn test_default_sim_depth() {
        assert_eq!(default_sim_depth(), DEFAULT_SIMULATION_DEPTH);
    }

    // ============================================================================
    // AutoBacktrackParams Tests
    // ============================================================================

    #[test]
    fn test_auto_backtrack_params_deserialize() {
        let json = json!({
            "session_id": "sess-456"
        });
        let params: AutoBacktrackParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.session_id, "sess-456");
        assert_eq!(params.confidence_threshold, 0.3); // default
        assert_eq!(params.reward_threshold, 0.2); // default
        assert!(params.timeline_id.is_none());
    }

    #[test]
    fn test_auto_backtrack_params_custom_thresholds() {
        let json = json!({
            "session_id": "sess",
            "confidence_threshold": 0.5,
            "reward_threshold": 0.4,
            "timeline_id": "tl-1"
        });
        let params: AutoBacktrackParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.confidence_threshold, 0.5);
        assert_eq!(params.reward_threshold, 0.4);
        assert_eq!(params.timeline_id, Some("tl-1".to_string()));
    }

    #[test]
    fn test_default_confidence_threshold() {
        assert_eq!(default_confidence_threshold(), 0.3);
    }

    #[test]
    fn test_default_reward_threshold() {
        assert_eq!(default_reward_threshold(), 0.2);
    }

    // ============================================================================
    // MCTSExploreResult Tests
    // ============================================================================

    #[test]
    fn test_mcts_explore_result_serialize() {
        let result = MCTSExploreResult {
            session_id: "sess-1".to_string(),
            root_node_id: "node-root".to_string(),
            best_path: vec!["node-1".to_string(), "node-2".to_string()],
            best_path_content: vec!["Content 1".to_string(), "Content 2".to_string()],
            best_path_value: 0.85,
            nodes_explored: 15,
            iteration_stats: vec![
                IterationStats {
                    iteration: 1,
                    selected_node: "node-1".to_string(),
                    simulation_value: 0.7,
                    backprop_nodes: 2,
                },
            ],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["session_id"], "sess-1");
        assert_eq!(json["root_node_id"], "node-root");
        assert_eq!(json["best_path_value"], 0.85);
        assert_eq!(json["nodes_explored"], 15);
        assert_eq!(json["best_path"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_mcts_explore_result_deserialize() {
        let json = json!({
            "session_id": "sess",
            "root_node_id": "root",
            "best_path": ["a", "b"],
            "best_path_content": ["c1", "c2"],
            "best_path_value": 0.9,
            "nodes_explored": 10,
            "iteration_stats": []
        });
        let result: MCTSExploreResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.session_id, "sess");
        assert_eq!(result.best_path_value, 0.9);
        assert!(result.iteration_stats.is_empty());
    }

    // ============================================================================
    // IterationStats Tests
    // ============================================================================

    #[test]
    fn test_iteration_stats_serialize() {
        let stats = IterationStats {
            iteration: 5,
            selected_node: "node-123".to_string(),
            simulation_value: 0.75,
            backprop_nodes: 3,
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["iteration"], 5);
        assert_eq!(json["selected_node"], "node-123");
        assert_eq!(json["simulation_value"], 0.75);
        assert_eq!(json["backprop_nodes"], 3);
    }

    #[test]
    fn test_iteration_stats_clone() {
        let stats = IterationStats {
            iteration: 1,
            selected_node: "n".to_string(),
            simulation_value: 0.5,
            backprop_nodes: 1,
        };
        let cloned = stats.clone();
        assert_eq!(stats.iteration, cloned.iteration);
        assert_eq!(stats.simulation_value, cloned.simulation_value);
    }

    // ============================================================================
    // AutoBacktrackResult Tests
    // ============================================================================

    #[test]
    fn test_auto_backtrack_result_no_backtrack() {
        let result = AutoBacktrackResult {
            backtracked: false,
            reason: "Quality is sufficient".to_string(),
            backtrack_to: None,
            alternative_paths: vec![],
            current_confidence: 0.8,
            current_reward: 0.7,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["backtracked"], false);
        assert!(json["backtrack_to"].is_null());
        assert_eq!(json["current_confidence"], 0.8);
    }

    #[test]
    fn test_auto_backtrack_result_with_backtrack() {
        let result = AutoBacktrackResult {
            backtracked: true,
            reason: "Low reward".to_string(),
            backtrack_to: Some("node-prev".to_string()),
            alternative_paths: vec![
                AlternativePath {
                    from_node: "node-prev".to_string(),
                    direction: "Try different approach".to_string(),
                    expected_improvement: 0.3,
                },
            ],
            current_confidence: 0.2,
            current_reward: 0.1,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["backtracked"], true);
        assert_eq!(json["backtrack_to"], "node-prev");
        assert_eq!(json["alternative_paths"].as_array().unwrap().len(), 1);
    }

    // ============================================================================
    // AlternativePath Tests
    // ============================================================================

    #[test]
    fn test_alternative_path_serialize() {
        let path = AlternativePath {
            from_node: "node-1".to_string(),
            direction: "Explore alternative hypothesis".to_string(),
            expected_improvement: 0.25,
        };
        let json = serde_json::to_value(&path).unwrap();
        assert_eq!(json["from_node"], "node-1");
        assert_eq!(json["expected_improvement"], 0.25);
    }

    #[test]
    fn test_alternative_path_clone() {
        let path = AlternativePath {
            from_node: "n".to_string(),
            direction: "d".to_string(),
            expected_improvement: 0.1,
        };
        let cloned = path.clone();
        assert_eq!(path.from_node, cloned.from_node);
        assert_eq!(path.expected_improvement, cloned.expected_improvement);
    }

    // ============================================================================
    // Internal Response Types Tests
    // ============================================================================

    #[test]
    fn test_expand_response_deserialize() {
        let json = json!({
            "branches": [
                {"thought": "First expansion", "confidence": 0.8},
                {"thought": "Second expansion", "confidence": 0.6}
            ]
        });
        let response: ExpandResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.branches.len(), 2);
        assert_eq!(response.branches[0].thought, "First expansion");
        assert_eq!(response.branches[1].confidence, 0.6);
    }

    #[test]
    fn test_expand_branch_deserialize() {
        let json = json!({
            "thought": "Test thought",
            "confidence": 0.75
        });
        let branch: ExpandBranch = serde_json::from_value(json).unwrap();
        assert_eq!(branch.thought, "Test thought");
        assert_eq!(branch.confidence, 0.75);
    }

    #[test]
    fn test_eval_response_deserialize() {
        let json = json!({
            "score": 0.85,
            "rationale": "Good reasoning quality"
        });
        let response: EvalResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.score, 0.85);
        assert_eq!(response.rationale, "Good reasoning quality");
    }

    // ============================================================================
    // Round-trip Serialization Tests
    // ============================================================================

    #[test]
    fn test_mcts_explore_params_round_trip() {
        let original = MCTSExploreParams {
            content: "Test content".to_string(),
            session_id: Some("sess-123".to_string()),
            timeline_id: Some("tl-456".to_string()),
            iterations: 8,
            exploration_constant: 1.8,
            simulation_depth: 4,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MCTSExploreParams = serde_json::from_str(&json).unwrap();
        assert_eq!(original.content, deserialized.content);
        assert_eq!(original.iterations, deserialized.iterations);
        assert_eq!(original.session_id, deserialized.session_id);
    }

    #[test]
    fn test_auto_backtrack_result_round_trip() {
        let original = AutoBacktrackResult {
            backtracked: true,
            reason: "Test reason".to_string(),
            backtrack_to: Some("node-123".to_string()),
            alternative_paths: vec![
                AlternativePath {
                    from_node: "n1".to_string(),
                    direction: "dir1".to_string(),
                    expected_improvement: 0.5,
                },
            ],
            current_confidence: 0.3,
            current_reward: 0.2,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: AutoBacktrackResult = serde_json::from_str(&json).unwrap();
        assert_eq!(original.backtracked, deserialized.backtracked);
        assert_eq!(original.reason, deserialized.reason);
        assert_eq!(original.backtrack_to, deserialized.backtrack_to);
    }

    // ============================================================================
    // Constants Tests
    // ============================================================================

    #[test]
    fn test_default_exploration_constant_value() {
        assert!((DEFAULT_EXPLORATION_CONSTANT - std::f64::consts::SQRT_2).abs() < 0.0001);
    }

    #[test]
    fn test_default_simulation_depth_value() {
        assert_eq!(DEFAULT_SIMULATION_DEPTH, 3);
    }

    // ============================================================================
    // Unicode and Edge Cases Tests
    // ============================================================================

    #[test]
    fn test_mcts_explore_params_unicode() {
        let json = json!({
            "content": "探索蒙特卡洛树搜索 🌳",
            "session_id": "sess-中文"
        });
        let params: MCTSExploreParams = serde_json::from_value(json).unwrap();
        assert!(params.content.contains("探索"));
        assert!(params.content.contains("🌳"));
    }

    #[test]
    fn test_alternative_path_long_direction() {
        let path = AlternativePath {
            from_node: "node".to_string(),
            direction: "A".repeat(1000),
            expected_improvement: 0.5,
        };
        let json = serde_json::to_value(&path).unwrap();
        assert_eq!(json["direction"].as_str().unwrap().len(), 1000);
    }

    #[test]
    fn test_iteration_stats_zero_values() {
        let stats = IterationStats {
            iteration: 0,
            selected_node: "".to_string(),
            simulation_value: 0.0,
            backprop_nodes: 0,
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["iteration"], 0);
        assert_eq!(json["simulation_value"], 0.0);
    }

    #[test]
    fn test_mcts_result_empty_paths() {
        let result = MCTSExploreResult {
            session_id: "s".to_string(),
            root_node_id: "r".to_string(),
            best_path: vec![],
            best_path_content: vec![],
            best_path_value: 0.0,
            nodes_explored: 0,
            iteration_stats: vec![],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["best_path"].as_array().unwrap().is_empty());
    }
}
