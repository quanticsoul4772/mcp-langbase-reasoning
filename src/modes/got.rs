//! Graph-of-Thoughts (GoT) reasoning mode - complex graph-based exploration
//!
//! Implements operations for building and exploring reasoning graphs:
//! - Initialize: Create a new graph with root node
//! - Generate: Create k diverse continuations from a node
//! - Score: Evaluate node quality
//! - Aggregate: Merge multiple nodes into unified insight
//! - Refine: Improve a node through self-critique
//! - Prune: Remove low-scoring nodes
//! - Finalize: Mark terminal nodes and get conclusions

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::{
    GOT_AGGREGATE_PROMPT, GOT_GENERATE_PROMPT, GOT_REFINE_PROMPT, GOT_SCORE_PROMPT,
};
use crate::storage::{
    EdgeType, GraphEdge, GraphNode, Invocation, NodeType, SqliteStorage, Storage,
};

#[cfg(test)]
#[path = "got_tests.rs"]
mod got_tests;

/// Configuration for GoT operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotConfig {
    /// Maximum number of nodes in the graph
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
    /// Maximum depth of the graph
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// Default number of continuations to generate
    #[serde(default = "default_k")]
    pub default_k: usize,
    /// Score threshold for pruning
    #[serde(default = "default_prune_threshold")]
    pub prune_threshold: f64,
}

fn default_max_nodes() -> usize {
    100
}

fn default_max_depth() -> usize {
    10
}

fn default_k() -> usize {
    3
}

fn default_prune_threshold() -> f64 {
    0.3
}

impl Default for GotConfig {
    fn default() -> Self {
        Self {
            max_nodes: default_max_nodes(),
            max_depth: default_max_depth(),
            default_k: default_k(),
            prune_threshold: default_prune_threshold(),
        }
    }
}

// ============================================================================
// Initialize Operation
// ============================================================================

/// Parameters for initializing a GoT graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotInitParams {
    /// Initial thought content for the root node
    pub content: String,
    /// Problem context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// Optional session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Configuration overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<GotConfig>,
}

/// Result of initializing a GoT graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotInitResult {
    /// The session ID for the graph.
    pub session_id: String,
    /// The ID of the root node created.
    pub root_node_id: String,
    /// The initial thought content.
    pub content: String,
    /// The effective configuration for this graph.
    pub config: GotConfig,
}

// ============================================================================
// Generate Operation
// ============================================================================

/// Parameters for generating continuations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotGenerateParams {
    /// Session ID
    pub session_id: String,
    /// Node ID to generate from (uses active nodes if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Number of continuations to generate
    #[serde(default = "default_k")]
    pub k: usize,
    /// Problem context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// A generated continuation from a source node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedContinuation {
    /// The ID of the newly created node.
    pub node_id: String,
    /// The thought content of this continuation.
    pub content: String,
    /// Confidence score for this continuation (0.0-1.0).
    pub confidence: f64,
    /// Novelty score indicating how different this is from the source (0.0-1.0).
    pub novelty: f64,
    /// Explanation for why this continuation was generated.
    pub rationale: String,
}

/// Result of generating continuations from a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotGenerateResult {
    /// The session ID.
    pub session_id: String,
    /// The ID of the node that continuations were generated from.
    pub source_node_id: String,
    /// The generated continuations.
    pub continuations: Vec<GeneratedContinuation>,
    /// The number of continuations requested (k).
    pub count: usize,
}

/// Langbase response for generate operation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenerateResponse {
    continuations: Vec<ContinuationItem>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContinuationItem {
    thought: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default)]
    novelty: f64,
    #[serde(default)]
    rationale: String,
}

fn default_confidence() -> f64 {
    0.7
}

impl GenerateResponse {
    fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<GenerateResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse GoT generate response, using fallback"
                );
                // Fallback - create a single continuation from plain text
                Self {
                    continuations: vec![ContinuationItem {
                        thought: completion.to_string(),
                        confidence: 0.7,
                        novelty: 0.5,
                        rationale: "Generated from plain text response (parse fallback)"
                            .to_string(),
                    }],
                    metadata: None,
                }
            }
        }
    }
}

// ============================================================================
// Score Operation
// ============================================================================

/// Parameters for scoring a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotScoreParams {
    /// Session ID
    pub session_id: String,
    /// Node ID to score
    pub node_id: String,
    /// Problem context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Score breakdown for a node across multiple quality dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// How relevant the thought is to the problem (0.0-1.0).
    pub relevance: f64,
    /// How logically valid and sound the reasoning is (0.0-1.0).
    pub validity: f64,
    /// How deep and substantive the analysis is (0.0-1.0).
    pub depth: f64,
    /// How novel or creative the thought is (0.0-1.0).
    pub novelty: f64,
}

