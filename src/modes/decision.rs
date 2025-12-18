//! Decision framework reasoning mode - structured multi-criteria decision making.
//!
//! This module provides decision-making capabilities:
//! - Multi-criteria decision analysis with weighted scoring
//! - Sensitivity analysis on criteria weights
//! - Trade-off identification between options
//! - Stakeholder perspective analysis with power/interest mapping
//! - Conflict and alignment detection

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::{DECISION_MAKER_PROMPT, PERSPECTIVE_ANALYZER_PROMPT};
use crate::storage::{
    Decision as StoredDecision, Invocation, PerspectiveAnalysis as StoredPerspective, Session,
    SqliteStorage, Storage, StoredCriterion,
};

// ============================================================================
// Parameters
// ============================================================================

/// Input parameters for multi-criteria decision making.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionParams {
    /// The decision question to analyze.
    pub question: String,
    /// Available options/alternatives (2-6 options).
    pub options: Vec<String>,
    /// Evaluation criteria with weights (should sum to 1.0).
    #[serde(default)]
    pub criteria: Vec<Criterion>,
    /// Hard constraints that must be satisfied.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Optional session ID (creates new if not provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Decision method to use.
    #[serde(default)]
    pub method: DecisionMethod,
}

/// A single criterion for decision evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    /// Criterion name.
    pub name: String,
    /// Weight (0.0-1.0).
    pub weight: f64,
    /// Description of the criterion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Decision analysis method.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMethod {
    /// Weighted Sum Model - simple additive weighting.
    #[default]
    WeightedSum,
    /// Pairwise Comparison - AHP-style comparison between alternatives.
    Pairwise,
    /// TOPSIS - Technique for Order Preference by Similarity to Ideal Solution.
    Topsis,
}

impl std::fmt::Display for DecisionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionMethod::WeightedSum => write!(f, "weighted_sum"),
            DecisionMethod::Pairwise => write!(f, "pairwise"),
            DecisionMethod::Topsis => write!(f, "topsis"),
        }
    }
}

/// Input parameters for stakeholder perspective analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveParams {
    /// The topic or decision to analyze from multiple perspectives.
    pub topic: String,
    /// Stakeholders to consider (optional - will infer if not provided).
    #[serde(default)]
    pub stakeholders: Vec<StakeholderInput>,
    /// Additional context about the situation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Optional session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Include power/interest analysis.
    #[serde(default = "default_true")]
    pub include_power_matrix: bool,
}

fn default_true() -> bool {
    true
}

/// Stakeholder input for perspective analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeholderInput {
    /// Stakeholder name.
    pub name: String,
    /// Stakeholder role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Known interests.
    #[serde(default)]
    pub interests: Vec<String>,
}

// ============================================================================
// Langbase Response Types
// ============================================================================

/// Response from decision maker Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DecisionResponse {
    recommendation: Recommendation,
    scores: Vec<OptionScore>,
    sensitivity_analysis: SensitivityAnalysis,
    trade_offs: Vec<TradeOffResponse>,
    constraints_satisfied: HashMap<String, bool>,
    #[serde(default)]
    metadata: serde_json::Value,
}

/// Response format for trade-offs from Langbase.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TradeOffResponse {
    between: Vec<String>,
    trade_off: String,
}

/// Response from perspective analyzer Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerspectiveResponse {
    stakeholders: Vec<StakeholderAnalysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    power_matrix: Option<PowerMatrix>,
    conflicts: Vec<ConflictResponse>,
    alignments: Vec<AlignmentResponse>,
    synthesis: Synthesis,
    confidence: f64,
    #[serde(default)]
    metadata: serde_json::Value,
}

/// Conflict response from Langbase (array format).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConflictResponse {
    stakeholders: Vec<String>,
    issue: String,
    severity: f64,
    resolution_approach: String,
}

