//! Timeline reasoning mode - temporal exploration and branching.
//!
//! This module provides timeline-based reasoning for:
//! - Creating named timelines for organizing reasoning paths
//! - Branching at decision points with MCTS-based exploration
//! - Comparing and merging timeline branches
//! - Tracking branch performance via UCB scores

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::TREE_REASONING_PROMPT;
use crate::storage::{
    Branch, Invocation, SqliteStorage, Storage, Thought, Timeline, TimelineBranch, TimelineState,
};

/// Input parameters for creating a timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCreateParams {
    /// Name for the timeline
    pub name: String,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Initial thought content
    pub content: String,
    /// Optional session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Input parameters for branching a timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranchParams {
    /// Timeline ID to branch from
    pub timeline_id: String,
    /// Content for the new branch
    pub content: String,
    /// Number of alternatives to generate (2-4)
    #[serde(default = "default_num_alternatives")]
    pub num_alternatives: usize,
    /// Exploration constant for UCB calculation (default: sqrt(2))
    #[serde(default = "default_exploration_constant")]
    pub exploration_constant: f64,
}

fn default_num_alternatives() -> usize {
    3
}

fn default_exploration_constant() -> f64 {
    std::f64::consts::SQRT_2
}

/// Input parameters for comparing timelines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCompareParams {
    /// First timeline or branch to compare
    pub timeline_a: String,
    /// Second timeline or branch to compare
    pub timeline_b: String,
    /// Optional session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Input parameters for merging timelines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineMergeParams {
    /// Source timeline/branch to merge from
    pub source_id: String,
    /// Target timeline/branch to merge into
    pub target_id: String,
    /// Optional merge strategy
    #[serde(default)]
    pub strategy: MergeStrategy,
}

/// Merge strategies for timeline consolidation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Keep best insights from both (default)
    #[default]
    Synthesize,
    /// Prefer source timeline
    PreferSource,
    /// Prefer target timeline
    PreferTarget,
}

/// Response from timeline creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCreateResult {
    /// The created timeline ID
    pub timeline_id: String,
    /// The session ID
    pub session_id: String,
    /// The root branch ID
    pub root_branch_id: String,
    /// Initial thought content
    pub content: String,
    /// Timeline name
    pub name: String,
}

/// Response from timeline branching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranchResult {
    /// The timeline ID
    pub timeline_id: String,
    /// Created branches with their UCB scores
    pub branches: Vec<BranchWithScore>,
    /// Recommended branch index (based on UCB)
    pub recommended_index: usize,
    /// Total visit count for parent
    pub parent_visits: i32,
}

/// Branch information with MCTS score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchWithScore {
    /// Branch ID
    pub branch_id: String,
    /// Thought content
    pub content: String,
    /// UCB score
    pub ucb_score: f64,
    /// Visit count
    pub visit_count: i32,
    /// Confidence score
    pub confidence: f64,
}

/// Response from timeline comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCompareResult {
    /// Comparison summary
    pub summary: String,
    /// Key differences
    pub differences: Vec<String>,
    /// Shared insights
    pub shared_insights: Vec<String>,
    /// Recommendation on which path to pursue
    pub recommendation: String,
    /// Confidence in recommendation
    pub confidence: f64,
}

/// Response from timeline merge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineMergeResult {
    /// The merged timeline/branch ID
    pub merged_id: String,
    /// Synthesized content
    pub content: String,
    /// Elements preserved from source
    pub from_source: Vec<String>,
    /// Elements preserved from target
    pub from_target: Vec<String>,
    /// New insights from synthesis
    pub synthesized_insights: Vec<String>,
}

/// Timeline mode handler for temporal reasoning exploration.
#[derive(Clone)]
pub struct TimelineMode {
    /// Core infrastructure
    core: ModeCore,
    /// Tree pipe for branching
    tree_pipe: String,
    /// Divergent pipe for alternatives.
    /// Reserved for future multi-perspective branch generation.
    #[allow(dead_code)]
    divergent_pipe: String,
    /// GoT pipe for comparison
    got_pipe: String,
    /// Reflection pipe for synthesis
    reflection_pipe: String,
}