/// Result of scoring a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotScoreResult {
    /// The session ID.
    pub session_id: String,
    /// The ID of the scored node.
    pub node_id: String,
    /// The overall quality score (0.0-1.0).
    pub overall_score: f64,
    /// Detailed breakdown of the score across dimensions.
    pub breakdown: ScoreBreakdown,
    /// Whether this node is a good candidate for a terminal conclusion.
    pub is_terminal_candidate: bool,
    /// Explanation for the assigned score.
    pub rationale: String,
}

/// Langbase response for score operation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScoreResponse {
    overall_score: f64,
    breakdown: ScoreBreakdownResponse,
    #[serde(default)]
    is_terminal_candidate: bool,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScoreBreakdownResponse {
    #[serde(default = "default_score")]
    relevance: f64,
    #[serde(default = "default_score")]
    validity: f64,
    #[serde(default = "default_score")]
    depth: f64,
    #[serde(default = "default_score")]
    novelty: f64,
}

fn default_score() -> f64 {
    0.5
}

impl ScoreResponse {
    fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<ScoreResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse GoT score response, using fallback"
                );
                // Fallback
                Self {
                    overall_score: 0.5,
                    breakdown: ScoreBreakdownResponse {
                        relevance: 0.5,
                        validity: 0.5,
                        depth: 0.5,
                        novelty: 0.5,
                    },
                    is_terminal_candidate: false,
                    rationale: "Score determined from fallback (parse error)".to_string(),
                    metadata: None,
                }
            }
        }
    }
}

// ============================================================================
// Aggregate Operation
// ============================================================================

/// Parameters for aggregating nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotAggregateParams {
    /// Session ID
    pub session_id: String,
    /// Node IDs to aggregate
    pub node_ids: Vec<String>,
    /// Problem context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Result of aggregating multiple nodes into a unified insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotAggregateResult {
    /// The session ID.
    pub session_id: String,
    /// The ID of the newly created aggregated node.
    pub aggregated_node_id: String,
    /// The synthesized thought content.
    pub content: String,
    /// Confidence in the aggregated result (0.0-1.0).
    pub confidence: f64,
    /// IDs of the source nodes that were aggregated.
    pub source_nodes: Vec<String>,
    /// Description of how the synthesis was performed.
    pub synthesis_approach: String,
    /// Conflicts or contradictions that were resolved during aggregation.
    pub conflicts_resolved: Vec<String>,
}

/// Langbase response for aggregate operation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AggregateResponse {
    aggregated_thought: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default)]
    sources_used: Vec<String>,
    #[serde(default)]
    synthesis_approach: String,
    #[serde(default)]
    conflicts_resolved: Vec<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

impl AggregateResponse {
    fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<AggregateResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse GoT aggregate response, using fallback"
                );
                // Fallback
                Self {
                    aggregated_thought: completion.to_string(),
                    confidence: 0.7,
                    sources_used: vec![],
                    synthesis_approach: "Direct synthesis (parse fallback)".to_string(),
                    conflicts_resolved: vec![],
                    metadata: None,
                }
            }
        }
    }
}

// ============================================================================
// Refine Operation
// ============================================================================

/// Parameters for refining a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotRefineParams {
    /// Session ID
    pub session_id: String,
    /// Node ID to refine
    pub node_id: String,
    /// Problem context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Result of refining a node through self-critique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotRefineResult {
    /// The session ID.
    pub session_id: String,
    /// The ID of the original node that was refined.
    pub original_node_id: String,
    /// The ID of the newly created refined node.
    pub refined_node_id: String,
    /// The improved thought content.
    pub content: String,
    /// Confidence in the refined result (0.0-1.0).
    pub confidence: f64,
    /// List of improvements made during refinement.
    pub improvements_made: Vec<String>,
    /// Change in quality score (positive means improvement).
    pub quality_delta: f64,
}

/// Langbase response for refine operation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RefineResponse {
    refined_thought: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default)]
    improvements_made: Vec<String>,
    #[serde(default)]
    aspects_unchanged: Vec<String>,
    #[serde(default)]
    quality_delta: f64,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

impl RefineResponse {
    fn from_completion(completion: &str) -> Self {
        match serde_json::from_str::<RefineResponse>(completion) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(
                    error = %e,
                    completion_preview = %completion.chars().take(200).collect::<String>(),
                    "Failed to parse GoT refine response, using fallback"
                );
                // Fallback
                Self {
                    refined_thought: completion.to_string(),
                    confidence: 0.75,
                    improvements_made: vec!["Clarity improvement (parse fallback)".to_string()],
                    aspects_unchanged: vec![],
                    quality_delta: 0.1,
                    metadata: None,
                }
            }
        }
    }
}