/// Alignment response from Langbase (array format).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlignmentResponse {
    stakeholders: Vec<String>,
    shared_interest: String,
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of multi-criteria decision analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResult {
    /// Unique decision ID.
    pub decision_id: String,
    /// Session ID.
    pub session_id: String,
    /// The question analyzed.
    pub question: String,
    /// The recommendation.
    pub recommendation: Recommendation,
    /// Scores for all options.
    pub scores: Vec<OptionScore>,
    /// Sensitivity analysis results.
    pub sensitivity_analysis: SensitivityAnalysis,
    /// Trade-offs between top options.
    pub trade_offs: Vec<TradeOff>,
    /// Constraint satisfaction per option.
    pub constraints_satisfied: HashMap<String, bool>,
}

/// The recommended option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// The recommended option.
    pub option: String,
    /// Overall score.
    pub score: f64,
    /// Confidence in the recommendation.
    pub confidence: f64,
    /// Rationale for the recommendation.
    pub rationale: String,
}

/// Score breakdown for an option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionScore {
    /// The option.
    pub option: String,
    /// Total weighted score.
    pub total_score: f64,
    /// Score breakdown by criterion.
    pub criteria_scores: HashMap<String, CriterionScore>,
    /// Rank (1 = best).
    pub rank: usize,
}

/// Score for a single criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionScore {
    /// Raw score (0.0-1.0).
    pub score: f64,
    /// Reasoning for the score.
    pub reasoning: String,
}

/// Sensitivity analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityAnalysis {
    /// Whether the recommendation is robust.
    pub robust: bool,
    /// Criteria that most affect the outcome.
    pub critical_criteria: Vec<String>,
    /// How much each criterion weight can change before ranking changes.
    pub threshold_changes: HashMap<String, f64>,
}

/// Trade-off between options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOff {
    /// Options being compared.
    pub between: (String, String),
    /// Description of the trade-off.
    pub trade_off: String,
}

/// Result of stakeholder perspective analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveResult {
    /// Unique analysis ID.
    pub analysis_id: String,
    /// Session ID.
    pub session_id: String,
    /// The topic analyzed.
    pub topic: String,
    /// Analyzed stakeholders.
    pub stakeholders: Vec<StakeholderAnalysis>,
    /// Power/interest matrix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_matrix: Option<PowerMatrix>,
    /// Identified conflicts.
    pub conflicts: Vec<Conflict>,
    /// Identified alignments.
    pub alignments: Vec<Alignment>,
    /// Synthesis of perspectives.
    pub synthesis: Synthesis,
    /// Overall confidence.
    pub confidence: f64,
}

/// Analysis of a single stakeholder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeholderAnalysis {
    /// Stakeholder name.
    pub name: String,
    /// Role.
    pub role: String,
    /// Their perspective on the topic.
    pub perspective: String,
    /// Their interests.
    pub interests: Vec<String>,
    /// Their concerns.
    pub concerns: Vec<String>,
    /// Power level (0.0-1.0).
    pub power_level: f64,
    /// Interest level (0.0-1.0).
    pub interest_level: f64,
    /// Quadrant in power/interest matrix.
    pub quadrant: Quadrant,
    /// Recommended engagement strategy.
    pub engagement_strategy: String,
}

/// Quadrant in power/interest matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quadrant {
    /// High power, high interest.
    KeyPlayer,
    /// High power, low interest.
    KeepSatisfied,
    /// Low power, high interest.
    KeepInformed,
    /// Low power, low interest.
    MinimalEffort,
}

impl std::fmt::Display for Quadrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Quadrant::KeyPlayer => write!(f, "key_player"),
            Quadrant::KeepSatisfied => write!(f, "keep_satisfied"),
            Quadrant::KeepInformed => write!(f, "keep_informed"),
            Quadrant::MinimalEffort => write!(f, "minimal_effort"),
        }
    }
}

