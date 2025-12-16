use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::TREE_REASONING_PROMPT;
use crate::storage::{Branch, BranchState, CrossRef, CrossRefType, Invocation, Session, SqliteStorage, Storage, Thought};

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

/// Cross-reference input for tree reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossRefInput {
    pub to_branch: String,
    #[serde(rename = "type")]
    pub ref_type: String,
    pub reason: Option<String>,
    pub strength: Option<f64>,
}

/// Response from tree reasoning Langbase pipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResponse {
    pub branches: Vec<TreeBranch>,
    pub recommended_branch: usize,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Individual branch in tree response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeBranch {
    pub thought: String,
    pub confidence: f64,
    pub rationale: String,
}

/// Result of tree reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResult {
    pub session_id: String,
    pub branch_id: String,
    pub thought_id: String,
    pub content: String,
    pub confidence: f64,
    pub child_branches: Vec<BranchInfo>,
    pub recommended_branch_index: usize,
    pub parent_branch: Option<String>,
    pub cross_refs_created: usize,
}

/// Branch information in result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub id: String,
    pub name: String,
    pub confidence: f64,
    pub rationale: String,
}

/// Tree reasoning mode handler
pub struct TreeMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}

impl TreeMode {
    /// Create a new tree mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
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
        let session = self.get_or_create_session(&params.session_id).await?;
        debug!(session_id = %session.id, "Processing tree reasoning");

        // Get or create branch
        let parent_branch = match &params.branch_id {
            Some(id) => self.storage.get_branch(id).await?,
            None => None,
        };

        // Create the current branch if extending or creating new
        let branch = match &parent_branch {
            Some(parent) => {
                let b = Branch::new(&session.id)
                    .with_parent(&parent.id)
                    .with_name(format!("Branch from {}", &parent.id[..8]))
                    .with_confidence(params.confidence);
                self.storage.create_branch(&b).await?;
                b
            }
            None => {
                // Check if session has an active branch, or create root
                match &session.active_branch_id {
                    Some(active_id) => {
                        self.storage.get_branch(active_id).await?.unwrap_or_else(|| {
                            Branch::new(&session.id).with_name("Root")
                        })
                    }
                    None => {
                        let b = Branch::new(&session.id).with_name("Root");
                        self.storage.create_branch(&b).await?;
                        // Update session with active branch
                        let mut updated_session = session.clone();
                        updated_session.active_branch_id = Some(b.id.clone());
                        self.storage.update_session(&updated_session).await?;
                        b
                    }
                }
            }
        };

        // Get context from branch history
        let branch_thoughts = self.storage.get_branch_thoughts(&branch.id).await?;

        // Build messages for Langbase
        let messages = self.build_messages(&params.content, &branch_thoughts, num_branches);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.tree",
            serde_json::to_value(&params).unwrap_or_default(),
        )
        .with_session(&session.id)
        .with_pipe(&self.pipe_name);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.pipe_name, messages);
        let response = match self.langbase.call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                self.storage.log_invocation(&invocation).await?;
                return Err(e.into());
            }
        };

        // Parse response
        let tree_response = self.parse_response(&response.completion)?;

        // Create main thought for this branch
        let thought = Thought::new(&session.id, &params.content, "tree")
            .with_confidence(params.confidence)
            .with_branch(&branch.id);
        self.storage.create_thought(&thought).await?;

        // Create child branches for each explored path
        let mut child_branches = Vec::new();
        for (i, tb) in tree_response.branches.iter().enumerate() {
            let child = Branch::new(&session.id)
                .with_parent(&branch.id)
                .with_name(format!("Option {}: {}", i + 1, truncate(&tb.thought, 30)))
                .with_confidence(tb.confidence)
                .with_priority(if i == tree_response.recommended_branch { 2.0 } else { 1.0 });

            self.storage.create_branch(&child).await?;

            // Create thought for this branch
            let child_thought = Thought::new(&session.id, &tb.thought, "tree")
                .with_confidence(tb.confidence)
                .with_branch(&child.id)
                .with_parent(&thought.id);
            self.storage.create_thought(&child_thought).await?;

            child_branches.push(BranchInfo {
                id: child.id,
                name: child.name.unwrap_or_default(),
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
                self.storage.create_cross_ref(&cr).await?;
                cross_refs_created += 1;
            }
        }

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serde_json::to_value(&tree_response).unwrap_or_default(),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

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
        let branch = self.storage.get_branch(branch_id).await?
            .ok_or_else(|| ToolError::Session(format!("Branch not found: {}", branch_id)))?;

        // Verify branch belongs to session
        if branch.session_id != session_id {
            return Err(ToolError::Session("Branch does not belong to this session".to_string()).into());
        }

        // Update session's active branch
        let session = self.storage.get_session(session_id).await?
            .ok_or_else(|| ToolError::Session(format!("Session not found: {}", session_id)))?;

        let mut updated_session = session;
        updated_session.active_branch_id = Some(branch_id.to_string());
        self.storage.update_session(&updated_session).await?;

        Ok(branch)
    }

    /// Get all branches for a session
    pub async fn list_branches(&self, session_id: &str) -> AppResult<Vec<Branch>> {
        Ok(self.storage.get_session_branches(session_id).await?)
    }

    /// Update branch state (complete, abandon)
    pub async fn update_branch_state(&self, branch_id: &str, state: BranchState) -> AppResult<Branch> {
        let mut branch = self.storage.get_branch(branch_id).await?
            .ok_or_else(|| ToolError::Session(format!("Branch not found: {}", branch_id)))?;

        branch.state = state;
        branch.updated_at = chrono::Utc::now();
        self.storage.update_branch(&branch).await?;

        Ok(branch)
    }

    async fn get_or_create_session(&self, session_id: &Option<String>) -> AppResult<Session> {
        match session_id {
            Some(id) => {
                match self.storage.get_session(id).await? {
                    Some(s) => Ok(s),
                    None => {
                        let mut new_session = Session::new("tree");
                        new_session.id = id.clone();
                        self.storage.create_session(&new_session).await?;
                        Ok(new_session)
                    }
                }
            }
            None => {
                let session = Session::new("tree");
                self.storage.create_session(&session).await?;
                Ok(session)
            }
        }
    }

    fn build_messages(&self, content: &str, history: &[Thought], num_branches: usize) -> Vec<Message> {
        let mut messages = Vec::new();

        // System prompt for tree reasoning
        let system_prompt = TREE_REASONING_PROMPT.replace(
            "2-4 distinct reasoning paths",
            &format!("{} distinct reasoning paths", num_branches)
        );
        messages.push(Message::system(system_prompt));

        // Add history context if available
        if !history.is_empty() {
            let history_text: Vec<String> = history
                .iter()
                .map(|t| format!("- {}", t.content))
                .collect();

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
        // Try to parse as JSON first
        if let Ok(response) = serde_json::from_str::<TreeResponse>(completion) {
            return Ok(response);
        }

        // Try to extract JSON from markdown code blocks
        let json_str = if completion.contains("```json") {
            completion
                .split("```json")
                .nth(1)
                .and_then(|s| s.split("```").next())
                .unwrap_or(completion)
        } else if completion.contains("```") {
            completion
                .split("```")
                .nth(1)
                .unwrap_or(completion)
        } else {
            completion
        };

        serde_json::from_str::<TreeResponse>(json_str.trim()).map_err(|e| {
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
    pub fn with_cross_ref(mut self, to_branch: impl Into<String>, ref_type: impl Into<String>) -> Self {
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