// ============================================================================
// Prune Operation
// ============================================================================

/// Parameters for pruning low-scoring nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotPruneParams {
    /// Session ID
    pub session_id: String,
    /// Score threshold (nodes below this are pruned)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// Result of pruning low-scoring nodes from the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotPruneResult {
    /// The session ID.
    pub session_id: String,
    /// Number of nodes that were pruned.
    pub pruned_count: usize,
    /// Number of nodes remaining after pruning.
    pub remaining_count: usize,
    /// The score threshold that was used for pruning.
    pub threshold_used: f64,
    /// IDs of the nodes that were pruned.
    pub pruned_node_ids: Vec<String>,
}

// ============================================================================
// Finalize Operation
// ============================================================================

/// Parameters for finalizing the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotFinalizeParams {
    /// Session ID
    pub session_id: String,
    /// Node IDs to mark as terminal (if empty, auto-selects best nodes)
    #[serde(default)]
    pub terminal_node_ids: Vec<String>,
}

/// A terminal conclusion node representing a final insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConclusion {
    /// The ID of the terminal node.
    pub node_id: String,
    /// The thought content of the conclusion.
    pub content: String,
    /// The quality score of this node, if scored.
    pub score: Option<f64>,
    /// The depth in the graph where this conclusion resides.
    pub depth: i32,
}

/// Result of finalizing the graph and extracting conclusions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotFinalizeResult {
    /// The session ID.
    pub session_id: String,
    /// Number of terminal nodes marked as conclusions.
    pub terminal_count: usize,
    /// The final conclusions extracted from the graph.
    pub conclusions: Vec<TerminalConclusion>,
}

// ============================================================================
// Get State Operation
// ============================================================================

/// Parameters for getting graph state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotGetStateParams {
    /// Session ID
    pub session_id: String,
}

/// Graph state summary showing the current structure and status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotStateResult {
    /// The session ID.
    pub session_id: String,
    /// Total number of nodes in the graph.
    pub total_nodes: usize,
    /// Number of currently active (explorable) nodes.
    pub active_nodes: usize,
    /// Number of terminal (conclusion) nodes.
    pub terminal_nodes: usize,
    /// Total number of edges connecting nodes.
    pub total_edges: usize,
    /// Maximum depth reached in the graph.
    pub max_depth: i32,
    /// IDs of all root nodes.
    pub root_node_ids: Vec<String>,
    /// IDs of all currently active nodes.
    pub active_node_ids: Vec<String>,
    /// IDs of all terminal nodes.
    pub terminal_node_ids: Vec<String>,
}

// ============================================================================
// GoT Mode Handler
// ============================================================================

/// Graph-of-Thoughts mode handler for managing complex reasoning graphs.
#[derive(Clone)]
pub struct GotMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// Pipe name for the generate operation.
    generate_pipe: String,
    /// Pipe name for the score operation.
    score_pipe: String,
    /// Pipe name for the aggregate operation.
    aggregate_pipe: String,
    /// Pipe name for the refine operation.
    refine_pipe: String,
    /// Configuration for GoT operations.
    config: GotConfig,
}

impl GotMode {
    /// Create a new GoT mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        let got_config = config
            .pipes
            .got
            .as_ref()
            .map(|g| GotConfig {
                max_nodes: g.max_nodes.unwrap_or_else(default_max_nodes),
                max_depth: g.max_depth.unwrap_or_else(default_max_depth),
                default_k: g.default_k.unwrap_or_else(default_k),
                prune_threshold: g.prune_threshold.unwrap_or_else(default_prune_threshold),
            })
            .unwrap_or_default();