/// Power/interest matrix breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMatrix {
    /// Key players (high power, high interest).
    pub key_players: Vec<String>,
    /// Keep satisfied (high power, low interest).
    pub keep_satisfied: Vec<String>,
    /// Keep informed (low power, high interest).
    pub keep_informed: Vec<String>,
    /// Minimal effort (low power, low interest).
    pub minimal_effort: Vec<String>,
}

/// Conflict between stakeholders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    /// Stakeholders in conflict.
    pub stakeholders: (String, String),
    /// The issue causing conflict.
    pub issue: String,
    /// Severity (0.0-1.0).
    pub severity: f64,
    /// Suggested resolution approach.
    pub resolution_approach: String,
}

/// Alignment between stakeholders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alignment {
    /// Aligned stakeholders.
    pub stakeholders: (String, String),
    /// Shared interest.
    pub shared_interest: String,
}

/// Synthesis of stakeholder perspectives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synthesis {
    /// Areas of consensus.
    pub consensus_areas: Vec<String>,
    /// Contentious areas.
    pub contentious_areas: Vec<String>,
    /// Overall recommendation.
    pub recommendation: String,
}

// ============================================================================
// Mode Handler
// ============================================================================

/// Decision framework mode handler.
#[derive(Clone)]
pub struct DecisionMode {
    /// Storage backend for persisting data.
    storage: SqliteStorage,
    /// Langbase client for LLM-powered reasoning.
    langbase: LangbaseClient,
    /// The Langbase pipe name for decision analysis.
    decision_pipe: String,
    /// The Langbase pipe name for perspective analysis.
    perspective_pipe: String,
}

