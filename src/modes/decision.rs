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
use tracing::{debug, error, info, warn};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::{DECISION_MAKER_PROMPT, PERSPECTIVE_ANALYZER_PROMPT};
use crate::storage::{
    Decision as StoredDecision, Invocation, PerspectiveAnalysis as StoredPerspective,
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
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// The Langbase pipe name for decision analysis.
    decision_pipe: String,
    /// The Langbase pipe name for perspective analysis.
    perspective_pipe: String,
}

impl DecisionMode {
    /// Create a new decision mode handler.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
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
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "decision")
            .await?;
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

        self.core
            .storage()
            .create_decision(&stored_decision)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    decision_id = %decision_id,
                    "Failed to persist decision - operation failed"
                );
                e
            })?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.make_decision output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

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
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "decision")
            .await?;
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

        self.core
            .storage()
            .create_perspective(&stored_perspective)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    analysis_id = %analysis_id,
                    "Failed to persist perspective analysis - operation failed"
                );
                e
            })?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.analyze_perspectives output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

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
                    s.role
                        .as_ref()
                        .map(|r| format!(" ({})", r))
                        .unwrap_or_default(),
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

    // ========================================================================
    // Additional DecisionParams Coverage
    // ========================================================================

    #[test]
    fn test_decision_params_full_builder() {
        let params = DecisionParams::new("Q", vec!["A".to_string(), "B".to_string()])
            .with_session("s-1")
            .with_criterion("cost", 0.3)
            .with_criterion("quality", 0.4)
            .with_criterion("speed", 0.3)
            .with_constraint("Under budget")
            .with_constraint("Meet deadline")
            .with_method(DecisionMethod::Pairwise);

        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.criteria.len(), 3);
        assert_eq!(params.constraints.len(), 2);
        assert_eq!(params.method, DecisionMethod::Pairwise);
    }

    #[test]
    fn test_decision_params_deserialize_all_fields() {
        let json = r#"{
            "question": "Full test",
            "options": ["A", "B", "C"],
            "criteria": [
                {"name": "cost", "weight": 0.5, "description": "Total cost"},
                {"name": "quality", "weight": 0.5}
            ],
            "constraints": ["Budget < 1000", "Time < 30 days"],
            "session_id": "sess-xyz",
            "method": "pairwise"
        }"#;
        let params: DecisionParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.question, "Full test");
        assert_eq!(params.options.len(), 3);
        assert_eq!(params.criteria.len(), 2);
        assert_eq!(
            params.criteria[0].description,
            Some("Total cost".to_string())
        );
        assert!(params.criteria[1].description.is_none());
        assert_eq!(params.constraints.len(), 2);
        assert_eq!(params.session_id, Some("sess-xyz".to_string()));
        assert_eq!(params.method, DecisionMethod::Pairwise);
    }

    #[test]
    fn test_decision_params_deserialize_empty_arrays() {
        let json = r#"{"question": "Q", "options": ["A", "B"], "criteria": [], "constraints": []}"#;
        let params: DecisionParams = serde_json::from_str(json).unwrap();

        assert!(params.criteria.is_empty());
        assert!(params.constraints.is_empty());
    }

    #[test]
    fn test_decision_params_deserialize_no_session() {
        let json = r#"{"question": "Q", "options": ["A", "B"]}"#;
        let params: DecisionParams = serde_json::from_str(json).unwrap();
        assert!(params.session_id.is_none());
    }

    // ========================================================================
    // Criterion Tests
    // ========================================================================

    #[test]
    fn test_criterion_serialize() {
        let criterion = Criterion {
            name: "cost".to_string(),
            weight: 0.6,
            description: Some("Total project cost".to_string()),
        };
        let json = serde_json::to_string(&criterion).unwrap();
        assert!(json.contains("cost"));
        assert!(json.contains("0.6"));
        assert!(json.contains("Total project cost"));
    }

    #[test]
    fn test_criterion_deserialize() {
        let json = r#"{"name": "quality", "weight": 0.8, "description": "Code quality"}"#;
        let criterion: Criterion = serde_json::from_str(json).unwrap();
        assert_eq!(criterion.name, "quality");
        assert_eq!(criterion.weight, 0.8);
        assert_eq!(criterion.description, Some("Code quality".to_string()));
    }

    #[test]
    fn test_criterion_round_trip() {
        let original = Criterion {
            name: "speed".to_string(),
            weight: 0.75,
            description: Some("Execution speed".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Criterion = serde_json::from_str(&json).unwrap();

        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.weight, deserialized.weight);
        assert_eq!(original.description, deserialized.description);
    }

    #[test]
    fn test_criterion_without_description() {
        let criterion = Criterion {
            name: "size".to_string(),
            weight: 0.4,
            description: None,
        };
        let json = serde_json::to_string(&criterion).unwrap();
        // Should skip description field when None
        assert!(!json.contains("description"));

        let deserialized: Criterion = serde_json::from_str(&json).unwrap();
        assert!(deserialized.description.is_none());
    }

    // ========================================================================
    // DecisionMethod Tests
    // ========================================================================

    #[test]
    fn test_decision_method_default() {
        let method = DecisionMethod::default();
        assert_eq!(method, DecisionMethod::WeightedSum);
    }

    #[test]
    fn test_decision_method_serialize_all() {
        assert_eq!(
            serde_json::to_string(&DecisionMethod::WeightedSum).unwrap(),
            "\"weighted_sum\""
        );
        assert_eq!(
            serde_json::to_string(&DecisionMethod::Pairwise).unwrap(),
            "\"pairwise\""
        );
        assert_eq!(
            serde_json::to_string(&DecisionMethod::Topsis).unwrap(),
            "\"topsis\""
        );
    }

    #[test]
    fn test_decision_method_deserialize_all() {
        let ws: DecisionMethod = serde_json::from_str("\"weighted_sum\"").unwrap();
        assert_eq!(ws, DecisionMethod::WeightedSum);

        let pw: DecisionMethod = serde_json::from_str("\"pairwise\"").unwrap();
        assert_eq!(pw, DecisionMethod::Pairwise);

        let tp: DecisionMethod = serde_json::from_str("\"topsis\"").unwrap();
        assert_eq!(tp, DecisionMethod::Topsis);
    }

    // ========================================================================
    // PerspectiveParams Tests
    // ========================================================================

    #[test]
    fn test_perspective_params_with_session() {
        let params = PerspectiveParams::new("Topic").with_session("sess-456");
        assert_eq!(params.session_id, Some("sess-456".to_string()));
    }

    #[test]
    fn test_perspective_params_full_builder() {
        let params = PerspectiveParams::new("Policy")
            .with_session("s-1")
            .with_stakeholder("Alice")
            .with_stakeholder("Bob")
            .with_context("Important context")
            .without_power_matrix();

        assert_eq!(params.session_id, Some("s-1".to_string()));
        assert_eq!(params.stakeholders.len(), 2);
        assert_eq!(params.context, Some("Important context".to_string()));
        assert!(!params.include_power_matrix);
    }

    #[test]
    fn test_perspective_params_deserialize_minimal() {
        let json = r#"{"topic": "Simple"}"#;
        let params: PerspectiveParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.topic, "Simple");
        assert!(params.stakeholders.is_empty());
        assert!(params.context.is_none());
        assert!(params.session_id.is_none());
        assert!(params.include_power_matrix); // default_true
    }

    #[test]
    fn test_perspective_params_deserialize_full() {
        let json = r#"{
            "topic": "Migration",
            "stakeholders": [
                {"name": "Dev", "role": "Engineering", "interests": ["Performance"]},
                {"name": "PM"}
            ],
            "context": "Cloud migration",
            "session_id": "s-99",
            "include_power_matrix": false
        }"#;
        let params: PerspectiveParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.topic, "Migration");
        assert_eq!(params.stakeholders.len(), 2);
        assert_eq!(params.stakeholders[0].role, Some("Engineering".to_string()));
        assert_eq!(params.stakeholders[0].interests.len(), 1);
        assert!(params.stakeholders[1].role.is_none());
        assert_eq!(params.context, Some("Cloud migration".to_string()));
        assert_eq!(params.session_id, Some("s-99".to_string()));
        assert!(!params.include_power_matrix);
    }

    // ========================================================================
    // StakeholderInput Tests
    // ========================================================================

    #[test]
    fn test_stakeholder_input_serialize_minimal() {
        let stakeholder = StakeholderInput {
            name: "Alice".to_string(),
            role: None,
            interests: Vec::new(),
        };
        let json = serde_json::to_string(&stakeholder).unwrap();
        assert!(json.contains("Alice"));
        assert!(!json.contains("role"));
    }

    #[test]
    fn test_stakeholder_input_serialize_full() {
        let stakeholder = StakeholderInput {
            name: "Bob".to_string(),
            role: Some("CTO".to_string()),
            interests: vec!["Security".to_string(), "Performance".to_string()],
        };
        let json = serde_json::to_string(&stakeholder).unwrap();
        assert!(json.contains("Bob"));
        assert!(json.contains("CTO"));
        assert!(json.contains("Security"));
        assert!(json.contains("Performance"));
    }

    #[test]
    fn test_stakeholder_input_deserialize() {
        let json = r#"{"name": "Carol", "role": "PM", "interests": ["Timeline"]}"#;
        let stakeholder: StakeholderInput = serde_json::from_str(json).unwrap();

        assert_eq!(stakeholder.name, "Carol");
        assert_eq!(stakeholder.role, Some("PM".to_string()));
        assert_eq!(stakeholder.interests.len(), 1);
        assert_eq!(stakeholder.interests[0], "Timeline");
    }

    // ========================================================================
    // Helper Functions
    // ========================================================================

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }

    // ========================================================================
    // Response Deserialization Tests
    // ========================================================================

    #[test]
    fn test_criterion_score_deserialize() {
        let json = r#"{"score": 0.85, "reasoning": "High quality output"}"#;
        let score: CriterionScore = serde_json::from_str(json).unwrap();
        assert_eq!(score.score, 0.85);
        assert_eq!(score.reasoning, "High quality output");
    }

    #[test]
    fn test_option_score_deserialize() {
        let json = r#"{
            "option": "Option A",
            "total_score": 0.78,
            "criteria_scores": {
                "cost": {"score": 0.9, "reasoning": "Low cost"}
            },
            "rank": 1
        }"#;
        let score: OptionScore = serde_json::from_str(json).unwrap();
        assert_eq!(score.option, "Option A");
        assert_eq!(score.total_score, 0.78);
        assert_eq!(score.rank, 1);
        assert!(score.criteria_scores.contains_key("cost"));
    }

    #[test]
    fn test_recommendation_deserialize() {
        let json = r#"{
            "option": "Best choice",
            "score": 0.92,
            "confidence": 0.88,
            "rationale": "Optimal across all criteria"
        }"#;
        let rec: Recommendation = serde_json::from_str(json).unwrap();
        assert_eq!(rec.option, "Best choice");
        assert_eq!(rec.score, 0.92);
        assert_eq!(rec.confidence, 0.88);
    }

    #[test]
    fn test_sensitivity_analysis_deserialize() {
        let json = r#"{
            "robust": false,
            "critical_criteria": ["cost", "quality"],
            "threshold_changes": {"cost": 0.1, "quality": 0.15}
        }"#;
        let sa: SensitivityAnalysis = serde_json::from_str(json).unwrap();
        assert!(!sa.robust);
        assert_eq!(sa.critical_criteria.len(), 2);
        assert_eq!(sa.threshold_changes.get("cost"), Some(&0.1));
    }

    // ========================================================================
    // TradeOff Tests
    // ========================================================================

    #[test]
    fn test_trade_off_deserialize() {
        let json = r#"{
            "between": ["Option A", "Option B"],
            "trade_off": "A is faster but B is cheaper"
        }"#;
        let to: TradeOff = serde_json::from_str(json).unwrap();
        assert_eq!(to.between.0, "Option A");
        assert_eq!(to.between.1, "Option B");
        assert!(to.trade_off.contains("faster"));
    }

    #[test]
    fn test_trade_off_round_trip() {
        let original = TradeOff {
            between: ("X".to_string(), "Y".to_string()),
            trade_off: "Different tradeoff".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TradeOff = serde_json::from_str(&json).unwrap();
        assert_eq!(original.between.0, deserialized.between.0);
        assert_eq!(original.between.1, deserialized.between.1);
        assert_eq!(original.trade_off, deserialized.trade_off);
    }

    // ========================================================================
    // Conflict Tests
    // ========================================================================

    #[test]
    fn test_conflict_deserialize() {
        let json = r#"{
            "stakeholders": ["Alice", "Bob"],
            "issue": "Resource allocation",
            "severity": 0.65,
            "resolution_approach": "Negotiate priorities"
        }"#;
        let conflict: Conflict = serde_json::from_str(json).unwrap();
        assert_eq!(conflict.stakeholders.0, "Alice");
        assert_eq!(conflict.stakeholders.1, "Bob");
        assert_eq!(conflict.severity, 0.65);
    }

    #[test]
    fn test_conflict_round_trip() {
        let original = Conflict {
            stakeholders: ("Dev".to_string(), "PM".to_string()),
            issue: "Timeline disagreement".to_string(),
            severity: 0.8,
            resolution_approach: "Escalate to CTO".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Conflict = serde_json::from_str(&json).unwrap();
        assert_eq!(original.stakeholders, deserialized.stakeholders);
        assert_eq!(original.issue, deserialized.issue);
        assert_eq!(original.severity, deserialized.severity);
        assert_eq!(
            original.resolution_approach,
            deserialized.resolution_approach
        );
    }

    // ========================================================================
    // Alignment Tests
    // ========================================================================

    #[test]
    fn test_alignment_deserialize() {
        let json = r#"{
            "stakeholders": ["Alice", "Carol"],
            "shared_interest": "Product quality"
        }"#;
        let alignment: Alignment = serde_json::from_str(json).unwrap();
        assert_eq!(alignment.stakeholders.0, "Alice");
        assert_eq!(alignment.stakeholders.1, "Carol");
        assert_eq!(alignment.shared_interest, "Product quality");
    }

    #[test]
    fn test_alignment_round_trip() {
        let original = Alignment {
            stakeholders: ("Team A".to_string(), "Team B".to_string()),
            shared_interest: "Security".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Alignment = serde_json::from_str(&json).unwrap();
        assert_eq!(original.stakeholders, deserialized.stakeholders);
        assert_eq!(original.shared_interest, deserialized.shared_interest);
    }

    // ========================================================================
    // Synthesis Tests
    // ========================================================================

    #[test]
    fn test_synthesis_deserialize() {
        let json = r#"{
            "consensus_areas": ["Quality", "Security"],
            "contentious_areas": ["Timeline", "Budget"],
            "recommendation": "Prioritize quality first"
        }"#;
        let synthesis: Synthesis = serde_json::from_str(json).unwrap();
        assert_eq!(synthesis.consensus_areas.len(), 2);
        assert_eq!(synthesis.contentious_areas.len(), 2);
        assert!(synthesis.recommendation.contains("quality"));
    }

    #[test]
    fn test_synthesis_round_trip() {
        let original = Synthesis {
            consensus_areas: vec!["A".to_string(), "B".to_string()],
            contentious_areas: vec!["C".to_string()],
            recommendation: "Proceed with caution".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Synthesis = serde_json::from_str(&json).unwrap();
        assert_eq!(original.consensus_areas, deserialized.consensus_areas);
        assert_eq!(original.contentious_areas, deserialized.contentious_areas);
        assert_eq!(original.recommendation, deserialized.recommendation);
    }

    // ========================================================================
    // PowerMatrix Tests
    // ========================================================================

    #[test]
    fn test_power_matrix_deserialize() {
        let json = r#"{
            "key_players": ["CEO", "CTO"],
            "keep_satisfied": ["Board"],
            "keep_informed": ["Team"],
            "minimal_effort": ["Vendor"]
        }"#;
        let pm: PowerMatrix = serde_json::from_str(json).unwrap();
        assert_eq!(pm.key_players.len(), 2);
        assert_eq!(pm.keep_satisfied.len(), 1);
        assert_eq!(pm.keep_informed.len(), 1);
        assert_eq!(pm.minimal_effort.len(), 1);
    }

    #[test]
    fn test_power_matrix_round_trip() {
        let original = PowerMatrix {
            key_players: vec!["A".to_string()],
            keep_satisfied: vec!["B".to_string(), "C".to_string()],
            keep_informed: vec![],
            minimal_effort: vec!["D".to_string()],
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PowerMatrix = serde_json::from_str(&json).unwrap();
        assert_eq!(original.key_players, deserialized.key_players);
        assert_eq!(original.keep_satisfied, deserialized.keep_satisfied);
        assert_eq!(original.keep_informed, deserialized.keep_informed);
        assert_eq!(original.minimal_effort, deserialized.minimal_effort);
    }

    #[test]
    fn test_power_matrix_empty_quadrants() {
        let pm = PowerMatrix {
            key_players: vec![],
            keep_satisfied: vec![],
            keep_informed: vec![],
            minimal_effort: vec![],
        };
        let json = serde_json::to_string(&pm).unwrap();
        let deserialized: PowerMatrix = serde_json::from_str(&json).unwrap();
        assert!(deserialized.key_players.is_empty());
        assert!(deserialized.keep_satisfied.is_empty());
    }

    // ========================================================================
    // StakeholderAnalysis Tests
    // ========================================================================

    #[test]
    fn test_stakeholder_analysis_deserialize() {
        let json = r#"{
            "name": "John",
            "role": "Manager",
            "perspective": "Focus on ROI",
            "interests": ["Cost reduction"],
            "concerns": ["Risk"],
            "power_level": 0.7,
            "interest_level": 0.8,
            "quadrant": "key_player",
            "engagement_strategy": "Weekly updates"
        }"#;
        let sa: StakeholderAnalysis = serde_json::from_str(json).unwrap();
        assert_eq!(sa.name, "John");
        assert_eq!(sa.role, "Manager");
        assert_eq!(sa.power_level, 0.7);
        assert_eq!(sa.quadrant, Quadrant::KeyPlayer);
    }

    #[test]
    fn test_stakeholder_analysis_round_trip() {
        let original = StakeholderAnalysis {
            name: "Sarah".to_string(),
            role: "Dev Lead".to_string(),
            perspective: "Tech debt concerns".to_string(),
            interests: vec!["Quality".to_string()],
            concerns: vec!["Timeline".to_string(), "Resources".to_string()],
            power_level: 0.6,
            interest_level: 0.9,
            quadrant: Quadrant::KeepInformed,
            engagement_strategy: "Monthly review".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: StakeholderAnalysis = serde_json::from_str(&json).unwrap();
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.quadrant, deserialized.quadrant);
        assert_eq!(original.power_level, deserialized.power_level);
    }

    // ========================================================================
    // DecisionResult Tests
    // ========================================================================

    #[test]
    fn test_decision_result_serialize() {
        let result = DecisionResult {
            decision_id: "dec-1".to_string(),
            session_id: "sess-1".to_string(),
            question: "Which option?".to_string(),
            recommendation: Recommendation {
                option: "A".to_string(),
                score: 0.9,
                confidence: 0.85,
                rationale: "Best choice".to_string(),
            },
            scores: vec![],
            sensitivity_analysis: SensitivityAnalysis {
                robust: true,
                critical_criteria: vec![],
                threshold_changes: HashMap::new(),
            },
            trade_offs: vec![],
            constraints_satisfied: HashMap::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("dec-1"));
        assert!(json.contains("Which option?"));
    }

    #[test]
    fn test_decision_result_deserialize() {
        let json = r#"{
            "decision_id": "d-123",
            "session_id": "s-456",
            "question": "Test?",
            "recommendation": {
                "option": "B",
                "score": 0.8,
                "confidence": 0.7,
                "rationale": "Good"
            },
            "scores": [],
            "sensitivity_analysis": {
                "robust": false,
                "critical_criteria": [],
                "threshold_changes": {}
            },
            "trade_offs": [],
            "constraints_satisfied": {}
        }"#;
        let result: DecisionResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.decision_id, "d-123");
        assert_eq!(result.session_id, "s-456");
        assert_eq!(result.recommendation.option, "B");
    }

    // ========================================================================
    // PerspectiveResult Tests
    // ========================================================================

    #[test]
    fn test_perspective_result_serialize() {
        let result = PerspectiveResult {
            analysis_id: "ana-1".to_string(),
            session_id: "sess-1".to_string(),
            topic: "Topic".to_string(),
            stakeholders: vec![],
            power_matrix: None,
            conflicts: vec![],
            alignments: vec![],
            synthesis: Synthesis {
                consensus_areas: vec![],
                contentious_areas: vec![],
                recommendation: "Proceed".to_string(),
            },
            confidence: 0.75,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("ana-1"));
        assert!(json.contains("Topic"));
        assert!(json.contains("0.75"));
    }

    #[test]
    fn test_perspective_result_deserialize() {
        let json = r#"{
            "analysis_id": "a-789",
            "session_id": "s-101",
            "topic": "Migration",
            "stakeholders": [],
            "conflicts": [],
            "alignments": [],
            "synthesis": {
                "consensus_areas": [],
                "contentious_areas": [],
                "recommendation": "Wait"
            },
            "confidence": 0.6
        }"#;
        let result: PerspectiveResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.analysis_id, "a-789");
        assert_eq!(result.topic, "Migration");
        assert_eq!(result.confidence, 0.6);
        assert!(result.power_matrix.is_none());
    }

    #[test]
    fn test_perspective_result_with_power_matrix() {
        let result = PerspectiveResult {
            analysis_id: "a-1".to_string(),
            session_id: "s-1".to_string(),
            topic: "T".to_string(),
            stakeholders: vec![],
            power_matrix: Some(PowerMatrix {
                key_players: vec!["CEO".to_string()],
                keep_satisfied: vec![],
                keep_informed: vec![],
                minimal_effort: vec![],
            }),
            conflicts: vec![],
            alignments: vec![],
            synthesis: Synthesis {
                consensus_areas: vec![],
                contentious_areas: vec![],
                recommendation: "R".to_string(),
            },
            confidence: 0.8,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PerspectiveResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.power_matrix.is_some());
        assert_eq!(deserialized.power_matrix.unwrap().key_players.len(), 1);
    }

    // ========================================================================
    // Edge Cases and Unicode
    // ========================================================================

    #[test]
    fn test_decision_params_unicode() {
        let params = DecisionParams::new(
            "选择哪个选项？",
            vec!["选项 A".to_string(), "选项 B".to_string()],
        )
        .with_criterion("成本", 0.5);

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: DecisionParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.question, "选择哪个选项？");
        assert_eq!(deserialized.options[0], "选项 A");
        assert_eq!(deserialized.criteria[0].name, "成本");
    }

    #[test]
    fn test_perspective_params_unicode() {
        let params = PerspectiveParams::new("政策变更")
            .with_stakeholder("张三")
            .with_context("重要背景");

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: PerspectiveParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.topic, "政策变更");
        assert_eq!(deserialized.stakeholders[0].name, "张三");
        assert_eq!(deserialized.context, Some("重要背景".to_string()));
    }

    #[test]
    fn test_criterion_extreme_weights() {
        let zero_weight = Criterion {
            name: "zero".to_string(),
            weight: 0.0,
            description: None,
        };
        let full_weight = Criterion {
            name: "full".to_string(),
            weight: 1.0,
            description: None,
        };

        let json_zero = serde_json::to_string(&zero_weight).unwrap();
        let json_full = serde_json::to_string(&full_weight).unwrap();

        let deserialized_zero: Criterion = serde_json::from_str(&json_zero).unwrap();
        let deserialized_full: Criterion = serde_json::from_str(&json_full).unwrap();

        assert_eq!(deserialized_zero.weight, 0.0);
        assert_eq!(deserialized_full.weight, 1.0);
    }

    #[test]
    fn test_decision_params_empty_options_deserialization() {
        let json = r#"{"question": "Q", "options": []}"#;
        let params: DecisionParams = serde_json::from_str(json).unwrap();
        assert!(params.options.is_empty());
    }

    #[test]
    fn test_stakeholder_analysis_extreme_power_interest() {
        let sa = StakeholderAnalysis {
            name: "Test".to_string(),
            role: "R".to_string(),
            perspective: "P".to_string(),
            interests: vec![],
            concerns: vec![],
            power_level: 0.0,
            interest_level: 1.0,
            quadrant: Quadrant::KeepInformed,
            engagement_strategy: "S".to_string(),
        };

        let json = serde_json::to_string(&sa).unwrap();
        let deserialized: StakeholderAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.power_level, 0.0);
        assert_eq!(deserialized.interest_level, 1.0);
    }

    #[test]
    fn test_decision_result_empty_collections() {
        let result = DecisionResult {
            decision_id: "d-1".to_string(),
            session_id: "s-1".to_string(),
            question: "Q".to_string(),
            recommendation: Recommendation {
                option: "A".to_string(),
                score: 0.5,
                confidence: 0.5,
                rationale: "R".to_string(),
            },
            scores: vec![],
            sensitivity_analysis: SensitivityAnalysis {
                robust: true,
                critical_criteria: vec![],
                threshold_changes: HashMap::new(),
            },
            trade_offs: vec![],
            constraints_satisfied: HashMap::new(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: DecisionResult = serde_json::from_str(&json).unwrap();

        assert!(deserialized.scores.is_empty());
        assert!(deserialized.trade_offs.is_empty());
        assert!(deserialized.constraints_satisfied.is_empty());
    }

    #[test]
    fn test_perspective_result_empty_collections() {
        let result = PerspectiveResult {
            analysis_id: "a-1".to_string(),
            session_id: "s-1".to_string(),
            topic: "T".to_string(),
            stakeholders: vec![],
            power_matrix: None,
            conflicts: vec![],
            alignments: vec![],
            synthesis: Synthesis {
                consensus_areas: vec![],
                contentious_areas: vec![],
                recommendation: "R".to_string(),
            },
            confidence: 0.5,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PerspectiveResult = serde_json::from_str(&json).unwrap();

        assert!(deserialized.stakeholders.is_empty());
        assert!(deserialized.conflicts.is_empty());
        assert!(deserialized.alignments.is_empty());
    }

    // ========================================================================
    // CriterionScore Tests
    // ========================================================================

    #[test]
    fn test_criterion_score_serialize() {
        let score = CriterionScore {
            score: 0.95,
            reasoning: "Excellent performance".to_string(),
        };
        let json = serde_json::to_string(&score).unwrap();
        assert!(json.contains("0.95"));
        assert!(json.contains("Excellent performance"));
    }

    #[test]
    fn test_criterion_score_round_trip() {
        let original = CriterionScore {
            score: 0.33,
            reasoning: "Below average".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CriterionScore = serde_json::from_str(&json).unwrap();
        assert_eq!(original.score, deserialized.score);
        assert_eq!(original.reasoning, deserialized.reasoning);
    }

    // ========================================================================
    // OptionScore Tests
    // ========================================================================

    #[test]
    fn test_option_score_serialize() {
        let mut criteria_scores = HashMap::new();
        criteria_scores.insert(
            "cost".to_string(),
            CriterionScore {
                score: 0.8,
                reasoning: "Good".to_string(),
            },
        );

        let score = OptionScore {
            option: "Option X".to_string(),
            total_score: 0.82,
            criteria_scores,
            rank: 2,
        };

        let json = serde_json::to_string(&score).unwrap();
        assert!(json.contains("Option X"));
        assert!(json.contains("0.82"));
    }

    #[test]
    fn test_option_score_round_trip() {
        let mut criteria_scores = HashMap::new();
        criteria_scores.insert(
            "quality".to_string(),
            CriterionScore {
                score: 0.9,
                reasoning: "High quality".to_string(),
            },
        );

        let original = OptionScore {
            option: "Best".to_string(),
            total_score: 0.88,
            criteria_scores,
            rank: 1,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: OptionScore = serde_json::from_str(&json).unwrap();

        assert_eq!(original.option, deserialized.option);
        assert_eq!(original.total_score, deserialized.total_score);
        assert_eq!(original.rank, deserialized.rank);
        assert_eq!(
            original.criteria_scores.len(),
            deserialized.criteria_scores.len()
        );
    }

    // ========================================================================
    // Response Type Tests (for parsing)
    // ========================================================================

    #[test]
    fn test_trade_off_response_deserialize() {
        let json = r#"{
            "between": ["A", "B"],
            "trade_off": "Speed vs cost"
        }"#;
        let tor: TradeOffResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tor.between.len(), 2);
        assert_eq!(tor.trade_off, "Speed vs cost");
    }

    #[test]
    fn test_conflict_response_deserialize() {
        let json = r#"{
            "stakeholders": ["Alice", "Bob"],
            "issue": "Priority conflict",
            "severity": 0.8,
            "resolution_approach": "Mediation"
        }"#;
        let cr: ConflictResponse = serde_json::from_str(json).unwrap();
        assert_eq!(cr.stakeholders.len(), 2);
        assert_eq!(cr.severity, 0.8);
    }

    #[test]
    fn test_alignment_response_deserialize() {
        let json = r#"{
            "stakeholders": ["Carol", "Dave"],
            "shared_interest": "Innovation"
        }"#;
        let ar: AlignmentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(ar.stakeholders.len(), 2);
        assert_eq!(ar.shared_interest, "Innovation");
    }

    // ========================================================================
    // Quadrant Comprehensive Tests
    // ========================================================================

    #[test]
    fn test_quadrant_equality() {
        assert_eq!(Quadrant::KeyPlayer, Quadrant::KeyPlayer);
        assert_ne!(Quadrant::KeyPlayer, Quadrant::KeepSatisfied);
    }

    #[test]
    fn test_quadrant_clone() {
        let q1 = Quadrant::KeyPlayer;
        let q2 = q1.clone();
        assert_eq!(q1, q2);
    }

    #[test]
    fn test_quadrant_all_variants_display() {
        let variants = vec![
            (Quadrant::KeyPlayer, "key_player"),
            (Quadrant::KeepSatisfied, "keep_satisfied"),
            (Quadrant::KeepInformed, "keep_informed"),
            (Quadrant::MinimalEffort, "minimal_effort"),
        ];

        for (variant, expected) in variants {
            assert_eq!(format!("{}", variant), expected);
        }
    }

    #[test]
    fn test_quadrant_all_variants_serialize() {
        let variants = vec![
            Quadrant::KeyPlayer,
            Quadrant::KeepSatisfied,
            Quadrant::KeepInformed,
            Quadrant::MinimalEffort,
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: Quadrant = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    // ========================================================================
    // DecisionMethod Comprehensive Tests
    // ========================================================================

    #[test]
    fn test_decision_method_equality() {
        assert_eq!(DecisionMethod::WeightedSum, DecisionMethod::WeightedSum);
        assert_ne!(DecisionMethod::WeightedSum, DecisionMethod::Pairwise);
    }

    #[test]
    fn test_decision_method_clone() {
        let m1 = DecisionMethod::Topsis;
        let m2 = m1.clone();
        assert_eq!(m1, m2);
    }

    #[test]
    fn test_decision_method_copy() {
        let m1 = DecisionMethod::Pairwise;
        let m2 = m1; // Copy trait
        assert_eq!(m1, m2);
    }

    // ========================================================================
    // Complex Nested Structures
    // ========================================================================

    #[test]
    fn test_decision_result_with_full_data() {
        let mut criteria_scores = HashMap::new();
        criteria_scores.insert(
            "cost".to_string(),
            CriterionScore {
                score: 0.9,
                reasoning: "Low cost".to_string(),
            },
        );

        let mut threshold_changes = HashMap::new();
        threshold_changes.insert("cost".to_string(), 0.15);

        let mut constraints = HashMap::new();
        constraints.insert("Option A".to_string(), true);

        let result = DecisionResult {
            decision_id: "d-complex".to_string(),
            session_id: "s-complex".to_string(),
            question: "Complex decision?".to_string(),
            recommendation: Recommendation {
                option: "Option A".to_string(),
                score: 0.92,
                confidence: 0.88,
                rationale: "Best overall".to_string(),
            },
            scores: vec![OptionScore {
                option: "Option A".to_string(),
                total_score: 0.92,
                criteria_scores: criteria_scores.clone(),
                rank: 1,
            }],
            sensitivity_analysis: SensitivityAnalysis {
                robust: true,
                critical_criteria: vec!["cost".to_string()],
                threshold_changes,
            },
            trade_offs: vec![TradeOff {
                between: ("Option A".to_string(), "Option B".to_string()),
                trade_off: "A is better".to_string(),
            }],
            constraints_satisfied: constraints,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: DecisionResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.decision_id, deserialized.decision_id);
        assert_eq!(result.scores.len(), deserialized.scores.len());
        assert_eq!(result.trade_offs.len(), deserialized.trade_offs.len());
    }

    #[test]
    fn test_perspective_result_with_full_data() {
        let result = PerspectiveResult {
            analysis_id: "a-complex".to_string(),
            session_id: "s-complex".to_string(),
            topic: "Complex topic".to_string(),
            stakeholders: vec![StakeholderAnalysis {
                name: "Alice".to_string(),
                role: "CEO".to_string(),
                perspective: "Strategic".to_string(),
                interests: vec!["Growth".to_string()],
                concerns: vec!["Risk".to_string()],
                power_level: 1.0,
                interest_level: 0.9,
                quadrant: Quadrant::KeyPlayer,
                engagement_strategy: "Direct".to_string(),
            }],
            power_matrix: Some(PowerMatrix {
                key_players: vec!["Alice".to_string()],
                keep_satisfied: vec![],
                keep_informed: vec![],
                minimal_effort: vec![],
            }),
            conflicts: vec![Conflict {
                stakeholders: ("Alice".to_string(), "Bob".to_string()),
                issue: "Priority".to_string(),
                severity: 0.6,
                resolution_approach: "Negotiate".to_string(),
            }],
            alignments: vec![Alignment {
                stakeholders: ("Alice".to_string(), "Carol".to_string()),
                shared_interest: "Quality".to_string(),
            }],
            synthesis: Synthesis {
                consensus_areas: vec!["Quality".to_string()],
                contentious_areas: vec!["Timeline".to_string()],
                recommendation: "Proceed".to_string(),
            },
            confidence: 0.85,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PerspectiveResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.analysis_id, deserialized.analysis_id);
        assert_eq!(result.stakeholders.len(), deserialized.stakeholders.len());
        assert_eq!(result.conflicts.len(), deserialized.conflicts.len());
        assert_eq!(result.alignments.len(), deserialized.alignments.len());
    }

    // ========================================================================
    // StakeholderInput Edge Cases
    // ========================================================================

    #[test]
    fn test_stakeholder_input_empty_interests() {
        let stakeholder = StakeholderInput {
            name: "Test".to_string(),
            role: Some("Role".to_string()),
            interests: vec![],
        };

        let json = serde_json::to_string(&stakeholder).unwrap();
        let deserialized: StakeholderInput = serde_json::from_str(&json).unwrap();

        assert!(deserialized.interests.is_empty());
    }

    #[test]
    fn test_stakeholder_input_round_trip() {
        let original = StakeholderInput {
            name: "Full Test".to_string(),
            role: Some("Engineer".to_string()),
            interests: vec!["Tech".to_string(), "Innovation".to_string()],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: StakeholderInput = serde_json::from_str(&json).unwrap();

        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.role, deserialized.role);
        assert_eq!(original.interests, deserialized.interests);
    }
}