impl TimelineMode {
    /// Create a new timeline mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        // Extract pipe names from config, with defaults
        let got_pipe = config
            .pipes
            .got
            .as_ref()
            .and_then(|c| c.pipe.clone())
            .unwrap_or_else(|| "got-reasoning-v1".to_string());

        Self {
            core: ModeCore::new(storage, langbase),
            tree_pipe: config.pipes.tree.clone(),
            divergent_pipe: config.pipes.divergent.clone(),
            got_pipe,
            reflection_pipe: config.pipes.reflection.clone(),
        }
    }

    /// Create a new timeline
    pub async fn create(&self, params: TimelineCreateParams) -> AppResult<TimelineCreateResult> {
        let start = Instant::now();

        // Validate input
        if params.name.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "name".to_string(),
                reason: "Timeline name cannot be empty".to_string(),
            }
            .into());
        }

        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Initial content cannot be empty".to_string(),
            }
            .into());
        }

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "timeline")
            .await?;
        debug!(session_id = %session.id, "Creating timeline");

        // Create root branch
        let root_branch = Branch::new(&session.id)
            .with_name(format!("{} - Root", params.name))
            .with_confidence(1.0);
        self.core.storage().create_branch(&root_branch).await?;

        // Create timeline
        let mut timeline = Timeline::new(&session.id, &params.name, &root_branch.id);
        if let Some(ref desc) = params.description {
            timeline = timeline.with_description(desc);
        }
        self.core.storage().create_timeline(&timeline).await?;

        // Create timeline branch metadata (depth 0 for root)
        let timeline_branch = TimelineBranch::new(&root_branch.id, &timeline.id, 0);
        self.core
            .storage()
            .create_timeline_branch(&timeline_branch)
            .await?;

        // Create initial thought
        let thought = Thought::new(&session.id, &params.content, "timeline")
            .with_branch(&root_branch.id)
            .with_confidence(1.0);
        self.core.storage().create_thought(&thought).await?;

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_timeline_create",
            serialize_for_log(&params, "timeline_create_params"),
        )
        .with_session(&session.id)
        .success(serde_json::json!({"timeline_id": timeline.id}), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            timeline_id = %timeline.id,
            session_id = %session.id,
            latency_ms = latency,
            "Timeline created"
        );

        Ok(TimelineCreateResult {
            timeline_id: timeline.id,
            session_id: session.id,
            root_branch_id: root_branch.id,
            content: params.content,
            name: params.name,
        })
    }

    /// Branch a timeline with MCTS-based exploration
    pub async fn branch(&self, params: TimelineBranchParams) -> AppResult<TimelineBranchResult> {
        let start = Instant::now();

        // Validate
        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Branch content cannot be empty".to_string(),
            }
            .into());
        }

        let num_alternatives = params.num_alternatives.clamp(2, 4);

        // Get timeline
        let timeline = self
            .core
            .storage()
            .get_timeline(&params.timeline_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "timeline_id".to_string(),
                reason: format!("Timeline not found: {}", params.timeline_id),
            })?;

        // Get active branch's timeline metadata
        let parent_timeline_branch = self
            .core
            .storage()
            .get_timeline_branch(&timeline.active_branch_id)
            .await?
            .unwrap_or_else(|| TimelineBranch::new(&timeline.active_branch_id, &timeline.id, 0));

        // Build prompt for tree reasoning with MCTS context
        let mcts_context = format!(
            "Generate {} alternative reasoning paths for exploration. \
             Context: Parent has {} visits with total value {:.2}. \
             Use UCB1 scoring to balance exploration vs exploitation.\n\n\
             Content to branch from:\n{}",
            num_alternatives,
            parent_timeline_branch.visit_count,
            parent_timeline_branch.total_value,
            params.content
        );

        // Call tree reasoning pipe
        let messages = vec![
            Message::system(TREE_REASONING_PROMPT),
            Message::user(mcts_context),
        ];
        let request = PipeRequest::new(&self.tree_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let json_str = extract_json_from_completion(&response.completion)
            .map_err(|e| ToolError::Reasoning { message: e })?;
        let tree_response: TreeBranchResponse = serde_json::from_str(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse tree response: {}", e),
            }
        })?;

        // Create branches with UCB scores
        let mut created_branches = Vec::new();
        for (i, branch_data) in tree_response.branches.iter().enumerate() {
            // Create storage branch
            let branch = Branch::new(&timeline.session_id)
                .with_parent(&timeline.active_branch_id)
                .with_name(format!("Alternative {}", i + 1))
                .with_confidence(branch_data.confidence);
            self.core.storage().create_branch(&branch).await?;

            // Create timeline branch with MCTS metadata
            let mut timeline_branch = TimelineBranch::new(
                &branch.id,
                &timeline.id,
                parent_timeline_branch.depth + 1,
            );

            // Calculate initial UCB score
            let ucb = timeline_branch.calculate_ucb(
                parent_timeline_branch.visit_count.max(1),
                params.exploration_constant,
            );
            timeline_branch.ucb_score = Some(ucb);

            self.core
                .storage()
                .create_timeline_branch(&timeline_branch)
                .await?;

            // Create thought for branch
            let thought = Thought::new(&timeline.session_id, &branch_data.thought, "timeline")
                .with_branch(&branch.id)
                .with_confidence(branch_data.confidence);
            self.core.storage().create_thought(&thought).await?;

            created_branches.push(BranchWithScore {
                branch_id: branch.id,
                content: branch_data.thought.clone(),
                ucb_score: ucb,
                visit_count: 0,
                confidence: branch_data.confidence,
            });
        }

        // Find recommended branch (highest UCB)
        let recommended_index = created_branches
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.ucb_score.partial_cmp(&b.ucb_score).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        // Update timeline stats
        let mut updated_timeline = timeline.clone();
        updated_timeline.branch_count += created_branches.len() as i32;
        updated_timeline.max_depth = updated_timeline
            .max_depth
            .max(parent_timeline_branch.depth + 1);
        self.core.storage().update_timeline(&updated_timeline).await?;

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_timeline_branch",
            serialize_for_log(&params, "timeline_branch_params"),
        )
        .with_session(&timeline.session_id)
        .with_pipe(&self.tree_pipe)
        .success(serde_json::json!({"branches": created_branches.len()}), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            timeline_id = %timeline.id,
            branches_created = created_branches.len(),
            recommended_index = recommended_index,
            latency_ms = latency,
            "Timeline branched"
        );

        Ok(TimelineBranchResult {
            timeline_id: timeline.id,
            branches: created_branches,
            recommended_index,
            parent_visits: parent_timeline_branch.visit_count,
        })
    }

    /// Compare two timeline branches
    pub async fn compare(
        &self,
        params: TimelineCompareParams,
    ) -> AppResult<TimelineCompareResult> {
        let start = Instant::now();

        // Get session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "timeline")
            .await?;

        // Get thoughts from both branches
        let thoughts_a = self
            .core
            .storage()
            .get_branch_thoughts(&params.timeline_a)
            .await?;
        let thoughts_b = self
            .core
            .storage()
            .get_branch_thoughts(&params.timeline_b)
            .await?;

        if thoughts_a.is_empty() && thoughts_b.is_empty() {
            return Err(ToolError::Validation {
                field: "timeline_a, timeline_b".to_string(),
                reason: "Both timelines have no thoughts to compare".to_string(),
            }
            .into());
        }

        // Build comparison prompt
        let content_a: Vec<String> = thoughts_a.iter().map(|t| t.content.clone()).collect();
        let content_b: Vec<String> = thoughts_b.iter().map(|t| t.content.clone()).collect();

        let compare_prompt = format!(
            "Compare these two reasoning paths and identify key differences, shared insights, \
             and provide a recommendation on which path to pursue:\n\n\
             PATH A:\n{}\n\n\
             PATH B:\n{}",
            content_a.join("\n---\n"),
            content_b.join("\n---\n")
        );

        // Use GoT pipe for comparison (aggregation capability)
        let messages = vec![
            Message::system(
                "You are analyzing and comparing reasoning paths. \
                 Respond with JSON: {\"summary\": \"...\", \"differences\": [...], \
                 \"shared_insights\": [...], \"recommendation\": \"...\", \"confidence\": 0.0-1.0}"
            ),
            Message::user(compare_prompt),
        ];
        let request = PipeRequest::new(&self.got_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let json_str = extract_json_from_completion(&response.completion)
            .map_err(|e| ToolError::Reasoning { message: e })?;
        let compare_response: CompareResponse = serde_json::from_str(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse comparison: {}", e),
            }
        })?;

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_timeline_compare",
            serialize_for_log(&params, "timeline_compare_params"),
        )
        .with_session(&session.id)
        .with_pipe(&self.got_pipe)
        .success(serialize_for_log(&compare_response, "compare_result"), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            timeline_a = %params.timeline_a,
            timeline_b = %params.timeline_b,
            latency_ms = latency,
            "Timelines compared"
        );

        Ok(TimelineCompareResult {
            summary: compare_response.summary,
            differences: compare_response.differences,
            shared_insights: compare_response.shared_insights,
            recommendation: compare_response.recommendation,
            confidence: compare_response.confidence,
        })
    }

    /// Merge two timeline branches
    pub async fn merge(&self, params: TimelineMergeParams) -> AppResult<TimelineMergeResult> {
        let start = Instant::now();

        // Get thoughts from both branches
        let source_thoughts = self
            .core
            .storage()
            .get_branch_thoughts(&params.source_id)
            .await?;
        let target_thoughts = self
            .core
            .storage()
            .get_branch_thoughts(&params.target_id)
            .await?;

        // Get source branch to find session
        let source_branch = self
            .core
            .storage()
            .get_branch(&params.source_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "source_id".to_string(),
                reason: format!("Source branch not found: {}", params.source_id),
            })?;

        // Build merge prompt based on strategy
        let strategy_instruction = match params.strategy {
            MergeStrategy::Synthesize => {
                "Synthesize the best insights from both paths into a unified conclusion."
            }
            MergeStrategy::PreferSource => {
                "Prefer insights from the source path, supplementing with target where valuable."
            }
            MergeStrategy::PreferTarget => {
                "Prefer insights from the target path, supplementing with source where valuable."
            }
        };

        let source_content: Vec<String> =
            source_thoughts.iter().map(|t| t.content.clone()).collect();
        let target_content: Vec<String> =
            target_thoughts.iter().map(|t| t.content.clone()).collect();

        let merge_prompt = format!(
            "Merge these two reasoning paths. Strategy: {}\n\n\
             SOURCE PATH:\n{}\n\n\
             TARGET PATH:\n{}",
            strategy_instruction,
            source_content.join("\n---\n"),
            target_content.join("\n---\n")
        );

        // Use reflection pipe for synthesis
        let messages = vec![
            Message::system(
                "You are merging reasoning paths. \
                 Respond with JSON: {\"content\": \"merged insight\", \
                 \"from_source\": [...], \"from_target\": [...], \"synthesized_insights\": [...]}"
            ),
            Message::user(merge_prompt),
        ];
        let request = PipeRequest::new(&self.reflection_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let json_str = extract_json_from_completion(&response.completion)
            .map_err(|e| ToolError::Reasoning { message: e })?;
        let merge_response: MergeResponse = serde_json::from_str(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse merge response: {}", e),
            }
        })?;

        // Create merged branch
        let merged_branch = Branch::new(&source_branch.session_id)
            .with_parent(&params.target_id)
            .with_name("Merged")
            .with_confidence(0.9);
        self.core.storage().create_branch(&merged_branch).await?;

        // Create merged thought
        let thought = Thought::new(&source_branch.session_id, &merge_response.content, "timeline")
            .with_branch(&merged_branch.id)
            .with_confidence(0.9);
        self.core.storage().create_thought(&thought).await?;

        // Mark source as merged if it's a timeline branch
        if let Some(timeline_branch) = self
            .core
            .storage()
            .get_timeline_branch(&params.source_id)
            .await?
        {
            if let Some(timeline) = self.core.storage().get_timeline(&timeline_branch.timeline_id).await? {
                let mut updated = timeline;
                updated.state = TimelineState::Merged;
                self.core.storage().update_timeline(&updated).await?;
            }
        }

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_timeline_merge",
            serialize_for_log(&params, "timeline_merge_params"),
        )
        .with_session(&source_branch.session_id)
        .with_pipe(&self.reflection_pipe)
        .success(serialize_for_log(&merge_response, "merge_result"), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            source_id = %params.source_id,
            target_id = %params.target_id,
            merged_id = %merged_branch.id,
            latency_ms = latency,
            "Timelines merged"
        );

        Ok(TimelineMergeResult {
            merged_id: merged_branch.id,
            content: merge_response.content,
            from_source: merge_response.from_source,
            from_target: merge_response.from_target,
            synthesized_insights: merge_response.synthesized_insights,
        })
    }
}

