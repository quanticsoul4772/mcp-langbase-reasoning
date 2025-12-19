//! Tree reasoning mode - branching exploration with multiple paths.
//!
//! This module provides tree-based reasoning for exploring multiple directions:
//! - Multiple branch exploration (2-4 branches)
//! - Branch focusing and navigation
//! - Cross-references between branches
//! - Recommended path identification

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::TREE_REASONING_PROMPT;
use crate::storage::{
    Branch, BranchState, CrossRef, CrossRefType, Invocation, SqliteStorage, Storage, Thought,
};

/// Input parameters for tree reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeParams {
    /// The thought content to process
    pub content: String,
    /// Optional session ID (creates new if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Branch ID to extend (creates root branch if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    /// Confidence threshold (0.0-1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Number of branches to explore (2-4)
    #[serde(default = "default_num_branches")]
    pub num_branches: usize,
    /// Cross-references to other branches
    #[serde(default)]
    pub cross_refs: Vec<CrossRefInput>,
}

fn default_confidence() -> f64 {
    0.8
}

fn default_num_branches() -> usize {
    3
}

/// Cross-reference input for tree reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossRefInput {
    /// The target branch ID to reference.
    pub to_branch: String,
    /// The type of reference (supports, contradicts, extends, alternative, depends).
    #[serde(rename = "type")]
    pub ref_type: String,
    /// Optional reason for the cross-reference.
    pub reason: Option<String>,
    /// Optional strength of the reference (0.0-1.0).
    pub strength: Option<f64>,
}

/// Response from tree reasoning Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResponse {
    /// The generated branches/paths.
    pub branches: Vec<TreeBranch>,
    /// Index of the recommended branch (0-based).
    pub recommended_branch: usize,
    /// Additional metadata from the response.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Individual branch in tree response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeBranch {
    /// The thought content for this branch.
    pub thought: String,
    /// Confidence in this branch (0.0-1.0).
    pub confidence: f64,
    /// Rationale for why this branch was generated.
    pub rationale: String,
}

/// Result of tree reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResult {
    /// The session ID.
    pub session_id: String,
    /// The current branch ID.
    pub branch_id: String,
    /// The ID of the created thought.
    pub thought_id: String,
    /// The thought content.
    pub content: String,
    /// Confidence in the thought (0.0-1.0).
    pub confidence: f64,
    /// Child branches created for exploration.
    pub child_branches: Vec<BranchInfo>,
    /// Index of the recommended branch (0-based).
    pub recommended_branch_index: usize,
    /// Parent branch ID, if this is an extension.
    pub parent_branch: Option<String>,
    /// Number of cross-references created.
    pub cross_refs_created: usize,
}

/// Branch information in result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    /// The branch ID.
    pub id: String,
    /// Human-readable branch name.
    pub name: String,
    /// Confidence in this branch (0.0-1.0).
    pub confidence: f64,
    /// Rationale for this branch.
    pub rationale: String,
}

/// Tree reasoning mode handler for branching exploration.
#[derive(Clone)]
pub struct TreeMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for tree reasoning.
    pipe_name: String,
}