impl DecisionMode {
    /// Create a new decision mode handler.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            decision_pipe: config
                .pipes
                .decision
                .as_ref()
                .and_then(|d| d.decision_pipe.clone())
                .unwrap_or_else(|| "decision-maker-v1".to_string()),
            perspective_pipe: config
                .pipes
                .decision
                .as_ref()
                .and_then(|d| d.perspective_pipe.clone())
                .unwrap_or_else(|| "perspective-analyzer-v1".to_string()),
        }
    }

    /// Process a multi-criteria decision request.
    pub async fn make_decision(&self, params: DecisionParams) -> AppResult<DecisionResult> {
        let start = Instant::now();

        // Validate input
        self.validate_decision_params(&params)?;

        // Get or create session
        let session = self.get_or_create_session(&params.session_id).await?;
        debug!(session_id = %session.id, "Processing decision analysis");

        // Build messages for Langbase
        let messages = self.build_decision_messages(&params);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.make_decision",
            serialize_for_log(&params, "reasoning.make_decision input"),
        )
        .with_session(&session.id)
        .with_pipe(&self.decision_pipe);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.decision_pipe, messages);
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
        let decision_response = self.parse_decision_response(&response.completion)?;

        // Generate decision ID
        let decision_id = uuid::Uuid::new_v4().to_string();

        // Convert trade-offs from response format
        let trade_offs: Vec<TradeOff> = decision_response
            .trade_offs
            .iter()
            .map(|t| TradeOff {
                between: (
                    t.between.first().cloned().unwrap_or_default(),
                    t.between.get(1).cloned().unwrap_or_default(),
                ),
                trade_off: t.trade_off.clone(),
            })
            .collect();

        // Build result
        let result = DecisionResult {
            decision_id: decision_id.clone(),
            session_id: session.id.clone(),
            question: params.question.clone(),
            recommendation: decision_response.recommendation,
            scores: decision_response.scores,
            sensitivity_analysis: decision_response.sensitivity_analysis,
            trade_offs,
            constraints_satisfied: decision_response.constraints_satisfied,
        };

        // Persist to storage
        let stored_criteria: Vec<StoredCriterion> = params
            .criteria
            .iter()
            .map(|c| StoredCriterion {
                name: c.name.clone(),
                weight: c.weight,
                description: c.description.clone(),
            })
            .collect();

        let stored_decision = StoredDecision::new(
            &session.id,
            &params.question,
            params.options.clone(),
            params.method.to_string(),
            serde_json::to_value(&result.recommendation).unwrap_or_default(),
            serde_json::to_value(&result.scores).unwrap_or_default(),
        )
        .with_criteria(stored_criteria)
        .with_sensitivity(serde_json::to_value(&result.sensitivity_analysis).unwrap_or_default())
        .with_trade_offs(serde_json::to_value(&result.trade_offs).unwrap_or_default())
        .with_constraints(serde_json::to_value(&result.constraints_satisfied).unwrap_or_default());

        if let Err(e) = self.storage.create_decision(&stored_decision).await {
            warn!(
                error = %e,
                decision_id = %decision_id,
                "Failed to persist decision to storage"
            );
        }

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.make_decision output"),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

        info!(
            decision_id = %decision_id,
            recommendation = %result.recommendation.option,
            latency_ms = latency,
            "Decision analysis completed"
        );

        Ok(result)
    }

    /// Process a stakeholder perspective analysis request.
    pub async fn analyze_perspectives(
        &self,
        params: PerspectiveParams,
    ) -> AppResult<PerspectiveResult> {
        let start = Instant::now();

        // Validate input
        if params.topic.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "topic".to_string(),
                reason: "Topic cannot be empty".to_string(),
            }
            .into());
        }

        // Get or create session
        let session = self.get_or_create_session(&params.session_id).await?;
        debug!(session_id = %session.id, "Processing perspective analysis");

        // Build messages for Langbase
        let messages = self.build_perspective_messages(&params);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.analyze_perspectives",
            serialize_for_log(&params, "reasoning.analyze_perspectives input"),
        )
        .with_session(&session.id)
        .with_pipe(&self.perspective_pipe);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.perspective_pipe, messages);
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
        let perspective_response = self.parse_perspective_response(&response.completion)?;

        // Generate analysis ID
        let analysis_id = uuid::Uuid::new_v4().to_string();

        // Convert conflicts from response format
        let conflicts: Vec<Conflict> = perspective_response
            .conflicts
            .iter()
            .map(|c| Conflict {
                stakeholders: (
                    c.stakeholders.first().cloned().unwrap_or_default(),
                    c.stakeholders.get(1).cloned().unwrap_or_default(),
                ),
                issue: c.issue.clone(),
                severity: c.severity,
                resolution_approach: c.resolution_approach.clone(),
            })
            .collect();

        // Convert alignments from response format
        let alignments: Vec<Alignment> = perspective_response
            .alignments
            .iter()
            .map(|a| Alignment {
                stakeholders: (
                    a.stakeholders.first().cloned().unwrap_or_default(),
                    a.stakeholders.get(1).cloned().unwrap_or_default(),
                ),
                shared_interest: a.shared_interest.clone(),
            })
            .collect();

        // Build result
        let result = PerspectiveResult {
            analysis_id: analysis_id.clone(),
            session_id: session.id.clone(),
            topic: params.topic.clone(),
            stakeholders: perspective_response.stakeholders,
            power_matrix: perspective_response.power_matrix,
            conflicts,
            alignments,
            synthesis: perspective_response.synthesis,
            confidence: perspective_response.confidence,
        };

        // Persist to storage
        let mut stored_perspective = StoredPerspective::new(
            &session.id,
            &params.topic,
            serde_json::to_value(&result.stakeholders).unwrap_or_default(),
            serde_json::to_value(&result.synthesis).unwrap_or_default(),
            result.confidence,
        )
        .with_conflicts(serde_json::to_value(&result.conflicts).unwrap_or_default())
        .with_alignments(serde_json::to_value(&result.alignments).unwrap_or_default());

        if let Some(pm) = &result.power_matrix {
            stored_perspective =
                stored_perspective.with_power_matrix(serde_json::to_value(pm).unwrap_or_default());
        }

        if let Err(e) = self.storage.create_perspective(&stored_perspective).await {
            warn!(
                error = %e,
                analysis_id = %analysis_id,
                "Failed to persist perspective analysis to storage"
            );
        }

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.analyze_perspectives output"),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

        info!(
            analysis_id = %analysis_id,
            stakeholder_count = result.stakeholders.len(),
            latency_ms = latency,
            "Perspective analysis completed"
        );

        Ok(result)
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    fn validate_decision_params(&self, params: &DecisionParams) -> AppResult<()> {
        if params.question.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "question".to_string(),
                reason: "Question cannot be empty".to_string(),
            }
            .into());
        }

        if params.options.len() < 2 {
            return Err(ToolError::Validation {
                field: "options".to_string(),
                reason: "At least 2 options are required".to_string(),
            }
            .into());
        }

        if params.options.len() > 6 {
            return Err(ToolError::Validation {
                field: "options".to_string(),
                reason: "Maximum 6 options allowed".to_string(),
            }
            .into());
        }

        // Validate criteria weights if provided
        if !params.criteria.is_empty() {
            let total_weight: f64 = params.criteria.iter().map(|c| c.weight).sum();
            if (total_weight - 1.0).abs() > 0.01 {
                warn!(
                    total_weight = total_weight,
                    "Criteria weights do not sum to 1.0"
                );
            }
        }

        Ok(())
    }

    async fn get_or_create_session(&self, session_id: &Option<String>) -> AppResult<Session> {
        match session_id {
            Some(id) => match self.storage.get_session(id).await? {
                Some(s) => Ok(s),
                None => {
                    let mut new_session = Session::new("decision");
                    new_session.id = id.clone();
                    self.storage.create_session(&new_session).await?;
                    Ok(new_session)
                }
            },
            None => {
                let session = Session::new("decision");
                self.storage.create_session(&session).await?;
                Ok(session)
            }
        }
    }

    fn build_decision_messages(&self, params: &DecisionParams) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(DECISION_MAKER_PROMPT.to_string()));

        // Build user message with decision context
        let mut user_content = format!("Decision Question: {}\n\n", params.question);
        user_content.push_str(&format!(
            "Options:\n{}\n",
            params
                .options
                .iter()
                .enumerate()
                .map(|(i, o)| format!("{}. {}", i + 1, o))
                .collect::<Vec<_>>()
                .join("\n")
        ));

        if !params.criteria.is_empty() {
            user_content.push_str("\nEvaluation Criteria:\n");
            for c in &params.criteria {
                user_content.push_str(&format!(
                    "- {} (weight: {:.2}){}\n",
                    c.name,
                    c.weight,
                    c.description
                        .as_ref()
                        .map(|d| format!(": {}", d))
                        .unwrap_or_default()
                ));
            }
        }

        if !params.constraints.is_empty() {
            user_content.push_str("\nConstraints:\n");
            for c in &params.constraints {
                user_content.push_str(&format!("- {}\n", c));
            }
        }

        user_content.push_str(&format!("\nAnalysis Method: {}", params.method));

        messages.push(Message::user(user_content));
        messages
    }

    fn build_perspective_messages(&self, params: &PerspectiveParams) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(PERSPECTIVE_ANALYZER_PROMPT.to_string()));

        // Build user message with topic and context
        let mut user_content = format!("Topic: {}\n", params.topic);

        if let Some(ref context) = params.context {
            user_content.push_str(&format!("\nContext: {}\n", context));
        }

        if !params.stakeholders.is_empty() {
            user_content.push_str("\nKnown Stakeholders:\n");
            for s in &params.stakeholders {
                user_content.push_str(&format!(
                    "- {}{}{}\n",
                    s.name,
                    s.role.as_ref().map(|r| format!(" ({})", r)).unwrap_or_default(),
                    if !s.interests.is_empty() {
                        format!(" - Interests: {}", s.interests.join(", "))
                    } else {
                        String::new()
                    }
                ));
            }
        } else {
            user_content.push_str("\nPlease infer relevant stakeholders for this topic.\n");
        }

        if params.include_power_matrix {
            user_content.push_str("\nPlease include power/interest matrix analysis.\n");
        }

        messages.push(Message::user(user_content));
        messages
    }

    fn parse_decision_response(&self, completion: &str) -> AppResult<DecisionResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from decision response"
            );
            ToolError::Reasoning {
                message: format!("Decision response extraction failed: {}", e),
            }
        })?;

        serde_json::from_str::<DecisionResponse>(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse decision response: {}", e),
            }
            .into()
        })
    }

    fn parse_perspective_response(&self, completion: &str) -> AppResult<PerspectiveResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from perspective response"
            );
            ToolError::Reasoning {
                message: format!("Perspective response extraction failed: {}", e),
            }
        })?;

        serde_json::from_str::<PerspectiveResponse>(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse perspective response: {}", e),
            }
            .into()
        })
    }
}