// Internal response types for parsing Langbase responses

#[derive(Debug, Deserialize)]
struct TreeBranchResponse {
    branches: Vec<TreeBranchData>,
}

/// Branch data from tree pipe response.
/// The `rationale` field is parsed for completeness but not currently used.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // rationale parsed for completeness
struct TreeBranchData {
    thought: String,
    confidence: f64,
    #[serde(default)]
    rationale: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompareResponse {
    summary: String,
    differences: Vec<String>,
    shared_insights: Vec<String>,
    recommendation: String,
    confidence: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct MergeResponse {
    content: String,
    from_source: Vec<String>,
    from_target: Vec<String>,
    synthesized_insights: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============================================================================
    // TimelineCreateParams Tests
    // ============================================================================

    #[test]
    fn test_timeline_create_params_deserialize() {
        let json = json!({
            "name": "My Timeline",
            "content": "Initial thought",
            "description": "A test timeline"
        });
        let params: TimelineCreateParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.name, "My Timeline");
        assert_eq!(params.content, "Initial thought");
        assert_eq!(params.description, Some("A test timeline".to_string()));
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_timeline_create_params_minimal() {
        let json = json!({
            "name": "Test",
            "content": "Content"
        });
        let params: TimelineCreateParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.name, "Test");
        assert_eq!(params.content, "Content");
        assert!(params.description.is_none());
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_timeline_create_params_with_session() {
        let json = json!({
            "name": "Timeline",
            "content": "Test content",
            "session_id": "sess-123"
        });
        let params: TimelineCreateParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_timeline_create_params_serialize() {
        let params = TimelineCreateParams {
            name: "Test".to_string(),
            content: "Content".to_string(),
            description: Some("Desc".to_string()),
            session_id: None,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["name"], "Test");
        assert_eq!(json["content"], "Content");
        assert_eq!(json["description"], "Desc");
        assert!(json.get("session_id").is_none()); // skip_serializing_if
    }

    #[test]
    fn test_timeline_create_params_unicode() {
        let json = json!({
            "name": "时间线测试",
            "content": "思考内容 🤔",
            "description": "描述"
        });
        let params: TimelineCreateParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.name, "时间线测试");
        assert!(params.content.contains("🤔"));
    }

    // ============================================================================
    // TimelineBranchParams Tests
    // ============================================================================

    #[test]
    fn test_timeline_branch_params_defaults() {
        let json = json!({
            "timeline_id": "tl-123",
            "content": "Branch content"
        });
        let params: TimelineBranchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.timeline_id, "tl-123");
        assert_eq!(params.content, "Branch content");
        assert_eq!(params.num_alternatives, 3); // default
        assert!((params.exploration_constant - std::f64::consts::SQRT_2).abs() < 0.001);
    }

    #[test]
    fn test_timeline_branch_params_custom_values() {
        let json = json!({
            "timeline_id": "tl-456",
            "content": "Content",
            "num_alternatives": 4,
            "exploration_constant": 2.0
        });
        let params: TimelineBranchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.num_alternatives, 4);
        assert_eq!(params.exploration_constant, 2.0);
    }