        Self {
            core: ModeCore::new(storage, langbase),
            generate_pipe: config
                .pipes
                .got
                .as_ref()
                .and_then(|g| g.generate_pipe.clone())
                .unwrap_or_else(|| "got-generate-v1".to_string()),
            score_pipe: config
                .pipes
                .got
                .as_ref()
                .and_then(|g| g.score_pipe.clone())
                .unwrap_or_else(|| "got-score-v1".to_string()),
            aggregate_pipe: config
                .pipes
                .got
                .as_ref()
                .and_then(|g| g.aggregate_pipe.clone())
                .unwrap_or_else(|| "got-aggregate-v1".to_string()),
            refine_pipe: config
                .pipes
                .got
                .as_ref()
                .and_then(|g| g.refine_pipe.clone())
                .unwrap_or_else(|| "got-refine-v1".to_string()),
            config: got_config,
        }
    }

    /// Initialize a new GoT graph
    pub async fn initialize(&self, params: GotInitParams) -> AppResult<GotInitResult> {
        let start = Instant::now();

        // Validate input
        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Content cannot be empty".to_string(),
            }
            .into());
        }

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "got")
            .await?;

        // Merge config with params override
        let effective_config = params.config.unwrap_or_else(|| self.config.clone());

        // Create root node
        let root_node = GraphNode::new(&session.id, &params.content)
            .with_type(NodeType::Root)
            .with_depth(0)
            .as_root()
            .as_active();

        self.core.storage().create_graph_node(&root_node).await?;

        let latency = start.elapsed().as_millis() as i64;
        info!(
            session_id = %session.id,
            root_node_id = %root_node.id,
            latency_ms = latency,
            "GoT graph initialized"
        );

        Ok(GotInitResult {
            session_id: session.id,
            root_node_id: root_node.id,
            content: params.content,
            config: effective_config,
        })
    }

    /// Generate continuations from a node
    pub async fn generate(&self, params: GotGenerateParams) -> AppResult<GotGenerateResult> {
        let start = Instant::now();

        // Get source node (specified or first active)
        let source_node = match &params.node_id {
            Some(id) => self
                .core
                .storage()
                .get_graph_node(id)
                .await?
                .ok_or_else(|| ToolError::Validation {
                    field: "node_id".to_string(),
                    reason: format!("Node not found: {}", id),
                })?,
            None => {
                let active = self
                    .core
                    .storage()
                    .get_active_graph_nodes(&params.session_id)
                    .await?;
                active
                    .into_iter()
                    .next()
                    .ok_or_else(|| ToolError::Validation {
                        field: "session_id".to_string(),
                        reason: "No active nodes in session".to_string(),
                    })?
            }
        };

        debug!(
            session_id = %params.session_id,
            source_node_id = %source_node.id,
            k = params.k,
            "Generating GoT continuations"
        );

        // Check depth limit
        if source_node.depth >= self.config.max_depth as i32 {
            return Err(ToolError::Validation {
                field: "depth".to_string(),
                reason: format!("Maximum depth {} reached", self.config.max_depth),
            }
            .into());
        }

        // Build messages for Langbase
        let messages =
            self.build_generate_messages(&source_node, params.k, params.problem.as_deref());

        // Log invocation
        let mut invocation = Invocation::new(
            "reasoning.got.generate",
            serialize_for_log(&params, "reasoning.got.generate input"),
        )
        .with_session(&params.session_id)
        .with_pipe(&self.generate_pipe);

        // Call Langbase
        let request = PipeRequest::new(&self.generate_pipe, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
                    warn!(
                        error = %log_err,
                        tool = %invocation.tool_name,
                        "Failed to log invocation - audit trail incomplete"
                    );
                }
                return Err(e.into());
            }
        };

        // Parse response
        let gen_response = GenerateResponse::from_completion(&response.completion);

        // Create nodes and edges for each continuation
        let mut continuations = Vec::new();
        for item in gen_response.continuations.into_iter().take(params.k) {
            // Create new node
            let node = GraphNode::new(&params.session_id, &item.thought)
                .with_type(NodeType::Thought)
                .with_depth(source_node.depth + 1)
                .with_score(item.confidence)
                .as_active();

            self.core.storage().create_graph_node(&node).await?;

            // Create edge from source to new node
            let edge = GraphEdge::new(&params.session_id, &source_node.id, &node.id)
                .with_type(EdgeType::Generates)
                .with_weight(item.confidence);

            self.core.storage().create_graph_edge(&edge).await?;

            continuations.push(GeneratedContinuation {
                node_id: node.id,
                content: item.thought,
                confidence: item.confidence,
                novelty: item.novelty,
                rationale: item.rationale,
            });
        }

        // Mark source node as no longer active (branched)
        let mut updated_source = source_node.clone();
        updated_source.is_active = false;
        self.core
            .storage()
            .update_graph_node(&updated_source)
            .await?;

        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&continuations, "reasoning.got.generate output"),
            latency,
        );
        if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
            warn!(
                error = %log_err,
                tool = %invocation.tool_name,
                "Failed to log invocation - audit trail incomplete"
            );
        }

        info!(
            session_id = %params.session_id,
            source_node_id = %source_node.id,
            generated_count = continuations.len(),
            latency_ms = latency,
            "GoT generate completed"
        );

        Ok(GotGenerateResult {
            session_id: params.session_id,
            source_node_id: source_node.id,
            continuations,
            count: params.k,
        })
    }

    /// Score a node
    pub async fn score(&self, params: GotScoreParams) -> AppResult<GotScoreResult> {
        let start = Instant::now();

        // Get the node
        let node = self
            .core
            .storage()
            .get_graph_node(&params.node_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "node_id".to_string(),
                reason: format!("Node not found: {}", params.node_id),
            })?;

        debug!(
            session_id = %params.session_id,
            node_id = %node.id,
            "Scoring GoT node"
        );

        // Build messages for Langbase
        let messages = self.build_score_messages(&node, params.problem.as_deref());

        // Log invocation
        let mut invocation = Invocation::new(
            "reasoning.got.score",
            serialize_for_log(&params, "reasoning.got.score input"),
        )
        .with_session(&params.session_id)
        .with_pipe(&self.score_pipe);

        // Call Langbase
        let request = PipeRequest::new(&self.score_pipe, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
                    warn!(
                        error = %log_err,
                        tool = %invocation.tool_name,
                        "Failed to log invocation - audit trail incomplete"
                    );
                }
                return Err(e.into());
            }
        };

        // Parse response
        let score_response = ScoreResponse::from_completion(&response.completion);

        // Update node with score
        let mut updated_node = node.clone();
        updated_node.score = Some(score_response.overall_score);
        self.core.storage().update_graph_node(&updated_node).await?;

        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&score_response, "reasoning.got.score output"),
            latency,
        );
        if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
            warn!(
                error = %log_err,
                tool = %invocation.tool_name,
                "Failed to log invocation - audit trail incomplete"
            );
        }

        info!(
            session_id = %params.session_id,
            node_id = %node.id,
            score = score_response.overall_score,
            latency_ms = latency,
            "GoT score completed"
        );

        Ok(GotScoreResult {
            session_id: params.session_id,
            node_id: node.id,
            overall_score: score_response.overall_score,
            breakdown: ScoreBreakdown {
                relevance: score_response.breakdown.relevance,
                validity: score_response.breakdown.validity,
                depth: score_response.breakdown.depth,
                novelty: score_response.breakdown.novelty,
            },
            is_terminal_candidate: score_response.is_terminal_candidate,
            rationale: score_response.rationale,
        })
    }

    /// Aggregate multiple nodes
    pub async fn aggregate(&self, params: GotAggregateParams) -> AppResult<GotAggregateResult> {
        let start = Instant::now();

        if params.node_ids.len() < 2 {
            return Err(ToolError::Validation {
                field: "node_ids".to_string(),
                reason: "At least 2 nodes required for aggregation".to_string(),
            }
            .into());
        }

        // Get all nodes
        let mut nodes = Vec::new();
        for id in &params.node_ids {
            let node = self
                .core
                .storage()
                .get_graph_node(id)
                .await?
                .ok_or_else(|| ToolError::Validation {
                    field: "node_ids".to_string(),
                    reason: format!("Node not found: {}", id),
                })?;
            nodes.push(node);
        }

        debug!(
            session_id = %params.session_id,
            node_count = nodes.len(),
            "Aggregating GoT nodes"
        );

        // Build messages for Langbase
        let messages = self.build_aggregate_messages(&nodes, params.problem.as_deref());

        // Log invocation
        let mut invocation = Invocation::new(
            "reasoning.got.aggregate",
            serialize_for_log(&params, "reasoning.got.aggregate input"),
        )
        .with_session(&params.session_id)
        .with_pipe(&self.aggregate_pipe);

        // Call Langbase
        let request = PipeRequest::new(&self.aggregate_pipe, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
                    warn!(
                        error = %log_err,
                        tool = %invocation.tool_name,
                        "Failed to log invocation - audit trail incomplete"
                    );
                }
                return Err(e.into());
            }
        };

        // Parse response
        let agg_response = AggregateResponse::from_completion(&response.completion);

        // Find max depth of source nodes
        let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);

        // Create aggregated node
        let agg_node = GraphNode::new(&params.session_id, &agg_response.aggregated_thought)
            .with_type(NodeType::Aggregation)
            .with_depth(max_depth + 1)
            .with_score(agg_response.confidence)
            .as_active();

        self.core.storage().create_graph_node(&agg_node).await?;

        // Create edges from source nodes to aggregated node
        for node in &nodes {
            let edge = GraphEdge::new(&params.session_id, &node.id, &agg_node.id)
                .with_type(EdgeType::Aggregates);
            self.core.storage().create_graph_edge(&edge).await?;

            // Mark source nodes as no longer active
            let mut updated = node.clone();
            updated.is_active = false;
            self.core.storage().update_graph_node(&updated).await?;
        }

        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&agg_response, "reasoning.got.aggregate output"),
            latency,
        );
        if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
            warn!(
                error = %log_err,
                tool = %invocation.tool_name,
                "Failed to log invocation - audit trail incomplete"
            );
        }

        info!(
            session_id = %params.session_id,
            aggregated_node_id = %agg_node.id,
            source_count = nodes.len(),
            latency_ms = latency,
            "GoT aggregate completed"
        );

        Ok(GotAggregateResult {
            session_id: params.session_id,
            aggregated_node_id: agg_node.id,
            content: agg_response.aggregated_thought,
            confidence: agg_response.confidence,
            source_nodes: params.node_ids,
            synthesis_approach: agg_response.synthesis_approach,
            conflicts_resolved: agg_response.conflicts_resolved,
        })
    }

    /// Refine a node
    pub async fn refine(&self, params: GotRefineParams) -> AppResult<GotRefineResult> {
        let start = Instant::now();

        // Get the node
        let node = self
            .core
            .storage()
            .get_graph_node(&params.node_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "node_id".to_string(),
                reason: format!("Node not found: {}", params.node_id),
            })?;

        debug!(
            session_id = %params.session_id,
            node_id = %node.id,
            "Refining GoT node"
        );

        // Build messages for Langbase
        let messages = self.build_refine_messages(&node, params.problem.as_deref());

        // Log invocation
        let mut invocation = Invocation::new(
            "reasoning.got.refine",
            serialize_for_log(&params, "reasoning.got.refine input"),
        )
        .with_session(&params.session_id)
        .with_pipe(&self.refine_pipe);

        // Call Langbase
        let request = PipeRequest::new(&self.refine_pipe, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
                    warn!(
                        error = %log_err,
                        tool = %invocation.tool_name,
                        "Failed to log invocation - audit trail incomplete"
                    );
                }
                return Err(e.into());
            }
        };

        // Parse response
        let refine_response = RefineResponse::from_completion(&response.completion);

        // Create refined node
        let refined_node = GraphNode::new(&params.session_id, &refine_response.refined_thought)
            .with_type(NodeType::Refinement)
            .with_depth(node.depth) // Same depth as original
            .with_score(refine_response.confidence)
            .as_active();

        self.core.storage().create_graph_node(&refined_node).await?;

        // Create edge from original to refined
        let edge = GraphEdge::new(&params.session_id, &node.id, &refined_node.id)
            .with_type(EdgeType::Refines);
        self.core.storage().create_graph_edge(&edge).await?;

        // Mark original as no longer active
        let mut updated_node = node.clone();
        updated_node.is_active = false;
        self.core.storage().update_graph_node(&updated_node).await?;

        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&refine_response, "reasoning.got.refine output"),
            latency,
        );
        if let Err(log_err) = self.core.storage().log_invocation(&invocation).await {
            warn!(
                error = %log_err,
                tool = %invocation.tool_name,
                "Failed to log invocation - audit trail incomplete"
            );
        }

        info!(
            session_id = %params.session_id,
            original_node_id = %node.id,
            refined_node_id = %refined_node.id,
            quality_delta = refine_response.quality_delta,
            latency_ms = latency,
            "GoT refine completed"
        );

        Ok(GotRefineResult {
            session_id: params.session_id,
            original_node_id: node.id,
            refined_node_id: refined_node.id,
            content: refine_response.refined_thought,
            confidence: refine_response.confidence,
            improvements_made: refine_response.improvements_made,
            quality_delta: refine_response.quality_delta,
        })
    }

    /// Prune low-scoring nodes
    pub async fn prune(&self, params: GotPruneParams) -> AppResult<GotPruneResult> {
        let start = Instant::now();

        let threshold = params.threshold.unwrap_or(self.config.prune_threshold);

        // Get all nodes for session
        let nodes = self
            .core
            .storage()
            .get_session_graph_nodes(&params.session_id)
            .await?;

        // Find nodes to prune (low score, not root, not terminal)
        let mut pruned_ids = Vec::new();
        for node in &nodes {
            // Skip root and terminal nodes
            if node.is_root || node.is_terminal {
                continue;
            }

            // Prune if score is below threshold (or unscored nodes)
            if let Some(score) = node.score {
                if score < threshold {
                    // Check if this node has children (don't prune if it does)
                    let children = self.core.storage().get_edges_from(&node.id).await?;
                    if children.is_empty() {
                        pruned_ids.push(node.id.clone());
                    }
                }
            }
        }

        // Delete pruned nodes and their edges
        for id in &pruned_ids {
            // Delete edges to/from this node
            let edges_from = self.core.storage().get_edges_from(id).await?;
            let edges_to = self.core.storage().get_edges_to(id).await?;

            for edge in edges_from.iter().chain(edges_to.iter()) {
                self.core.storage().delete_graph_edge(&edge.id).await?;
            }

            // Delete the node
            self.core.storage().delete_graph_node(id).await?;
        }

        let remaining_count = nodes.len() - pruned_ids.len();
        let latency = start.elapsed().as_millis() as i64;

        info!(
            session_id = %params.session_id,
            pruned_count = pruned_ids.len(),
            remaining_count = remaining_count,
            threshold = threshold,
            latency_ms = latency,
            "GoT prune completed"
        );

        Ok(GotPruneResult {
            session_id: params.session_id,
            pruned_count: pruned_ids.len(),
            remaining_count,
            threshold_used: threshold,
            pruned_node_ids: pruned_ids,
        })
    }

    /// Finalize the graph and get conclusions
    pub async fn finalize(&self, params: GotFinalizeParams) -> AppResult<GotFinalizeResult> {
        let start = Instant::now();

        let nodes_to_finalize = if params.terminal_node_ids.is_empty() {
            // Auto-select best active nodes as terminal
            let active = self
                .core
                .storage()
                .get_active_graph_nodes(&params.session_id)
                .await?;
            let mut scored: Vec<_> = active.into_iter().filter(|n| n.score.is_some()).collect();
            scored.sort_by(|a, b| {
                b.score
                    .unwrap_or(0.0)
                    .partial_cmp(&a.score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            // Take top 3 or all if fewer
            scored.into_iter().take(3).collect::<Vec<_>>()
        } else {
            // Use specified nodes
            let mut nodes = Vec::new();
            for id in &params.terminal_node_ids {
                let node = self
                    .core
                    .storage()
                    .get_graph_node(id)
                    .await?
                    .ok_or_else(|| ToolError::Validation {
                        field: "terminal_node_ids".to_string(),
                        reason: format!("Node not found: {}", id),
                    })?;
                nodes.push(node);
            }
            nodes
        };

        // Mark nodes as terminal
        let mut conclusions = Vec::new();
        for node in nodes_to_finalize {
            let mut updated = node.clone();
            updated.is_terminal = true;
            updated.is_active = false;
            updated.node_type = NodeType::Terminal;
            self.core.storage().update_graph_node(&updated).await?;

            conclusions.push(TerminalConclusion {
                node_id: node.id,
                content: node.content,
                score: node.score,
                depth: node.depth,
            });
        }

        let latency = start.elapsed().as_millis() as i64;
        info!(
            session_id = %params.session_id,
            terminal_count = conclusions.len(),
            latency_ms = latency,
            "GoT finalize completed"
        );

        Ok(GotFinalizeResult {
            session_id: params.session_id,
            terminal_count: conclusions.len(),
            conclusions,
        })
    }

    /// Get current graph state
    pub async fn get_state(&self, params: GotGetStateParams) -> AppResult<GotStateResult> {
        let nodes = self
            .core
            .storage()
            .get_session_graph_nodes(&params.session_id)
            .await?;
        let edges = self
            .core
            .storage()
            .get_session_edges(&params.session_id)
            .await?;

        let active_nodes: Vec<_> = nodes.iter().filter(|n| n.is_active).collect();
        let terminal_nodes: Vec<_> = nodes.iter().filter(|n| n.is_terminal).collect();
        let root_nodes: Vec<_> = nodes.iter().filter(|n| n.is_root).collect();
        let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);

        Ok(GotStateResult {
            session_id: params.session_id,
            total_nodes: nodes.len(),
            active_nodes: active_nodes.len(),
            terminal_nodes: terminal_nodes.len(),
            total_edges: edges.len(),
            max_depth,
            root_node_ids: root_nodes.iter().map(|n| n.id.clone()).collect(),
            active_node_ids: active_nodes.iter().map(|n| n.id.clone()).collect(),
            terminal_node_ids: terminal_nodes.iter().map(|n| n.id.clone()).collect(),
        })
    }

    /// Detect cycles in the graph (returns true if cycle exists)
    pub async fn has_cycle(&self, session_id: &str) -> AppResult<bool> {
        let nodes = self
            .core
            .storage()
            .get_session_graph_nodes(session_id)
            .await?;
        let edges = self.core.storage().get_session_edges(session_id).await?;

        // Build adjacency list
        let mut adj: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for edge in &edges {
            adj.entry(edge.from_node.clone())
                .or_default()
                .push(edge.to_node.clone());
        }

        // DFS-based cycle detection
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        fn dfs(
            node: &str,
            adj: &std::collections::HashMap<String, Vec<String>>,
            visited: &mut HashSet<String>,
            rec_stack: &mut HashSet<String>,
        ) -> bool {
            visited.insert(node.to_string());
            rec_stack.insert(node.to_string());

            if let Some(neighbors) = adj.get(node) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        if dfs(neighbor, adj, visited, rec_stack) {
                            return true;
                        }
                    } else if rec_stack.contains(neighbor) {
                        return true;
                    }
                }
            }

            rec_stack.remove(node);
            false
        }

        for node in &nodes {
            if !visited.contains(&node.id) && dfs(&node.id, &adj, &mut visited, &mut rec_stack) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    // ========================================================================
    // Helper methods for building Langbase messages
    // ========================================================================

    fn build_generate_messages(
        &self,
        source_node: &GraphNode,
        k: usize,
        problem: Option<&str>,
    ) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(GOT_GENERATE_PROMPT));

        let mut user_msg = format!(
            "Generate {} diverse continuations from this thought:\n\n\"{}\"",
            k, source_node.content
        );

        if let Some(p) = problem {
            user_msg.push_str(&format!("\n\nProblem context: {}", p));
        }

        user_msg.push_str(&format!("\n\nCurrent depth: {}", source_node.depth));

        messages.push(Message::user(user_msg));
        messages
    }

    fn build_score_messages(&self, node: &GraphNode, problem: Option<&str>) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(GOT_SCORE_PROMPT));

        let mut user_msg = format!("Score this thought:\n\n\"{}\"", node.content);

        if let Some(p) = problem {
            user_msg.push_str(&format!("\n\nProblem context: {}", p));
        }

        user_msg.push_str(&format!("\n\nDepth: {}", node.depth));
        if let Some(score) = node.score {
            user_msg.push_str(&format!("\nPrevious score: {}", score));
        }

        messages.push(Message::user(user_msg));
        messages
    }

    fn build_aggregate_messages(&self, nodes: &[GraphNode], problem: Option<&str>) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(GOT_AGGREGATE_PROMPT));

        let thoughts: Vec<String> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{}. \"{}\"", i + 1, n.content))
            .collect();

        let mut user_msg = format!(
            "Aggregate these {} thoughts into a unified insight:\n\n{}",
            nodes.len(),
            thoughts.join("\n\n")
        );

        if let Some(p) = problem {
            user_msg.push_str(&format!("\n\nProblem context: {}", p));
        }

        messages.push(Message::user(user_msg));
        messages
    }

    fn build_refine_messages(&self, node: &GraphNode, problem: Option<&str>) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(GOT_REFINE_PROMPT));

        let mut user_msg = format!("Refine and improve this thought:\n\n\"{}\"", node.content);

        if let Some(p) = problem {
            user_msg.push_str(&format!("\n\nProblem context: {}", p));
        }

        if let Some(score) = node.score {
            user_msg.push_str(&format!("\n\nCurrent score: {:.2}", score));
        }

        messages.push(Message::user(user_msg));
        messages
    }
}