// ============================================================================
// Builder Methods
// ============================================================================

impl DecisionParams {
    /// Create new decision params with question and options.
    pub fn new(question: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            question: question.into(),
            options,
            criteria: Vec::new(),
            constraints: Vec::new(),
            session_id: None,
            method: DecisionMethod::default(),
        }
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add a criterion.
    pub fn with_criterion(mut self, name: impl Into<String>, weight: f64) -> Self {
        self.criteria.push(Criterion {
            name: name.into(),
            weight,
            description: None,
        });
        self
    }

    /// Add a constraint.
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Set the decision method.
    pub fn with_method(mut self, method: DecisionMethod) -> Self {
        self.method = method;
        self
    }
}

impl PerspectiveParams {
    /// Create new perspective params with topic.
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            stakeholders: Vec::new(),
            context: None,
            session_id: None,
            include_power_matrix: true,
        }
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add a stakeholder.
    pub fn with_stakeholder(mut self, name: impl Into<String>) -> Self {
        self.stakeholders.push(StakeholderInput {
            name: name.into(),
            role: None,
            interests: Vec::new(),
        });
        self
    }

    /// Set the context.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Disable power matrix analysis.
    pub fn without_power_matrix(mut self) -> Self {
        self.include_power_matrix = false;
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // DecisionParams Tests
    // ========================================================================

    #[test]
    fn test_decision_params_new() {
        let params = DecisionParams::new(
            "Which option to choose?",
            vec!["Option A".to_string(), "Option B".to_string()],
        );
        assert_eq!(params.question, "Which option to choose?");
        assert_eq!(params.options.len(), 2);
        assert!(params.criteria.is_empty());
        assert!(params.constraints.is_empty());
        assert!(params.session_id.is_none());
        assert_eq!(params.method, DecisionMethod::WeightedSum);
    }

    #[test]
    fn test_decision_params_with_session() {
        let params = DecisionParams::new("Q", vec!["A".to_string(), "B".to_string()])
            .with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_decision_params_with_criterion() {
        let params = DecisionParams::new("Q", vec!["A".to_string(), "B".to_string()])
            .with_criterion("cost", 0.5)
            .with_criterion("quality", 0.5);
        assert_eq!(params.criteria.len(), 2);
        assert_eq!(params.criteria[0].name, "cost");
        assert_eq!(params.criteria[0].weight, 0.5);
    }

    #[test]
    fn test_decision_params_with_constraint() {
        let params = DecisionParams::new("Q", vec!["A".to_string(), "B".to_string()])
            .with_constraint("Must be under budget");
        assert_eq!(params.constraints.len(), 1);
        assert_eq!(params.constraints[0], "Must be under budget");
    }

    #[test]
    fn test_decision_params_with_method() {
        let params = DecisionParams::new("Q", vec!["A".to_string(), "B".to_string()])
            .with_method(DecisionMethod::Topsis);
        assert_eq!(params.method, DecisionMethod::Topsis);
    }

    #[test]
    fn test_decision_method_display() {
        assert_eq!(format!("{}", DecisionMethod::WeightedSum), "weighted_sum");
        assert_eq!(format!("{}", DecisionMethod::Pairwise), "pairwise");
        assert_eq!(format!("{}", DecisionMethod::Topsis), "topsis");
    }

    #[test]
    fn test_decision_params_serialize() {
        let params = DecisionParams::new("Choose", vec!["A".to_string(), "B".to_string()])
            .with_session("s-1")
            .with_criterion("cost", 0.6);

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Choose"));
        assert!(json.contains("s-1"));
        assert!(json.contains("cost"));
        assert!(json.contains("0.6"));
    }

    #[test]
    fn test_decision_params_deserialize() {
        let json = r#"{
            "question": "Which?",
            "options": ["X", "Y", "Z"],
            "criteria": [{"name": "speed", "weight": 0.7}],
            "method": "topsis"
        }"#;
        let params: DecisionParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.question, "Which?");
        assert_eq!(params.options.len(), 3);
        assert_eq!(params.criteria.len(), 1);
        assert_eq!(params.method, DecisionMethod::Topsis);
    }

    #[test]
    fn test_decision_params_deserialize_minimal() {
        let json = r#"{"question": "Q?", "options": ["A", "B"]}"#;
        let params: DecisionParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.question, "Q?");
        assert_eq!(params.options.len(), 2);
        assert!(params.criteria.is_empty());
        assert!(params.constraints.is_empty());
        assert_eq!(params.method, DecisionMethod::WeightedSum);
    }

    // ========================================================================
    // PerspectiveParams Tests
    // ========================================================================

    #[test]
    fn test_perspective_params_new() {
        let params = PerspectiveParams::new("New policy");
        assert_eq!(params.topic, "New policy");
        assert!(params.stakeholders.is_empty());
        assert!(params.context.is_none());
        assert!(params.session_id.is_none());
        assert!(params.include_power_matrix);
    }

    #[test]
    fn test_perspective_params_with_stakeholder() {
        let params = PerspectiveParams::new("Topic")
            .with_stakeholder("Alice")
            .with_stakeholder("Bob");
        assert_eq!(params.stakeholders.len(), 2);
        assert_eq!(params.stakeholders[0].name, "Alice");
    }

    #[test]
    fn test_perspective_params_with_context() {
        let params = PerspectiveParams::new("Topic").with_context("Some context");
        assert_eq!(params.context, Some("Some context".to_string()));
    }

    #[test]
    fn test_perspective_params_without_power_matrix() {
        let params = PerspectiveParams::new("Topic").without_power_matrix();
        assert!(!params.include_power_matrix);
    }

    #[test]
    fn test_perspective_params_serialize() {
        let params = PerspectiveParams::new("Policy change")
            .with_session("s-1")
            .with_stakeholder("CEO");

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Policy change"));
        assert!(json.contains("s-1"));
        assert!(json.contains("CEO"));
    }

    #[test]
    fn test_perspective_params_deserialize() {
        let json = r#"{
            "topic": "Expansion",
            "stakeholders": [{"name": "Team", "role": "Engineering"}],
            "include_power_matrix": false
        }"#;
        let params: PerspectiveParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.topic, "Expansion");
        assert_eq!(params.stakeholders.len(), 1);
        assert!(!params.include_power_matrix);
    }

    // ========================================================================
    // Result Type Tests
    // ========================================================================

    #[test]
    fn test_quadrant_display() {
        assert_eq!(format!("{}", Quadrant::KeyPlayer), "key_player");
        assert_eq!(format!("{}", Quadrant::KeepSatisfied), "keep_satisfied");
        assert_eq!(format!("{}", Quadrant::KeepInformed), "keep_informed");
        assert_eq!(format!("{}", Quadrant::MinimalEffort), "minimal_effort");
    }

    #[test]
    fn test_quadrant_serialize() {
        let json = serde_json::to_string(&Quadrant::KeyPlayer).unwrap();
        assert_eq!(json, "\"key_player\"");
    }

    #[test]
    fn test_quadrant_deserialize() {
        let q: Quadrant = serde_json::from_str("\"keep_informed\"").unwrap();
        assert_eq!(q, Quadrant::KeepInformed);
    }

    #[test]
    fn test_recommendation_serialize() {
        let rec = Recommendation {
            option: "Option A".to_string(),
            score: 0.85,
            confidence: 0.9,
            rationale: "Best overall".to_string(),
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert!(json.contains("Option A"));
        assert!(json.contains("0.85"));
    }

    #[test]
    fn test_sensitivity_analysis_serialize() {
        let sa = SensitivityAnalysis {
            robust: true,
            critical_criteria: vec!["cost".to_string()],
            threshold_changes: HashMap::from([("cost".to_string(), 0.15)]),
        };
        let json = serde_json::to_string(&sa).unwrap();
        assert!(json.contains("\"robust\":true"));
        assert!(json.contains("cost"));
    }

    #[test]
    fn test_trade_off_serialize() {
        let trade_off = TradeOff {
            between: ("A".to_string(), "B".to_string()),
            trade_off: "A is faster, B is cheaper".to_string(),
        };
        let json = serde_json::to_string(&trade_off).unwrap();
        assert!(json.contains("faster"));
    }

    #[test]
    fn test_conflict_serialize() {
        let conflict = Conflict {
            stakeholders: ("Alice".to_string(), "Bob".to_string()),
            issue: "Budget allocation".to_string(),
            severity: 0.7,
            resolution_approach: "Mediation".to_string(),
        };
        let json = serde_json::to_string(&conflict).unwrap();
        assert!(json.contains("Alice"));
        assert!(json.contains("Budget allocation"));
    }

    #[test]
    fn test_alignment_serialize() {
        let alignment = Alignment {
            stakeholders: ("Alice".to_string(), "Carol".to_string()),
            shared_interest: "Quality".to_string(),
        };
        let json = serde_json::to_string(&alignment).unwrap();
        assert!(json.contains("Carol"));
        assert!(json.contains("Quality"));
    }

    #[test]
    fn test_synthesis_serialize() {
        let synthesis = Synthesis {
            consensus_areas: vec!["Growth".to_string()],
            contentious_areas: vec!["Timeline".to_string()],
            recommendation: "Proceed cautiously".to_string(),
        };
        let json = serde_json::to_string(&synthesis).unwrap();
        assert!(json.contains("Growth"));
        assert!(json.contains("Timeline"));
        assert!(json.contains("Proceed cautiously"));
    }

    #[test]
    fn test_power_matrix_serialize() {
        let pm = PowerMatrix {
            key_players: vec!["CEO".to_string()],
            keep_satisfied: vec!["Board".to_string()],
            keep_informed: vec!["Team".to_string()],
            minimal_effort: vec!["Vendor".to_string()],
        };
        let json = serde_json::to_string(&pm).unwrap();
        assert!(json.contains("CEO"));
        assert!(json.contains("Board"));
    }

    #[test]
    fn test_stakeholder_analysis_serialize() {
        let sa = StakeholderAnalysis {
            name: "John".to_string(),
            role: "Manager".to_string(),
            perspective: "Focus on efficiency".to_string(),
            interests: vec!["Cost reduction".to_string()],
            concerns: vec!["Timeline".to_string()],
            power_level: 0.8,
            interest_level: 0.9,
            quadrant: Quadrant::KeyPlayer,
            engagement_strategy: "Regular updates".to_string(),
        };
        let json = serde_json::to_string(&sa).unwrap();
        assert!(json.contains("John"));
        assert!(json.contains("key_player"));
    }
}