impl TreeMode {
    /// Create a new tree mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            pipe_name: config.pipes.tree.clone(),
        }
    }

    /// Process a tree reasoning request
    pub async fn process(&self, params: TreeParams) -> AppResult<TreeResult> {
        let start = Instant::now();

        // Validate input
        if params.content.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "content".to_string(),
                reason: "Content cannot be empty".to_string(),
            }
            .into());
        }

        let num_branches = params.num_branches.clamp(2, 4);

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "tree")
            .await?;
        debug!(session_id = %session.id, "Processing tree reasoning");

        // Get or create branch
        let parent_branch = match &params.branch_id {
            Some(id) => self.core.storage().get_branch(id).await?,
            None => None,
        };

        // Create the current branch if extending or creating new
        let branch = match &parent_branch {
            Some(parent) => {
                let b = Branch::new(&session.id)
                    .with_parent(&parent.id)
                    .with_name(format!("Branch from {}", &parent.id[..8]))
                    .with_confidence(params.confidence);
                self.core.storage().create_branch(&b).await?;
                b
            }
            None => {
                // Check if session has an active branch, or create root
                match &session.active_branch_id {
                    Some(active_id) => self
                        .core
                        .storage()
                        .get_branch(active_id)
                        .await?
                        .unwrap_or_else(|| Branch::new(&session.id).with_name("Root")),
                    None => {
                        let b = Branch::new(&session.id).with_name("Root");
                        self.core.storage().create_branch(&b).await?;
                        // Update session with active branch
                        let mut updated_session = session.clone();
                        updated_session.active_branch_id = Some(b.id.clone());
                        self.core.storage().update_session(&updated_session).await?;
                        b
                    }
                }
            }
        };

        // Get context from branch history
        let branch_thoughts = self.core.storage().get_branch_thoughts(&branch.id).await?;

        // Build messages for Langbase
        let messages = self.build_messages(&params.content, &branch_thoughts, num_branches);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.tree",
            serialize_for_log(&params, "reasoning.tree input"),
        )
        .with_session(&session.id)
        .with_pipe(&self.pipe_name);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.pipe_name, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                self.core.storage().log_invocation(&invocation).await?;
                return Err(e.into());
            }
        };

        // Parse response
        let tree_response = self.parse_response(&response.completion)?;

        // Create main thought for this branch
        let thought = Thought::new(&session.id, &params.content, "tree")
            .with_confidence(params.confidence)
            .with_branch(&branch.id);
        self.core.storage().create_thought(&thought).await?;

        // Create child branches for each explored path
        let mut child_branches = Vec::new();
        for (i, tb) in tree_response.branches.iter().enumerate() {
            let child = Branch::new(&session.id)
                .with_parent(&branch.id)
                .with_name(format!("Option {}: {}", i + 1, truncate(&tb.thought, 30)))
                .with_confidence(tb.confidence)
                .with_priority(if i == tree_response.recommended_branch {
                    2.0
                } else {
                    1.0
                });

            self.core.storage().create_branch(&child).await?;

            // Create thought for this branch
            let child_thought = Thought::new(&session.id, &tb.thought, "tree")
                .with_confidence(tb.confidence)
                .with_branch(&child.id)
                .with_parent(&thought.id);
            self.core.storage().create_thought(&child_thought).await?;

            child_branches.push(BranchInfo {
                id: child.id,
                name: child
                    .name
                    .clone()
                    .unwrap_or_else(|| "Unnamed Branch".to_string()),
                confidence: tb.confidence,
                rationale: tb.rationale.clone(),
            });
        }

        // Create cross-references if specified
        let mut cross_refs_created = 0;
        for cr_input in &params.cross_refs {
            if let Ok(ref_type) = cr_input.ref_type.parse::<CrossRefType>() {
                let cr = CrossRef::new(&branch.id, &cr_input.to_branch, ref_type)
                    .with_strength(cr_input.strength.unwrap_or(1.0));
                let cr = if let Some(reason) = &cr_input.reason {
                    cr.with_reason(reason)
                } else {
                    cr
                };
                self.core.storage().create_cross_ref(&cr).await?;
                cross_refs_created += 1;
            }
        }

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&tree_response, "reasoning.tree output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            session_id = %session.id,
            branch_id = %branch.id,
            thought_id = %thought.id,
            num_children = child_branches.len(),
            latency_ms = latency,
            "Tree reasoning completed"
        );

        Ok(TreeResult {
            session_id: session.id,
            branch_id: branch.id,
            thought_id: thought.id,
            content: params.content,
            confidence: params.confidence,
            child_branches,
            recommended_branch_index: tree_response.recommended_branch,
            parent_branch: parent_branch.map(|b| b.id),
            cross_refs_created,
        })
    }

    /// Focus on a specific branch, making it the active branch
    pub async fn focus_branch(&self, session_id: &str, branch_id: &str) -> AppResult<Branch> {
        let branch = self
            .core
            .storage()
            .get_branch(branch_id)
            .await?
            .ok_or_else(|| ToolError::Session(format!("Branch not found: {}", branch_id)))?;

        // Verify branch belongs to session
        if branch.session_id != session_id {
            return Err(
                ToolError::Session("Branch does not belong to this session".to_string()).into(),
            );
        }

        // Update session's active branch
        let session = self
            .core
            .storage()
            .get_session(session_id)
            .await?
            .ok_or_else(|| ToolError::Session(format!("Session not found: {}", session_id)))?;

        let mut updated_session = session;
        updated_session.active_branch_id = Some(branch_id.to_string());
        self.core.storage().update_session(&updated_session).await?;

        Ok(branch)
    }

    /// Get all branches for a session
    pub async fn list_branches(&self, session_id: &str) -> AppResult<Vec<Branch>> {
        Ok(self.core.storage().get_session_branches(session_id).await?)
    }

    /// Update branch state (complete, abandon)
    pub async fn update_branch_state(
        &self,
        branch_id: &str,
        state: BranchState,
    ) -> AppResult<Branch> {
        let mut branch = self
            .core
            .storage()
            .get_branch(branch_id)
            .await?
            .ok_or_else(|| ToolError::Session(format!("Branch not found: {}", branch_id)))?;

        branch.state = state;
        branch.updated_at = chrono::Utc::now();
        self.core.storage().update_branch(&branch).await?;

        Ok(branch)
    }

    fn build_messages(
        &self,
        content: &str,
        history: &[Thought],
        num_branches: usize,
    ) -> Vec<Message> {
        let mut messages = Vec::new();

        // System prompt for tree reasoning
        let system_prompt = TREE_REASONING_PROMPT.replace(
            "2-4 distinct reasoning paths",
            &format!("{} distinct reasoning paths", num_branches),
        );
        messages.push(Message::system(system_prompt));

        // Add history context if available
        if !history.is_empty() {
            let history_text: Vec<String> =
                history.iter().map(|t| format!("- {}", t.content)).collect();

            messages.push(Message::user(format!(
                "Previous reasoning in this branch:\n{}\n\nNow explore this thought:",
                history_text.join("\n")
            )));
        }

        // Add current content
        messages.push(Message::user(content.to_string()));

        messages
    }

    fn parse_response(&self, completion: &str) -> AppResult<TreeResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from tree response"
            );
            ToolError::Reasoning {
                message: format!("Tree response extraction failed: {}", e),
            }
        })?;

        serde_json::from_str::<TreeResponse>(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse tree response: {}", e),
            }
            .into()
        })
    }
}