// ============================================================================
// Builder implementations
// ============================================================================

impl GotInitParams {
    /// Create new initialization parameters with the given content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            problem: None,
            session_id: None,
            config: None,
        }
    }

    /// Set the problem context.
    pub fn with_problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the configuration overrides.
    pub fn with_config(mut self, config: GotConfig) -> Self {
        self.config = Some(config);
        self
    }
}

impl GotGenerateParams {
    /// Create new generate parameters for the given session.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            node_id: None,
            k: default_k(),
            problem: None,
        }
    }

    /// Set the specific node to generate from.
    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Set the number of continuations to generate.
    pub fn with_k(mut self, k: usize) -> Self {
        self.k = k;
        self
    }

    /// Set the problem context.
    pub fn with_problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }
}

impl GotScoreParams {
    /// Create new score parameters for the given session and node.
    pub fn new(session_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            node_id: node_id.into(),
            problem: None,
        }
    }

    /// Set the problem context.
    pub fn with_problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }
}

impl GotAggregateParams {
    /// Create new aggregate parameters for the given session and nodes.
    pub fn new(session_id: impl Into<String>, node_ids: Vec<String>) -> Self {
        Self {
            session_id: session_id.into(),
            node_ids,
            problem: None,
        }
    }

    /// Set the problem context.
    pub fn with_problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }
}

impl GotRefineParams {
    /// Create new refine parameters for the given session and node.
    pub fn new(session_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            node_id: node_id.into(),
            problem: None,
        }
    }

    /// Set the problem context.
    pub fn with_problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }
}

impl GotPruneParams {
    /// Create new prune parameters for the given session.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            threshold: None,
        }
    }

    /// Set the score threshold for pruning.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = Some(threshold);
        self
    }
}

impl GotFinalizeParams {
    /// Create new finalize parameters for the given session.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            terminal_node_ids: vec![],
        }
    }

    /// Set the specific nodes to mark as terminal conclusions.
    pub fn with_terminal_nodes(mut self, node_ids: Vec<String>) -> Self {
        self.terminal_node_ids = node_ids;
        self
    }
}

impl GotGetStateParams {
    /// Create new get state parameters for the given session.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}