    #[test]
    fn test_default_num_alternatives() {
        assert_eq!(default_num_alternatives(), 3);
    }

    #[test]
    fn test_default_exploration_constant() {
        let expected = std::f64::consts::SQRT_2;
        assert!((default_exploration_constant() - expected).abs() < 0.0001);
    }

    // ============================================================================
    // TimelineCompareParams Tests
    // ============================================================================

    #[test]
    fn test_timeline_compare_params_deserialize() {
        let json = json!({
            "timeline_a": "branch-1",
            "timeline_b": "branch-2"
        });
        let params: TimelineCompareParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.timeline_a, "branch-1");
        assert_eq!(params.timeline_b, "branch-2");
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_timeline_compare_params_with_session() {
        let json = json!({
            "timeline_a": "a",
            "timeline_b": "b",
            "session_id": "sess-789"
        });
        let params: TimelineCompareParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.session_id, Some("sess-789".to_string()));
    }

    // ============================================================================
    // TimelineMergeParams Tests
    // ============================================================================

    #[test]
    fn test_timeline_merge_params_defaults() {
        let json = json!({
            "source_id": "src-1",
            "target_id": "tgt-1"
        });
        let params: TimelineMergeParams = serde_json::from_value(json).unwrap();
        assert!(matches!(params.strategy, MergeStrategy::Synthesize));
    }

    #[test]
    fn test_timeline_merge_params_with_strategy() {
        let json = json!({
            "source_id": "src",
            "target_id": "tgt",
            "strategy": "prefer_source"
        });
        let params: TimelineMergeParams = serde_json::from_value(json).unwrap();
        assert!(matches!(params.strategy, MergeStrategy::PreferSource));
    }

    #[test]
    fn test_merge_strategy_all_variants() {
        let strategies = vec![
            ("synthesize", MergeStrategy::Synthesize),
            ("prefer_source", MergeStrategy::PreferSource),
            ("prefer_target", MergeStrategy::PreferTarget),
        ];
        for (json_val, expected) in strategies {
            let json = json!({
                "source_id": "s",
                "target_id": "t",
                "strategy": json_val
            });
            let params: TimelineMergeParams = serde_json::from_value(json).unwrap();
            assert!(std::mem::discriminant(&params.strategy) == std::mem::discriminant(&expected));
        }
    }

    #[test]
    fn test_merge_strategy_default() {
        let default = MergeStrategy::default();
        assert!(matches!(default, MergeStrategy::Synthesize));
    }

    // ============================================================================
    // Result Types Tests
    // ============================================================================

    #[test]
    fn test_timeline_create_result_serialize() {
        let result = TimelineCreateResult {
            timeline_id: "tl-1".to_string(),
            session_id: "sess-1".to_string(),
            root_branch_id: "branch-1".to_string(),
            content: "Test content".to_string(),
            name: "Test Timeline".to_string(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["timeline_id"], "tl-1");
        assert_eq!(json["session_id"], "sess-1");
        assert_eq!(json["root_branch_id"], "branch-1");
    }

    #[test]
    fn test_branch_with_score_serialize() {
        let branch = BranchWithScore {
            branch_id: "b-1".to_string(),
            content: "Content".to_string(),
            ucb_score: 1.5,
            visit_count: 10,
            confidence: 0.8,
        };
        let json = serde_json::to_value(&branch).unwrap();
        assert_eq!(json["branch_id"], "b-1");
        assert_eq!(json["ucb_score"], 1.5);
        assert_eq!(json["visit_count"], 10);
        assert_eq!(json["confidence"], 0.8);
    }

    #[test]
    fn test_timeline_branch_result_serialize() {
        let result = TimelineBranchResult {
            timeline_id: "tl-1".to_string(),
            branches: vec![
                BranchWithScore {
                    branch_id: "b-1".to_string(),
                    content: "First".to_string(),
                    ucb_score: 1.2,
                    visit_count: 5,
                    confidence: 0.7,
                },
                BranchWithScore {
                    branch_id: "b-2".to_string(),
                    content: "Second".to_string(),
                    ucb_score: 1.5,
                    visit_count: 3,
                    confidence: 0.9,
                },
            ],
            recommended_index: 1,
            parent_visits: 8,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["recommended_index"], 1);
        assert_eq!(json["parent_visits"], 8);
        assert_eq!(json["branches"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_timeline_compare_result_serialize() {
        let result = TimelineCompareResult {
            summary: "Summary".to_string(),
            differences: vec!["Diff 1".to_string(), "Diff 2".to_string()],
            shared_insights: vec!["Shared".to_string()],
            recommendation: "Choose A".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["summary"], "Summary");
        assert_eq!(json["differences"].as_array().unwrap().len(), 2);
        assert_eq!(json["confidence"], 0.85);
    }

    #[test]
    fn test_timeline_merge_result_serialize() {
        let result = TimelineMergeResult {
            merged_id: "m-1".to_string(),
            content: "Merged content".to_string(),
            from_source: vec!["Source insight".to_string()],
            from_target: vec!["Target insight".to_string()],
            synthesized_insights: vec!["New insight".to_string()],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["merged_id"], "m-1");
        assert!(!json["from_source"].as_array().unwrap().is_empty());
        assert!(!json["synthesized_insights"].as_array().unwrap().is_empty());
    }

    // ============================================================================
    // Internal Response Types Tests
    // ============================================================================

    #[test]
    fn test_tree_branch_response_deserialize() {
        let json = json!({
            "branches": [
                {"thought": "First thought", "confidence": 0.8, "rationale": "Reason 1"},
                {"thought": "Second thought", "confidence": 0.7, "rationale": "Reason 2"}
            ]
        });
        let response: TreeBranchResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.branches.len(), 2);
        assert_eq!(response.branches[0].thought, "First thought");
        assert_eq!(response.branches[0].confidence, 0.8);
    }

    #[test]
    fn test_tree_branch_data_without_rationale() {
        let json = json!({
            "branches": [
                {"thought": "Thought", "confidence": 0.9}
            ]
        });
        let response: TreeBranchResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.branches[0].rationale, ""); // default
    }

    #[test]
    fn test_compare_response_deserialize() {
        let json = json!({
            "summary": "Summary",
            "differences": ["diff1"],
            "shared_insights": ["shared1"],
            "recommendation": "Choose A",
            "confidence": 0.9
        });
        let response: CompareResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.summary, "Summary");
        assert_eq!(response.confidence, 0.9);
    }

    #[test]
    fn test_merge_response_deserialize() {
        let json = json!({
            "content": "Merged",
            "from_source": ["s1"],
            "from_target": ["t1"],
            "synthesized_insights": ["new1"]
        });
        let response: MergeResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.content, "Merged");
        assert!(!response.from_source.is_empty());
    }

    // ============================================================================
    // Clone and Debug Tests
    // ============================================================================

    #[test]
    fn test_timeline_create_params_clone() {
        let params = TimelineCreateParams {
            name: "Test".to_string(),
            content: "Content".to_string(),
            description: Some("Desc".to_string()),
            session_id: Some("sess".to_string()),
        };
        let cloned = params.clone();
        assert_eq!(params.name, cloned.name);
        assert_eq!(params.content, cloned.content);
    }

    #[test]
    fn test_merge_strategy_clone() {
        let strategy = MergeStrategy::PreferSource;
        let cloned = strategy.clone();
        assert!(matches!(cloned, MergeStrategy::PreferSource));
    }

    #[test]
    fn test_branch_with_score_clone() {
        let branch = BranchWithScore {
            branch_id: "b".to_string(),
            content: "c".to_string(),
            ucb_score: 1.0,
            visit_count: 1,
            confidence: 0.5,
        };
        let cloned = branch.clone();
        assert_eq!(branch.branch_id, cloned.branch_id);
        assert_eq!(branch.ucb_score, cloned.ucb_score);
    }

    // ============================================================================
    // Round-trip Serialization Tests
    // ============================================================================

    #[test]
    fn test_timeline_create_params_round_trip() {
        let original = TimelineCreateParams {
            name: "Test Timeline".to_string(),
            content: "Test content".to_string(),
            description: Some("Description".to_string()),
            session_id: Some("sess-123".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TimelineCreateParams = serde_json::from_str(&json).unwrap();
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.content, deserialized.content);
        assert_eq!(original.description, deserialized.description);
        assert_eq!(original.session_id, deserialized.session_id);
    }

    #[test]
    fn test_timeline_compare_result_round_trip() {
        let original = TimelineCompareResult {
            summary: "Test summary".to_string(),
            differences: vec!["diff1".to_string(), "diff2".to_string()],
            shared_insights: vec!["insight1".to_string()],
            recommendation: "Choose path A".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TimelineCompareResult = serde_json::from_str(&json).unwrap();
        assert_eq!(original.summary, deserialized.summary);
        assert_eq!(original.differences, deserialized.differences);
        assert_eq!(original.confidence, deserialized.confidence);
    }
}