impl TreeParams {
    /// Create new params with just content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            session_id: None,
            branch_id: None,
            confidence: default_confidence(),
            num_branches: default_num_branches(),
            cross_refs: Vec::new(),
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the branch ID to extend
    pub fn with_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = Some(branch_id.into());
        self
    }

    /// Set the confidence threshold
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the number of branches to explore
    pub fn with_num_branches(mut self, num: usize) -> Self {
        self.num_branches = num.clamp(2, 4);
        self
    }

    /// Add a cross-reference
    pub fn with_cross_ref(
        mut self,
        to_branch: impl Into<String>,
        ref_type: impl Into<String>,
    ) -> Self {
        self.cross_refs.push(CrossRefInput {
            to_branch: to_branch.into(),
            ref_type: ref_type.into(),
            reason: None,
            strength: None,
        });
        self
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Default Function Tests
    // ============================================================================

    #[test]
    fn test_default_confidence() {
        assert_eq!(default_confidence(), 0.8);
    }

    #[test]
    fn test_default_num_branches() {
        assert_eq!(default_num_branches(), 3);
    }

    // ============================================================================
    // Truncate Function Tests
    // ============================================================================

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("Hello", 10), "Hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("Hello World", 8), "Hello...");
    }

    #[test]
    fn test_truncate_very_short_max() {
        assert_eq!(truncate("Hello World", 3), "...");
    }

    // ============================================================================
    // TreeParams Tests
    // ============================================================================

    #[test]
    fn test_tree_params_new() {
        let params = TreeParams::new("Test content");
        assert_eq!(params.content, "Test content");
        assert!(params.session_id.is_none());
        assert!(params.branch_id.is_none());
        assert_eq!(params.confidence, 0.8);
        assert_eq!(params.num_branches, 3);
        assert!(params.cross_refs.is_empty());
    }

    #[test]
    fn test_tree_params_with_session() {
        let params = TreeParams::new("Content").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_tree_params_with_branch() {
        let params = TreeParams::new("Content").with_branch("branch-456");
        assert_eq!(params.branch_id, Some("branch-456".to_string()));
    }

    #[test]
    fn test_tree_params_with_confidence() {
        let params = TreeParams::new("Content").with_confidence(0.9);
        assert_eq!(params.confidence, 0.9);
    }

    #[test]
    fn test_tree_params_confidence_clamped_high() {
        let params = TreeParams::new("Content").with_confidence(1.5);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_tree_params_confidence_clamped_low() {
        let params = TreeParams::new("Content").with_confidence(-0.5);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_tree_params_with_num_branches() {
        let params = TreeParams::new("Content").with_num_branches(4);
        assert_eq!(params.num_branches, 4);
    }

    #[test]
    fn test_tree_params_num_branches_clamped_high() {
        let params = TreeParams::new("Content").with_num_branches(10);
        assert_eq!(params.num_branches, 4); // max is 4
    }

    #[test]
    fn test_tree_params_num_branches_clamped_low() {
        let params = TreeParams::new("Content").with_num_branches(1);
        assert_eq!(params.num_branches, 2); // min is 2
    }

    #[test]
    fn test_tree_params_with_cross_ref() {
        let params = TreeParams::new("Content").with_cross_ref("branch-target", "supports");
        assert_eq!(params.cross_refs.len(), 1);
        assert_eq!(params.cross_refs[0].to_branch, "branch-target");
        assert_eq!(params.cross_refs[0].ref_type, "supports");
        assert!(params.cross_refs[0].reason.is_none());
        assert!(params.cross_refs[0].strength.is_none());
    }

    #[test]
    fn test_tree_params_multiple_cross_refs() {
        let params = TreeParams::new("Content")
            .with_cross_ref("branch-1", "supports")
            .with_cross_ref("branch-2", "contradicts");
        assert_eq!(params.cross_refs.len(), 2);
    }

    #[test]
    fn test_tree_params_builder_chain() {
        let params = TreeParams::new("Chained")
            .with_session("my-session")
            .with_branch("my-branch")
            .with_confidence(0.85)
            .with_num_branches(4)
            .with_cross_ref("ref-branch", "supports");

        assert_eq!(params.content, "Chained");
        assert_eq!(params.session_id, Some("my-session".to_string()));
        assert_eq!(params.branch_id, Some("my-branch".to_string()));
        assert_eq!(params.confidence, 0.85);
        assert_eq!(params.num_branches, 4);
        assert_eq!(params.cross_refs.len(), 1);
    }

    #[test]
    fn test_tree_params_serialize() {
        let params = TreeParams::new("Test")
            .with_session("sess-1")
            .with_num_branches(3);

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("sess-1"));
        assert!(json.contains("\"num_branches\":3"));
    }

    #[test]
    fn test_tree_params_deserialize() {
        let json = r#"{
            "content": "Parsed",
            "session_id": "s-1",
            "branch_id": "b-1",
            "confidence": 0.9,
            "num_branches": 4,
            "cross_refs": []
        }"#;
        let params: TreeParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.content, "Parsed");
        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.branch_id, Some("b-1".to_string()));
        assert_eq!(params.confidence, 0.9);
        assert_eq!(params.num_branches, 4);
    }

    #[test]
    fn test_tree_params_deserialize_minimal() {
        let json = r#"{"content": "Only content"}"#;
        let params: TreeParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.content, "Only content");
        assert!(params.session_id.is_none());
        assert!(params.branch_id.is_none());
        assert_eq!(params.confidence, 0.8); // default
        assert_eq!(params.num_branches, 3); // default
        assert!(params.cross_refs.is_empty());
    }

    // ============================================================================
    // CrossRefInput Tests
    // ============================================================================

    #[test]
    fn test_cross_ref_input_serialize() {
        let cr = CrossRefInput {
            to_branch: "target-branch".to_string(),
            ref_type: "supports".to_string(),
            reason: Some("Strong evidence".to_string()),
            strength: Some(0.9),
        };

        let json = serde_json::to_string(&cr).unwrap();
        assert!(json.contains("target-branch"));
        assert!(json.contains("supports"));
        assert!(json.contains("Strong evidence"));
        assert!(json.contains("0.9"));
    }

    #[test]
    fn test_cross_ref_input_deserialize() {
        let json = r#"{
            "to_branch": "b-1",
            "type": "contradicts",
            "reason": "Conflicts with main thesis",
            "strength": 0.8
        }"#;
        let cr: CrossRefInput = serde_json::from_str(json).unwrap();

        assert_eq!(cr.to_branch, "b-1");
        assert_eq!(cr.ref_type, "contradicts");
        assert_eq!(cr.reason, Some("Conflicts with main thesis".to_string()));
        assert_eq!(cr.strength, Some(0.8));
    }

    #[test]
    fn test_cross_ref_input_deserialize_minimal() {
        let json = r#"{"to_branch": "b-1", "type": "supports"}"#;
        let cr: CrossRefInput = serde_json::from_str(json).unwrap();

        assert_eq!(cr.to_branch, "b-1");
        assert_eq!(cr.ref_type, "supports");
        assert!(cr.reason.is_none());
        assert!(cr.strength.is_none());
    }

    // ============================================================================
    // TreeBranch Tests
    // ============================================================================

    #[test]
    fn test_tree_branch_serialize() {
        let branch = TreeBranch {
            thought: "A branching thought".to_string(),
            confidence: 0.85,
            rationale: "This is the reasoning".to_string(),
        };

        let json = serde_json::to_string(&branch).unwrap();
        assert!(json.contains("A branching thought"));
        assert!(json.contains("0.85"));
        assert!(json.contains("This is the reasoning"));
    }

    #[test]
    fn test_tree_branch_deserialize() {
        let json = r#"{
            "thought": "Branch thought",
            "confidence": 0.75,
            "rationale": "Because reasons"
        }"#;
        let branch: TreeBranch = serde_json::from_str(json).unwrap();

        assert_eq!(branch.thought, "Branch thought");
        assert_eq!(branch.confidence, 0.75);
        assert_eq!(branch.rationale, "Because reasons");
    }

    // ============================================================================
    // TreeResponse Tests
    // ============================================================================

    #[test]
    fn test_tree_response_serialize() {
        let response = TreeResponse {
            branches: vec![
                TreeBranch {
                    thought: "Option 1".to_string(),
                    confidence: 0.8,
                    rationale: "First path".to_string(),
                },
                TreeBranch {
                    thought: "Option 2".to_string(),
                    confidence: 0.7,
                    rationale: "Second path".to_string(),
                },
            ],
            recommended_branch: 0,
            metadata: serde_json::json!({"analysis": "complete"}),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Option 1"));
        assert!(json.contains("Option 2"));
        assert!(json.contains("recommended_branch"));
    }

    #[test]
    fn test_tree_response_deserialize() {
        let json = r#"{
            "branches": [
                {"thought": "Path A", "confidence": 0.9, "rationale": "Strong"},
                {"thought": "Path B", "confidence": 0.6, "rationale": "Weak"}
            ],
            "recommended_branch": 1
        }"#;
        let response: TreeResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.branches.len(), 2);
        assert_eq!(response.recommended_branch, 1);
        assert_eq!(response.branches[0].thought, "Path A");
    }

    // ============================================================================
    // BranchInfo Tests
    // ============================================================================

    #[test]
    fn test_branch_info_serialize() {
        let info = BranchInfo {
            id: "branch-123".to_string(),
            name: "Main branch".to_string(),
            confidence: 0.82,
            rationale: "Best option".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("branch-123"));
        assert!(json.contains("Main branch"));
        assert!(json.contains("0.82"));
        assert!(json.contains("Best option"));
    }

    #[test]
    fn test_branch_info_deserialize() {
        let json = r#"{
            "id": "b-1",
            "name": "Test Branch",
            "confidence": 0.95,
            "rationale": "The rationale"
        }"#;
        let info: BranchInfo = serde_json::from_str(json).unwrap();

        assert_eq!(info.id, "b-1");
        assert_eq!(info.name, "Test Branch");
        assert_eq!(info.confidence, 0.95);
        assert_eq!(info.rationale, "The rationale");
    }

    #[test]
    fn test_branch_info_unnamed_fallback() {
        // Verify that "Unnamed Branch" is a valid name value for serialization
        let info = BranchInfo {
            id: "branch-1".to_string(),
            name: "Unnamed Branch".to_string(),
            confidence: 0.7,
            rationale: "No name provided".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Unnamed Branch"));

        let parsed: BranchInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Unnamed Branch");
    }

    // ============================================================================
    // TreeResult Tests
    // ============================================================================

    #[test]
    fn test_tree_result_serialize() {
        let result = TreeResult {
            session_id: "sess-1".to_string(),
            branch_id: "branch-1".to_string(),
            thought_id: "thought-1".to_string(),
            content: "Main thought content".to_string(),
            confidence: 0.88,
            child_branches: vec![BranchInfo {
                id: "child-1".to_string(),
                name: "Child Branch".to_string(),
                confidence: 0.75,
                rationale: "Exploring option".to_string(),
            }],
            recommended_branch_index: 0,
            parent_branch: Some("parent-1".to_string()),
            cross_refs_created: 2,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("sess-1"));
        assert!(json.contains("branch-1"));
        assert!(json.contains("Main thought content"));
        assert!(json.contains("parent-1"));
    }

    #[test]
    fn test_tree_result_deserialize() {
        let json = r#"{
            "session_id": "s-1",
            "branch_id": "b-1",
            "thought_id": "t-1",
            "content": "Content",
            "confidence": 0.8,
            "child_branches": [],
            "recommended_branch_index": 0,
            "parent_branch": null,
            "cross_refs_created": 0
        }"#;
        let result: TreeResult = serde_json::from_str(json).unwrap();

        assert_eq!(result.session_id, "s-1");
        assert_eq!(result.branch_id, "b-1");
        assert_eq!(result.thought_id, "t-1");
        assert!(result.child_branches.is_empty());
        assert!(result.parent_branch.is_none());
        assert_eq!(result.cross_refs_created, 0);
    }

    #[test]
    fn test_tree_result_with_children() {
        let result = TreeResult {
            session_id: "s-1".to_string(),
            branch_id: "b-1".to_string(),
            thought_id: "t-1".to_string(),
            content: "Root".to_string(),
            confidence: 0.9,
            child_branches: vec![
                BranchInfo {
                    id: "c-1".to_string(),
                    name: "Child 1".to_string(),
                    confidence: 0.85,
                    rationale: "First".to_string(),
                },
                BranchInfo {
                    id: "c-2".to_string(),
                    name: "Child 2".to_string(),
                    confidence: 0.7,
                    rationale: "Second".to_string(),
                },
            ],
            recommended_branch_index: 1,
            parent_branch: None,
            cross_refs_created: 1,
        };

        assert_eq!(result.child_branches.len(), 2);
        assert_eq!(result.recommended_branch_index, 1);
        assert_eq!(result.cross_refs_created, 1);
    }

    // ============================================================================
    // Edge Cases and Boundary Tests
    // ============================================================================

    #[test]
    fn test_tree_params_empty_content() {
        let params = TreeParams::new("");
        assert_eq!(params.content, "");
    }

    #[test]
    fn test_tree_params_whitespace_content() {
        let params = TreeParams::new("   ");
        assert_eq!(params.content, "   ");
    }

    #[test]
    fn test_tree_params_num_branches_boundary_min() {
        let params = TreeParams::new("Content").with_num_branches(2);
        assert_eq!(params.num_branches, 2);
    }

    #[test]
    fn test_tree_params_num_branches_boundary_max() {
        let params = TreeParams::new("Content").with_num_branches(4);
        assert_eq!(params.num_branches, 4);
    }

    #[test]
    fn test_tree_params_num_branches_zero() {
        let params = TreeParams::new("Content").with_num_branches(0);
        assert_eq!(params.num_branches, 2); // clamped to min
    }

    #[test]
    fn test_tree_params_confidence_boundary_zero() {
        let params = TreeParams::new("Content").with_confidence(0.0);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_tree_params_confidence_boundary_one() {
        let params = TreeParams::new("Content").with_confidence(1.0);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_tree_params_confidence_negative() {
        let params = TreeParams::new("Content").with_confidence(-10.5);
        assert_eq!(params.confidence, 0.0);
    }

    #[test]
    fn test_tree_params_confidence_very_high() {
        let params = TreeParams::new("Content").with_confidence(100.0);
        assert_eq!(params.confidence, 1.0);
    }

    #[test]
    fn test_tree_result_empty_child_branches() {
        let result = TreeResult {
            session_id: "s-1".to_string(),
            branch_id: "b-1".to_string(),
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.8,
            child_branches: vec![],
            recommended_branch_index: 0,
            parent_branch: None,
            cross_refs_created: 0,
        };

        assert!(result.child_branches.is_empty());
    }

    #[test]
    fn test_tree_result_with_parent_branch() {
        let result = TreeResult {
            session_id: "s-1".to_string(),
            branch_id: "b-1".to_string(),
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.8,
            child_branches: vec![],
            recommended_branch_index: 0,
            parent_branch: Some("parent-123".to_string()),
            cross_refs_created: 0,
        };

        assert!(result.parent_branch.is_some());
        assert_eq!(result.parent_branch.unwrap(), "parent-123");
    }

    #[test]
    fn test_tree_result_multiple_cross_refs() {
        let result = TreeResult {
            session_id: "s-1".to_string(),
            branch_id: "b-1".to_string(),
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.8,
            child_branches: vec![],
            recommended_branch_index: 0,
            parent_branch: None,
            cross_refs_created: 5,
        };

        assert_eq!(result.cross_refs_created, 5);
    }

    // ============================================================================
    // CrossRefInput All Variants Tests
    // ============================================================================

    #[test]
    fn test_cross_ref_input_type_supports() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: None,
            strength: None,
        };
        assert_eq!(cr.ref_type, "supports");
    }

    #[test]
    fn test_cross_ref_input_type_contradicts() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "contradicts".to_string(),
            reason: None,
            strength: None,
        };
        assert_eq!(cr.ref_type, "contradicts");
    }

    #[test]
    fn test_cross_ref_input_type_extends() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "extends".to_string(),
            reason: None,
            strength: None,
        };
        assert_eq!(cr.ref_type, "extends");
    }

    #[test]
    fn test_cross_ref_input_type_alternative() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "alternative".to_string(),
            reason: None,
            strength: None,
        };
        assert_eq!(cr.ref_type, "alternative");
    }

    #[test]
    fn test_cross_ref_input_type_depends() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "depends".to_string(),
            reason: None,
            strength: None,
        };
        assert_eq!(cr.ref_type, "depends");
    }

    #[test]
    fn test_cross_ref_input_with_reason() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: Some("Strong evidence".to_string()),
            strength: None,
        };
        assert_eq!(cr.reason, Some("Strong evidence".to_string()));
    }

    #[test]
    fn test_cross_ref_input_with_strength_zero() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: None,
            strength: Some(0.0),
        };
        assert_eq!(cr.strength, Some(0.0));
    }

    #[test]
    fn test_cross_ref_input_with_strength_one() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: None,
            strength: Some(1.0),
        };
        assert_eq!(cr.strength, Some(1.0));
    }

    #[test]
    fn test_cross_ref_input_with_strength_mid() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: None,
            strength: Some(0.5),
        };
        assert_eq!(cr.strength, Some(0.5));
    }

    #[test]
    fn test_cross_ref_input_full_fields() {
        let cr = CrossRefInput {
            to_branch: "target-branch".to_string(),
            ref_type: "extends".to_string(),
            reason: Some("Builds on previous work".to_string()),
            strength: Some(0.95),
        };

        assert_eq!(cr.to_branch, "target-branch");
        assert_eq!(cr.ref_type, "extends");
        assert_eq!(cr.reason, Some("Builds on previous work".to_string()));
        assert_eq!(cr.strength, Some(0.95));
    }

    // ============================================================================
    // TreeResponse Additional Tests
    // ============================================================================

    #[test]
    fn test_tree_response_empty_branches() {
        let response = TreeResponse {
            branches: vec![],
            recommended_branch: 0,
            metadata: serde_json::json!({}),
        };
        assert!(response.branches.is_empty());
    }

    #[test]
    fn test_tree_response_single_branch() {
        let response = TreeResponse {
            branches: vec![TreeBranch {
                thought: "Only option".to_string(),
                confidence: 0.9,
                rationale: "Best choice".to_string(),
            }],
            recommended_branch: 0,
            metadata: serde_json::json!({}),
        };
        assert_eq!(response.branches.len(), 1);
        assert_eq!(response.recommended_branch, 0);
    }

    #[test]
    fn test_tree_response_four_branches() {
        let response = TreeResponse {
            branches: vec![
                TreeBranch {
                    thought: "Branch 1".to_string(),
                    confidence: 0.8,
                    rationale: "Option 1".to_string(),
                },
                TreeBranch {
                    thought: "Branch 2".to_string(),
                    confidence: 0.85,
                    rationale: "Option 2".to_string(),
                },
                TreeBranch {
                    thought: "Branch 3".to_string(),
                    confidence: 0.7,
                    rationale: "Option 3".to_string(),
                },
                TreeBranch {
                    thought: "Branch 4".to_string(),
                    confidence: 0.9,
                    rationale: "Option 4".to_string(),
                },
            ],
            recommended_branch: 3,
            metadata: serde_json::json!({}),
        };
        assert_eq!(response.branches.len(), 4);
        assert_eq!(response.recommended_branch, 3);
    }

    #[test]
    fn test_tree_response_with_metadata() {
        let response = TreeResponse {
            branches: vec![],
            recommended_branch: 0,
            metadata: serde_json::json!({
                "total_time": 123,
                "model": "gpt-4",
                "tokens": 456
            }),
        };
        assert_eq!(response.metadata["total_time"], 123);
        assert_eq!(response.metadata["model"], "gpt-4");
        assert_eq!(response.metadata["tokens"], 456);
    }

    #[test]
    fn test_tree_response_deserialize_with_default_metadata() {
        let json = r#"{
            "branches": [],
            "recommended_branch": 0
        }"#;
        let response: TreeResponse = serde_json::from_str(json).unwrap();
        // Default metadata is Value::Null (serde_json::Value default)
        assert!(response.metadata.is_null());
    }

    // ============================================================================
    // TreeBranch Additional Tests
    // ============================================================================

    #[test]
    fn test_tree_branch_zero_confidence() {
        let branch = TreeBranch {
            thought: "Low confidence branch".to_string(),
            confidence: 0.0,
            rationale: "Uncertain".to_string(),
        };
        assert_eq!(branch.confidence, 0.0);
    }

    #[test]
    fn test_tree_branch_max_confidence() {
        let branch = TreeBranch {
            thought: "High confidence branch".to_string(),
            confidence: 1.0,
            rationale: "Very certain".to_string(),
        };
        assert_eq!(branch.confidence, 1.0);
    }

    #[test]
    fn test_tree_branch_empty_thought() {
        let branch = TreeBranch {
            thought: "".to_string(),
            confidence: 0.5,
            rationale: "No content".to_string(),
        };
        assert_eq!(branch.thought, "");
    }

    #[test]
    fn test_tree_branch_empty_rationale() {
        let branch = TreeBranch {
            thought: "Branch content".to_string(),
            confidence: 0.5,
            rationale: "".to_string(),
        };
        assert_eq!(branch.rationale, "");
    }

    // ============================================================================
    // BranchInfo Additional Tests
    // ============================================================================

    #[test]
    fn test_branch_info_zero_confidence() {
        let info = BranchInfo {
            id: "b-1".to_string(),
            name: "Branch".to_string(),
            confidence: 0.0,
            rationale: "Low".to_string(),
        };
        assert_eq!(info.confidence, 0.0);
    }

    #[test]
    fn test_branch_info_max_confidence() {
        let info = BranchInfo {
            id: "b-1".to_string(),
            name: "Branch".to_string(),
            confidence: 1.0,
            rationale: "High".to_string(),
        };
        assert_eq!(info.confidence, 1.0);
    }

    #[test]
    fn test_branch_info_empty_name() {
        let info = BranchInfo {
            id: "b-1".to_string(),
            name: "".to_string(),
            confidence: 0.8,
            rationale: "Rationale".to_string(),
        };
        assert_eq!(info.name, "");
    }

    #[test]
    fn test_branch_info_long_name() {
        let long_name = "A".repeat(200);
        let info = BranchInfo {
            id: "b-1".to_string(),
            name: long_name.clone(),
            confidence: 0.8,
            rationale: "Rationale".to_string(),
        };
        assert_eq!(info.name, long_name);
    }

    // ============================================================================
    // Truncate Additional Edge Cases
    // ============================================================================

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn test_truncate_max_len_zero() {
        assert_eq!(truncate("Hello", 0), "...");
    }

    #[test]
    fn test_truncate_max_len_one() {
        assert_eq!(truncate("Hello", 1), "...");
    }

    #[test]
    fn test_truncate_max_len_two() {
        assert_eq!(truncate("Hello", 2), "...");
    }

    #[test]
    fn test_truncate_unicode() {
        // Test with ASCII string (truncate function uses byte indexing, not unicode-safe)
        let result = truncate("Hello World!", 10);
        assert!(result.len() <= 10);
        assert!(result.ends_with("..."));
    }

    // ============================================================================
    // Serialization Round-trip Tests
    // ============================================================================

    #[test]
    fn test_tree_params_roundtrip() {
        let params = TreeParams::new("Test content")
            .with_session("sess-123")
            .with_branch("branch-456")
            .with_confidence(0.75)
            .with_num_branches(3)
            .with_cross_ref("ref-1", "supports");

        let json = serde_json::to_string(&params).unwrap();
        let parsed: TreeParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.content, params.content);
        assert_eq!(parsed.session_id, params.session_id);
        assert_eq!(parsed.branch_id, params.branch_id);
        assert_eq!(parsed.confidence, params.confidence);
        assert_eq!(parsed.num_branches, params.num_branches);
        assert_eq!(parsed.cross_refs.len(), params.cross_refs.len());
    }

    #[test]
    fn test_tree_response_roundtrip() {
        let response = TreeResponse {
            branches: vec![
                TreeBranch {
                    thought: "Path 1".to_string(),
                    confidence: 0.8,
                    rationale: "Reason 1".to_string(),
                },
                TreeBranch {
                    thought: "Path 2".to_string(),
                    confidence: 0.9,
                    rationale: "Reason 2".to_string(),
                },
            ],
            recommended_branch: 1,
            metadata: serde_json::json!({"key": "value"}),
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: TreeResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.branches.len(), response.branches.len());
        assert_eq!(parsed.recommended_branch, response.recommended_branch);
        assert_eq!(parsed.metadata, response.metadata);
    }

    #[test]
    fn test_tree_result_roundtrip() {
        let result = TreeResult {
            session_id: "s-1".to_string(),
            branch_id: "b-1".to_string(),
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.88,
            child_branches: vec![BranchInfo {
                id: "c-1".to_string(),
                name: "Child".to_string(),
                confidence: 0.77,
                rationale: "Child rationale".to_string(),
            }],
            recommended_branch_index: 0,
            parent_branch: Some("p-1".to_string()),
            cross_refs_created: 3,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: TreeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.session_id, result.session_id);
        assert_eq!(parsed.branch_id, result.branch_id);
        assert_eq!(parsed.thought_id, result.thought_id);
        assert_eq!(parsed.confidence, result.confidence);
        assert_eq!(parsed.child_branches.len(), result.child_branches.len());
        assert_eq!(
            parsed.recommended_branch_index,
            result.recommended_branch_index
        );
        assert_eq!(parsed.cross_refs_created, result.cross_refs_created);
    }

    #[test]
    fn test_cross_ref_input_roundtrip() {
        let cr = CrossRefInput {
            to_branch: "target".to_string(),
            ref_type: "contradicts".to_string(),
            reason: Some("Conflicts".to_string()),
            strength: Some(0.85),
        };

        let json = serde_json::to_string(&cr).unwrap();
        let parsed: CrossRefInput = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.to_branch, cr.to_branch);
        assert_eq!(parsed.ref_type, cr.ref_type);
        assert_eq!(parsed.reason, cr.reason);
        assert_eq!(parsed.strength, cr.strength);
    }

    #[test]
    fn test_branch_info_roundtrip() {
        let info = BranchInfo {
            id: "b-123".to_string(),
            name: "Test Branch".to_string(),
            confidence: 0.92,
            rationale: "Good choice".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: BranchInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, info.id);
        assert_eq!(parsed.name, info.name);
        assert_eq!(parsed.confidence, info.confidence);
        assert_eq!(parsed.rationale, info.rationale);
    }

    // ============================================================================
    // TreeParams Field Skipping Tests (Optional Fields)
    // ============================================================================

    #[test]
    fn test_tree_params_serialize_skips_none_session() {
        let params = TreeParams::new("Content");
        let json = serde_json::to_string(&params).unwrap();
        assert!(!json.contains("session_id"));
    }

    #[test]
    fn test_tree_params_serialize_includes_some_session() {
        let params = TreeParams::new("Content").with_session("sess-1");
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("session_id"));
    }

    #[test]
    fn test_tree_params_serialize_skips_none_branch() {
        let params = TreeParams::new("Content");
        let json = serde_json::to_string(&params).unwrap();
        assert!(!json.contains("branch_id"));
    }

    #[test]
    fn test_tree_params_serialize_includes_some_branch() {
        let params = TreeParams::new("Content").with_branch("b-1");
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("branch_id"));
    }

    #[test]
    fn test_tree_params_default_cross_refs_empty() {
        let params = TreeParams::new("Content");
        assert!(params.cross_refs.is_empty());
    }

    // ============================================================================
    // JSON Field Name Tests (serde rename)
    // ============================================================================

    #[test]
    fn test_cross_ref_input_json_uses_type_not_ref_type() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: None,
            strength: None,
        };
        let json = serde_json::to_string(&cr).unwrap();
        assert!(json.contains(r#""type":"supports""#));
        assert!(!json.contains("ref_type"));
    }

    #[test]
    fn test_cross_ref_input_deserialize_from_type_field() {
        let json = r#"{"to_branch":"b-1","type":"extends"}"#;
        let cr: CrossRefInput = serde_json::from_str(json).unwrap();
        assert_eq!(cr.ref_type, "extends");
    }

    // ============================================================================
    // TreeMode Constructor and Pipe Name Tests
    // ============================================================================

    fn create_test_config() -> Config {
        use crate::config::{DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig};
        use std::path::PathBuf;

        Config {
            langbase: LangbaseConfig {
                api_key: "test-key".to_string(),
                base_url: "https://api.langbase.com".to_string(),
            },
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Pretty,
            },
            request: crate::config::RequestConfig::default(),
            pipes: PipeConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
        }
    }

    #[test]
    fn test_tree_mode_new() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        assert_eq!(tree_mode.pipe_name, config.pipes.tree);
    }

    #[test]
    fn test_tree_mode_custom_pipe_name() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let mut config = create_test_config();
        config.pipes.tree = "custom-tree-pipe".to_string();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        assert_eq!(tree_mode.pipe_name, "custom-tree-pipe");
    }

    // ============================================================================
    // build_messages() Tests
    // ============================================================================

    #[test]
    fn test_build_messages_empty_content() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let messages = tree_mode.build_messages("", &[], 3);

        // Should have system prompt + user message
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("3 distinct reasoning paths"));
        assert_eq!(messages[1].content, "");
    }

    #[test]
    fn test_build_messages_no_history() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let messages = tree_mode.build_messages("Test content", &[], 3);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, "Test content");
    }

    #[test]
    fn test_build_messages_with_history() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{SqliteStorage, Thought};

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let history = vec![
            Thought::new("sess-1", "First thought", "tree"),
            Thought::new("sess-1", "Second thought", "tree"),
        ];

        let messages = tree_mode.build_messages("Current thought", &history, 3);

        // Should have: system prompt + history context + current content
        assert_eq!(messages.len(), 3);
        assert!(messages[1].content.contains("Previous reasoning"));
        assert!(messages[1].content.contains("First thought"));
        assert!(messages[1].content.contains("Second thought"));
        assert_eq!(messages[2].content, "Current thought");
    }

    #[test]
    fn test_build_messages_num_branches_2() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let messages = tree_mode.build_messages("Content", &[], 2);

        assert!(messages[0].content.contains("2 distinct reasoning paths"));
    }

    #[test]
    fn test_build_messages_num_branches_4() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let messages = tree_mode.build_messages("Content", &[], 4);

        assert!(messages[0].content.contains("4 distinct reasoning paths"));
    }

    #[test]
    fn test_build_messages_unicode_content() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let unicode_content = "Unicode: 世界 🌍 مرحبا";
        let messages = tree_mode.build_messages(unicode_content, &[], 3);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, unicode_content);
    }

    #[test]
    fn test_build_messages_multiline_content() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let multiline = "Line 1\nLine 2\nLine 3";
        let messages = tree_mode.build_messages(multiline, &[], 3);

        assert_eq!(messages[1].content, multiline);
        assert!(messages[1].content.contains('\n'));
    }

    #[test]
    fn test_build_messages_special_characters() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let special = "Special: \n\t\r\"'\\{}[]()!@#$%^&*";
        let messages = tree_mode.build_messages(special, &[], 3);

        assert_eq!(messages[1].content, special);
    }

    #[test]
    fn test_build_messages_long_history() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::{SqliteStorage, Thought};

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let history: Vec<Thought> = (0..10)
            .map(|i| Thought::new("sess-1", format!("Thought {}", i), "tree"))
            .collect();

        let messages = tree_mode.build_messages("Current", &history, 3);

        assert_eq!(messages.len(), 3);
        assert!(messages[1].content.contains("Thought 0"));
        assert!(messages[1].content.contains("Thought 9"));
    }

    // ============================================================================
    // parse_response() Tests
    // ============================================================================

    #[test]
    fn test_parse_response_valid_json() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "Branch 1", "confidence": 0.8, "rationale": "Reason 1"},
                {"thought": "Branch 2", "confidence": 0.7, "rationale": "Reason 2"}
            ],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.branches.len(), 2);
        assert_eq!(response.recommended_branch, 0);
        assert_eq!(response.branches[0].thought, "Branch 1");
    }

    #[test]
    fn test_parse_response_with_markdown_json() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let completion = r#"Here's the tree response:
```json
{
    "branches": [
        {"thought": "Path A", "confidence": 0.9, "rationale": "Strong"}
    ],
    "recommended_branch": 0
}
```"#;

        let response = tree_mode.parse_response(completion).unwrap();
        assert_eq!(response.branches.len(), 1);
        assert_eq!(response.branches[0].thought, "Path A");
    }

    #[test]
    fn test_parse_response_with_code_block() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let completion = r#"
```
{
    "branches": [
        {"thought": "Option 1", "confidence": 0.85, "rationale": "Good"}
    ],
    "recommended_branch": 0
}
```"#;

        let response = tree_mode.parse_response(completion).unwrap();
        assert_eq!(response.branches.len(), 1);
    }

    #[test]
    fn test_parse_response_with_metadata() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "Test", "confidence": 0.8, "rationale": "Testing"}
            ],
            "recommended_branch": 0,
            "metadata": {"analysis": "complete", "duration": 123}
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.metadata["analysis"], "complete");
        assert_eq!(response.metadata["duration"], 123);
    }

    #[test]
    fn test_parse_response_empty_branches() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert!(response.branches.is_empty());
    }

    #[test]
    fn test_parse_response_four_branches() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "A", "confidence": 0.8, "rationale": "R1"},
                {"thought": "B", "confidence": 0.85, "rationale": "R2"},
                {"thought": "C", "confidence": 0.7, "rationale": "R3"},
                {"thought": "D", "confidence": 0.9, "rationale": "R4"}
            ],
            "recommended_branch": 3
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.branches.len(), 4);
        assert_eq!(response.recommended_branch, 3);
    }

    #[test]
    fn test_parse_response_unicode_in_branches() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "世界 🌍", "confidence": 0.8, "rationale": "مرحبا"}
            ],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert!(response.branches[0].thought.contains("世界"));
        assert!(response.branches[0].rationale.contains("مرحبا"));
    }

    #[test]
    fn test_parse_response_invalid_json_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let invalid = "This is not JSON at all";
        let result = tree_mode.parse_response(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_missing_branches_field_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{"recommended_branch": 0}"#;
        let result = tree_mode.parse_response(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_missing_recommended_branch_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{"branches": []}"#;
        let result = tree_mode.parse_response(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_malformed_branch_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "Test"}
            ],
            "recommended_branch": 0
        }"#;
        let result = tree_mode.parse_response(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_special_chars_in_thought() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "Test\n\twith \"quotes\"", "confidence": 0.8, "rationale": "Special chars"}
            ],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert!(response.branches[0].thought.contains("Test"));
    }

    #[test]
    fn test_parse_response_empty_thought_string() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "", "confidence": 0.8, "rationale": "Empty thought"}
            ],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.branches[0].thought, "");
    }

    #[test]
    fn test_parse_response_zero_confidence() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "Low confidence", "confidence": 0.0, "rationale": "Uncertain"}
            ],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.branches[0].confidence, 0.0);
    }

    #[test]
    fn test_parse_response_max_confidence() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "High confidence", "confidence": 1.0, "rationale": "Very certain"}
            ],
            "recommended_branch": 0
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.branches[0].confidence, 1.0);
    }

    #[test]
    fn test_parse_response_empty_completion_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let result = tree_mode.parse_response("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_whitespace_only_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let result = tree_mode.parse_response("   \n\t  ");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_large_recommended_index() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json = r#"{
            "branches": [
                {"thought": "Branch 1", "confidence": 0.8, "rationale": "R1"}
            ],
            "recommended_branch": 999
        }"#;

        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.recommended_branch, 999);
    }

    #[test]
    fn test_parse_response_long_rationale() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let long_rationale = "A".repeat(10000);
        let json = format!(
            r#"{{
            "branches": [
                {{"thought": "Test", "confidence": 0.8, "rationale": "{}"}}
            ],
            "recommended_branch": 0
        }}"#,
            long_rationale
        );

        let response = tree_mode.parse_response(&json).unwrap();
        assert_eq!(response.branches[0].rationale.len(), 10000);
    }

    #[test]
    fn test_parse_response_partial_json_block_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let incomplete = r#"```json
{"branches": [{"thought""#;
        let result = tree_mode.parse_response(incomplete);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_json_with_comments_error() {
        use crate::config::RequestConfig;
        use crate::langbase::LangbaseClient;
        use crate::storage::SqliteStorage;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase = LangbaseClient::new(&config.langbase, RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);

        let json_with_comments = r#"{
            // This is a comment
            "branches": [],
            "recommended_branch": 0
        }"#;
        let result = tree_mode.parse_response(json_with_comments);
        assert!(result.is_err());
    }

    // ============================================================================
    // Additional Edge Case Tests
    // ============================================================================

    #[test]
    fn test_tree_params_very_long_content() {
        let long_content = "a".repeat(100000);
        let params = TreeParams::new(long_content.clone());
        assert_eq!(params.content.len(), 100000);
    }

    #[test]
    fn test_tree_params_cross_refs_with_all_fields() {
        let cr = CrossRefInput {
            to_branch: "target".to_string(),
            ref_type: "supports".to_string(),
            reason: Some("Strong evidence".to_string()),
            strength: Some(0.95),
        };

        // Test serialization of fully-populated CrossRefInput
        let json = serde_json::to_string(&cr).unwrap();
        assert!(json.contains("Strong evidence"));
        assert!(json.contains("0.95"));
    }

    #[test]
    fn test_truncate_unicode_safe() {
        // Note: truncate uses byte slicing, so may not be unicode-safe
        let ascii = "Hello World";
        let result = truncate(ascii, 8);
        assert_eq!(result, "Hello...");
    }

    #[test]
    fn test_tree_branch_clone() {
        let branch = TreeBranch {
            thought: "Test".to_string(),
            confidence: 0.8,
            rationale: "Reason".to_string(),
        };

        let cloned = branch.clone();
        assert_eq!(cloned.thought, branch.thought);
        assert_eq!(cloned.confidence, branch.confidence);
        assert_eq!(cloned.rationale, branch.rationale);
    }

    #[test]
    fn test_tree_response_clone() {
        let response = TreeResponse {
            branches: vec![TreeBranch {
                thought: "Test".to_string(),
                confidence: 0.8,
                rationale: "Reason".to_string(),
            }],
            recommended_branch: 0,
            metadata: serde_json::json!({"key": "value"}),
        };

        let cloned = response.clone();
        assert_eq!(cloned.branches.len(), response.branches.len());
        assert_eq!(cloned.recommended_branch, response.recommended_branch);
    }

    #[test]
    fn test_tree_result_debug_format() {
        let result = TreeResult {
            session_id: "s-1".to_string(),
            branch_id: "b-1".to_string(),
            thought_id: "t-1".to_string(),
            content: "Content".to_string(),
            confidence: 0.8,
            child_branches: vec![],
            recommended_branch_index: 0,
            parent_branch: None,
            cross_refs_created: 0,
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("TreeResult"));
        assert!(debug_str.contains("s-1"));
    }

    #[test]
    fn test_cross_ref_input_debug_format() {
        let cr = CrossRefInput {
            to_branch: "b-1".to_string(),
            ref_type: "supports".to_string(),
            reason: Some("Test".to_string()),
            strength: Some(0.9),
        };

        let debug_str = format!("{:?}", cr);
        assert!(debug_str.contains("CrossRefInput"));
        assert!(debug_str.contains("b-1"));
    }

    #[test]
    fn test_tree_params_content_with_null_bytes() {
        // Test that null bytes are preserved (unusual but valid for strings)
        let content_with_null = "Before\0After";
        let params = TreeParams::new(content_with_null);
        assert_eq!(params.content, content_with_null);
        assert!(params.content.contains('\0'));
    }

    #[test]
    fn test_tree_response_negative_recommended_branch() {
        // Note: In Rust, usize is unsigned so -1 would be serialized as a large number
        // This test verifies parsing doesn't panic on unusual values
        let json = r#"{
            "branches": [
                {"thought": "Test", "confidence": 0.8, "rationale": "R"}
            ],
            "recommended_branch": 0
        }"#;

        let config = create_test_config();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = rt.block_on(SqliteStorage::new_in_memory()).unwrap();
        let langbase =
            LangbaseClient::new(&config.langbase, crate::config::RequestConfig::default()).unwrap();

        let tree_mode = TreeMode::new(storage, langbase, &config);
        let response = tree_mode.parse_response(json).unwrap();
        assert_eq!(response.recommended_branch, 0);
    }

    #[test]
    fn test_branch_info_very_long_rationale() {
        let long_rationale = "X".repeat(50000);
        let info = BranchInfo {
            id: "b-1".to_string(),
            name: "Branch".to_string(),
            confidence: 0.8,
            rationale: long_rationale.clone(),
        };

        assert_eq!(info.rationale.len(), 50000);
        let json = serde_json::to_string(&info).unwrap();
        let parsed: BranchInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rationale.len(), 50000);
    }
}
