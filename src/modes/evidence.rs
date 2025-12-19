//! Evidence assessment reasoning mode - evidence evaluation and Bayesian reasoning.
//!
//! This module provides evidence assessment capabilities:
//! - Evidence quality and credibility evaluation
//! - Source reliability analysis
//! - Corroboration tracking
//! - Bayesian probability updates
//! - Uncertainty quantification with entropy

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, error, info, warn};

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::prompts::{BAYESIAN_UPDATER_PROMPT, EVIDENCE_ASSESSOR_PROMPT};
use crate::storage::{
    EvidenceAssessment as StoredEvidence, Invocation, ProbabilityUpdate as StoredProbability,
    SqliteStorage, Storage,
};

// ============================================================================
// Evidence Assessment Parameters
// ============================================================================

/// Input parameters for evidence assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceParams {
    /// The claim to assess evidence for.
    pub claim: String,
    /// Evidence items to assess.
    pub evidence: Vec<EvidenceInput>,
    /// Optional session ID (creates new if not provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Additional context for the assessment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Evidence input item for assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceInput {
    /// Evidence content or description.
    pub content: String,
    /// Source of the evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Type of source (primary, secondary, anecdotal, expert, statistical).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,
    /// Date of evidence (ISO format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// Type of evidence source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Primary/direct source.
    Primary,
    /// Secondary/derived source.
    Secondary,
    /// Anecdotal evidence.
    Anecdotal,
    /// Expert opinion.
    Expert,
    /// Statistical data.
    Statistical,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Primary => write!(f, "primary"),
            SourceType::Secondary => write!(f, "secondary"),
            SourceType::Anecdotal => write!(f, "anecdotal"),
            SourceType::Expert => write!(f, "expert"),
            SourceType::Statistical => write!(f, "statistical"),
        }
    }
}

/// Input parameters for Bayesian probability updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilisticParams {
    /// The hypothesis to evaluate.
    pub hypothesis: String,
    /// Prior probability (0-1).
    pub prior: f64,
    /// Evidence items with likelihood ratios.
    pub evidence: Vec<BayesianEvidence>,
    /// Optional session ID for context persistence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Evidence item for Bayesian updating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BayesianEvidence {
    /// Evidence description.
    pub description: String,
    /// P(evidence|hypothesis true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likelihood_if_true: Option<f64>,
    /// P(evidence|hypothesis false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likelihood_if_false: Option<f64>,
}

// ============================================================================
// Langbase Response Types
// ============================================================================

/// Response from evidence assessor Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceResponse {
    overall_support: OverallSupport,
    evidence_analysis: Vec<EvidenceAnalysisItem>,
    #[serde(default)]
    chain_analysis: Option<ChainAnalysis>,
    #[serde(default)]
    contradictions: Vec<Contradiction>,
    #[serde(default)]
    gaps: Vec<EvidenceGap>,
    #[serde(default)]
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverallSupport {
    level: String,
    confidence: f64,
    explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceAnalysisItem {
    #[serde(default)]
    evidence_id: String,
    content_summary: String,
    relevance: RelevanceScore,
    credibility: CredibilityScore,
    weight: f64,
    supports_claim: bool,
    #[serde(default)]
    inferential_distance: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelevanceScore {
    score: f64,
    #[serde(default)]
    relevance_type: String,
    explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CredibilityScore {
    score: f64,
    #[serde(default)]
    factors: CredibilityFactors,
    #[serde(default)]
    concerns: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CredibilityFactors {
    #[serde(default)]
    source_reliability: f64,
    #[serde(default)]
    methodology: f64,
    #[serde(default)]
    recency: f64,
    #[serde(default)]
    corroboration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainAnalysis {
    #[serde(default)]
    primary_chain: Vec<String>,
    #[serde(default)]
    weak_links: Vec<WeakLink>,
    #[serde(default)]
    redundancy: Vec<String>,
    #[serde(default)]
    synergies: Vec<Synergy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WeakLink {
    from: String,
    to: String,
    weakness: String,
    impact: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Synergy {
    evidence_ids: Vec<String>,
    combined_strength: f64,
    explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Contradiction {
    evidence_a: String,
    evidence_b: String,
    nature: String,
    resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceGap {
    gap: String,
    importance: f64,
    suggested_evidence: String,
}

/// Response from Bayesian updater Langbase pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BayesianResponse {
    prior: f64,
    posterior: f64,
    #[serde(default)]
    confidence_interval: Option<ConfidenceInterval>,
    update_steps: Vec<UpdateStepResponse>,
    #[serde(default)]
    uncertainty_analysis: Option<UncertaintyAnalysis>,
    #[serde(default)]
    sensitivity: Option<SensitivityAnalysis>,
    interpretation: Interpretation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfidenceInterval {
    lower: f64,
    upper: f64,
    level: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateStepResponse {
    evidence: String,
    prior_before: f64,
    likelihood_ratio: f64,
    posterior_after: f64,
    #[serde(default)]
    explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UncertaintyAnalysis {
    entropy_before: f64,
    entropy_after: f64,
    information_gained: f64,
    remaining_uncertainty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SensitivityAnalysis {
    most_influential_evidence: String,
    robustness: f64,
    #[serde(default)]
    critical_assumptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Interpretation {
    verbal_probability: String,
    recommendation: String,
    #[serde(default)]
    caveats: Vec<String>,
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of evidence assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceResult {
    /// Unique assessment ID.
    pub assessment_id: String,
    /// Session ID.
    pub session_id: String,
    /// The claim being assessed.
    pub claim: String,
    /// Overall support level and confidence.
    pub overall_support: SupportLevel,
    /// Individual evidence analyses.
    pub evidence_analyses: Vec<EvidenceAnalysis>,
    /// Chain of reasoning analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_analysis: Option<InferentialChain>,
    /// Detected contradictions.
    pub contradictions: Vec<EvidenceContradiction>,
    /// Identified gaps.
    pub gaps: Vec<Gap>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Overall support level for a claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportLevel {
    /// Support level (strong, moderate, weak, insufficient, contradictory).
    pub level: String,
    /// Confidence in assessment (0.0-1.0).
    pub confidence: f64,
    /// Explanation of the assessment.
    pub explanation: String,
}

/// Analysis of a single piece of evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAnalysis {
    /// Evidence identifier.
    pub evidence_id: String,
    /// Summary of content.
    pub content_summary: String,
    /// Relevance to claim (0.0-1.0).
    pub relevance: f64,
    /// Source credibility (0.0-1.0).
    pub credibility: f64,
    /// Combined weight (0.0-1.0).
    pub weight: f64,
    /// Whether it supports the claim.
    pub supports_claim: bool,
    /// Assessment notes.
    pub notes: String,
}

/// Inferential chain from evidence to claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferentialChain {
    /// Primary reasoning chain.
    pub primary_chain: Vec<String>,
    /// Weak links in the chain.
    pub weak_links: Vec<ChainWeakness>,
    /// Evidence providing redundancy.
    pub redundant_evidence: Vec<String>,
}

/// Weakness in an inferential chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainWeakness {
    /// From node.
    pub from: String,
    /// To node.
    pub to: String,
    /// Description of weakness.
    pub weakness: String,
    /// Impact on conclusion (0.0-1.0).
    pub impact: f64,
}

/// Contradiction between evidence items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceContradiction {
    /// First evidence item.
    pub evidence_a: String,
    /// Second evidence item.
    pub evidence_b: String,
    /// Nature of contradiction.
    pub nature: String,
    /// Resolution approach.
    pub resolution: String,
}

/// Gap in evidence coverage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    /// Description of what's missing.
    pub gap: String,
    /// Importance of filling this gap (0.0-1.0).
    pub importance: f64,
    /// Suggested evidence to gather.
    pub suggested_evidence: String,
}

/// Result of Bayesian probability update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilisticResult {
    /// Unique update ID.
    pub update_id: String,
    /// Session ID.
    pub session_id: String,
    /// The hypothesis evaluated.
    pub hypothesis: String,
    /// Prior probability.
    pub prior: f64,
    /// Posterior probability after all evidence.
    pub posterior: f64,
    /// Confidence interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_interval: Option<ProbabilityInterval>,
    /// Update steps for each evidence.
    pub update_steps: Vec<BayesianUpdateStep>,
    /// Uncertainty metrics.
    pub uncertainty: UncertaintyMetrics,
    /// Human interpretation.
    pub interpretation: ProbabilityInterpretation,
}

/// Confidence interval for probability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilityInterval {
    /// Lower bound.
    pub lower: f64,
    /// Upper bound.
    pub upper: f64,
    /// Confidence level (e.g., 0.95).
    pub level: f64,
}

/// Single Bayesian update step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BayesianUpdateStep {
    /// Evidence description.
    pub evidence: String,
    /// Prior before this evidence.
    pub prior: f64,
    /// Posterior after this evidence.
    pub posterior: f64,
    /// Likelihood ratio used.
    pub likelihood_ratio: f64,
}

/// Uncertainty metrics for probability assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyMetrics {
    /// Shannon entropy before updates.
    pub entropy_before: f64,
    /// Shannon entropy after updates.
    pub entropy_after: f64,
    /// Information gained.
    pub information_gained: f64,
}

/// Human interpretation of probability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilityInterpretation {
    /// Verbal probability (almost_certain, highly_likely, likely, possible, unlikely, etc.).
    pub verbal: String,
    /// Recommendation based on probability.
    pub recommendation: String,
    /// Important caveats.
    pub caveats: Vec<String>,
}

// ============================================================================
// Mode Handler
// ============================================================================

/// Evidence assessment mode handler.
#[derive(Clone)]
pub struct EvidenceMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// Consolidated pipe name for decision framework operations (prompts passed dynamically).
    decision_framework_pipe: String,
}

impl EvidenceMode {
    /// Create a new evidence mode handler.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            core: ModeCore::new(storage, langbase),
            decision_framework_pipe: config
                .pipes
                .evidence
                .as_ref()
                .and_then(|e| e.pipe.clone())
                .unwrap_or_else(|| "decision-framework-v1".to_string()),
        }
    }

    /// Assess evidence for a claim.
    pub async fn assess_evidence(&self, params: EvidenceParams) -> AppResult<EvidenceResult> {
        let start = Instant::now();

        // Validate input
        self.validate_evidence_params(&params)?;

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "evidence")
            .await?;
        debug!(session_id = %session.id, "Processing evidence assessment");

        // Build messages for Langbase
        let messages = self.build_evidence_messages(&params);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.assess_evidence",
            serialize_for_log(&params, "reasoning.assess_evidence input"),
        )
        .with_session(&session.id)
        .with_pipe(&self.decision_framework_pipe);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.decision_framework_pipe, messages);
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
        let evidence_response = self.parse_evidence_response(&response.completion)?;

        // Generate assessment ID
        let assessment_id = uuid::Uuid::new_v4().to_string();

        // Build result
        let result = EvidenceResult {
            assessment_id: assessment_id.clone(),
            session_id: session.id.clone(),
            claim: params.claim.clone(),
            overall_support: SupportLevel {
                level: evidence_response.overall_support.level,
                confidence: evidence_response.overall_support.confidence,
                explanation: evidence_response.overall_support.explanation,
            },
            evidence_analyses: evidence_response
                .evidence_analysis
                .into_iter()
                .map(|ea| EvidenceAnalysis {
                    evidence_id: ea.evidence_id,
                    content_summary: ea.content_summary,
                    relevance: ea.relevance.score,
                    credibility: ea.credibility.score,
                    weight: ea.weight,
                    supports_claim: ea.supports_claim,
                    notes: ea.relevance.explanation,
                })
                .collect(),
            chain_analysis: evidence_response.chain_analysis.map(|ca| InferentialChain {
                primary_chain: ca.primary_chain,
                weak_links: ca
                    .weak_links
                    .into_iter()
                    .map(|wl| ChainWeakness {
                        from: wl.from,
                        to: wl.to,
                        weakness: wl.weakness,
                        impact: wl.impact,
                    })
                    .collect(),
                redundant_evidence: ca.redundancy,
            }),
            contradictions: evidence_response
                .contradictions
                .into_iter()
                .map(|c| EvidenceContradiction {
                    evidence_a: c.evidence_a,
                    evidence_b: c.evidence_b,
                    nature: c.nature,
                    resolution: c.resolution,
                })
                .collect(),
            gaps: evidence_response
                .gaps
                .into_iter()
                .map(|g| Gap {
                    gap: g.gap,
                    importance: g.importance,
                    suggested_evidence: g.suggested_evidence,
                })
                .collect(),
            recommendations: evidence_response.recommendations,
        };

        // Persist to storage
        let mut stored_evidence = StoredEvidence::new(
            &session.id,
            &params.claim,
            serde_json::to_value(&params.evidence).unwrap_or_default(),
            serde_json::to_value(&result.overall_support).unwrap_or_default(),
            serde_json::to_value(&result.evidence_analyses).unwrap_or_default(),
        )
        .with_contradictions(serde_json::to_value(&result.contradictions).unwrap_or_default())
        .with_gaps(serde_json::to_value(&result.gaps).unwrap_or_default())
        .with_recommendations(serde_json::to_value(&result.recommendations).unwrap_or_default());

        if let Some(chain) = &result.chain_analysis {
            stored_evidence = stored_evidence
                .with_chain_analysis(serde_json::to_value(chain).unwrap_or_default());
        }

        self.core
            .storage()
            .create_evidence_assessment(&stored_evidence)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    assessment_id = %assessment_id,
                    "Failed to persist evidence assessment - operation failed"
                );
                e
            })?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.assess_evidence output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            assessment_id = %assessment_id,
            support_level = %result.overall_support.level,
            latency_ms = latency,
            "Evidence assessment completed"
        );

        Ok(result)
    }

    /// Perform Bayesian probability update.
    pub async fn update_probability(
        &self,
        params: ProbabilisticParams,
    ) -> AppResult<ProbabilisticResult> {
        let start = Instant::now();

        // Validate input
        self.validate_probabilistic_params(&params)?;

        // Get or create session
        let session = self
            .core
            .storage()
            .get_or_create_session(&params.session_id, "evidence")
            .await?;
        debug!(session_id = %session.id, "Processing probabilistic update");

        // Build messages for Langbase
        let messages = self.build_probabilistic_messages(&params);

        // Create invocation log
        let mut invocation = Invocation::new(
            "reasoning.probabilistic",
            serialize_for_log(&params, "reasoning.probabilistic input"),
        )
        .with_session(&session.id)
        .with_pipe(&self.decision_framework_pipe);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.decision_framework_pipe, messages);
        let response = match self.core.langbase().call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                self.core.storage().log_invocation(&invocation).await?;

                error!(error = %e, "Langbase call failed - propagating error");
                return Err(ToolError::PipeUnavailable {
                    pipe: self.decision_framework_pipe.clone(),
                    reason: e.to_string(),
                }
                .into());
            }
        };

        // Parse response
        let bayesian_response = self.parse_bayesian_response(&response.completion, &params)?;

        // Generate update ID
        let update_id = uuid::Uuid::new_v4().to_string();

        // Build result
        let result = ProbabilisticResult {
            update_id: update_id.clone(),
            session_id: session.id.clone(),
            hypothesis: params.hypothesis.clone(),
            prior: params.prior,
            posterior: bayesian_response.posterior,
            confidence_interval: bayesian_response.confidence_interval.map(|ci| {
                ProbabilityInterval {
                    lower: ci.lower,
                    upper: ci.upper,
                    level: ci.level,
                }
            }),
            update_steps: bayesian_response
                .update_steps
                .into_iter()
                .map(|us| BayesianUpdateStep {
                    evidence: us.evidence,
                    prior: us.prior_before,
                    posterior: us.posterior_after,
                    likelihood_ratio: us.likelihood_ratio,
                })
                .collect(),
            uncertainty: bayesian_response
                .uncertainty_analysis
                .map(|ua| UncertaintyMetrics {
                    entropy_before: ua.entropy_before,
                    entropy_after: ua.entropy_after,
                    information_gained: ua.information_gained,
                })
                .unwrap_or_else(|| {
                    let entropy_before = self.calculate_entropy(params.prior);
                    let entropy_after = self.calculate_entropy(bayesian_response.posterior);
                    UncertaintyMetrics {
                        entropy_before,
                        entropy_after,
                        information_gained: entropy_before - entropy_after,
                    }
                }),
            interpretation: ProbabilityInterpretation {
                verbal: bayesian_response.interpretation.verbal_probability,
                recommendation: bayesian_response.interpretation.recommendation,
                caveats: bayesian_response.interpretation.caveats,
            },
        };

        // Persist to storage
        let mut stored_probability = StoredProbability::new(
            &session.id,
            &params.hypothesis,
            result.prior,
            result.posterior,
            serde_json::to_value(&result.update_steps).unwrap_or_default(),
            serde_json::to_value(&result.interpretation).unwrap_or_default(),
        )
        .with_uncertainty(serde_json::to_value(&result.uncertainty).unwrap_or_default());

        if let Some(ci) = &result.confidence_interval {
            stored_probability = stored_probability.with_confidence_interval(
                Some(ci.lower),
                Some(ci.upper),
                Some(ci.level),
            );
        }

        self.core
            .storage()
            .create_probability_update(&stored_probability)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    update_id = %update_id,
                    "Failed to persist probability update - operation failed"
                );
                e
            })?;

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.probabilistic output"),
            latency,
        );
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            update_id = %update_id,
            prior = %params.prior,
            posterior = %result.posterior,
            latency_ms = latency,
            "Probabilistic update completed"
        );

        Ok(result)
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    fn validate_evidence_params(&self, params: &EvidenceParams) -> AppResult<()> {
        if params.claim.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "claim".to_string(),
                reason: "Claim cannot be empty".to_string(),
            }
            .into());
        }

        if params.evidence.is_empty() {
            return Err(ToolError::Validation {
                field: "evidence".to_string(),
                reason: "At least one evidence item is required".to_string(),
            }
            .into());
        }

        Ok(())
    }

    fn validate_probabilistic_params(&self, params: &ProbabilisticParams) -> AppResult<()> {
        if params.hypothesis.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "hypothesis".to_string(),
                reason: "Hypothesis cannot be empty".to_string(),
            }
            .into());
        }

        if !(0.0..=1.0).contains(&params.prior) {
            return Err(ToolError::Validation {
                field: "prior".to_string(),
                reason: "Prior probability must be between 0 and 1".to_string(),
            }
            .into());
        }

        Ok(())
    }

    fn build_evidence_messages(&self, params: &EvidenceParams) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(EVIDENCE_ASSESSOR_PROMPT.to_string()));

        // Build user message with claim and evidence
        let evidence_json = serde_json::to_string_pretty(&params.evidence).unwrap_or_default();
        let user_content = if let Some(ref context) = params.context {
            format!(
                "Assess the following evidence for this claim:\n\nClaim: {}\n\nContext: {}\n\nEvidence:\n{}",
                params.claim, context, evidence_json
            )
        } else {
            format!(
                "Assess the following evidence for this claim:\n\nClaim: {}\n\nEvidence:\n{}",
                params.claim, evidence_json
            )
        };

        messages.push(Message::user(user_content));
        messages
    }

    fn build_probabilistic_messages(&self, params: &ProbabilisticParams) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(BAYESIAN_UPDATER_PROMPT.to_string()));

        // Build user message
        let evidence_json = serde_json::to_string_pretty(&params.evidence).unwrap_or_default();
        let user_content = format!(
            "Perform Bayesian probability updates for this hypothesis:\n\nHypothesis: {}\n\nPrior probability: {}\n\nEvidence:\n{}",
            params.hypothesis, params.prior, evidence_json
        );

        messages.push(Message::user(user_content));
        messages
    }

    fn parse_evidence_response(&self, completion: &str) -> AppResult<EvidenceResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from evidence response"
            );
            ToolError::Reasoning {
                message: format!("Evidence response extraction failed: {}", e),
            }
        })?;

        serde_json::from_str::<EvidenceResponse>(json_str).map_err(|e| {
            ToolError::Reasoning {
                message: format!("Failed to parse evidence response: {}", e),
            }
            .into()
        })
    }

    fn parse_bayesian_response(
        &self,
        completion: &str,
        _params: &ProbabilisticParams,
    ) -> AppResult<BayesianResponse> {
        let json_str = extract_json_from_completion(completion).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %completion.chars().take(200).collect::<String>(),
                "Failed to extract JSON from Bayesian response"
            );
            ToolError::Reasoning {
                message: format!("Bayesian response extraction failed: {}", e),
            }
        })?;

        // Parse response - returns error on failure (no fallbacks)
        serde_json::from_str::<BayesianResponse>(json_str).map_err(|e| {
            let preview: String = json_str.chars().take(200).collect();
            ToolError::ParseFailed {
                mode: "evidence.probabilistic".to_string(),
                message: format!("JSON parse error: {} | Response preview: {}", e, preview),
            }
            .into()
        })
    }

    /// Calculate Shannon entropy for a probability.
    fn calculate_entropy(&self, p: f64) -> f64 {
        if p <= 0.0 || p >= 1.0 {
            return 0.0;
        }
        let q = 1.0 - p;
        -(p * p.log2() + q * q.log2())
    }
}

// ============================================================================
// Builder Methods
// ============================================================================

impl EvidenceParams {
    /// Create new evidence params with claim.
    pub fn new(claim: impl Into<String>) -> Self {
        Self {
            claim: claim.into(),
            evidence: Vec::new(),
            session_id: None,
            context: None,
        }
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add evidence.
    pub fn with_evidence(mut self, content: impl Into<String>) -> Self {
        self.evidence.push(EvidenceInput {
            content: content.into(),
            source: None,
            source_type: None,
            date: None,
        });
        self
    }

    /// Add evidence with source.
    pub fn with_sourced_evidence(
        mut self,
        content: impl Into<String>,
        source: impl Into<String>,
        source_type: SourceType,
    ) -> Self {
        self.evidence.push(EvidenceInput {
            content: content.into(),
            source: Some(source.into()),
            source_type: Some(source_type),
            date: None,
        });
        self
    }

    /// Set context.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

impl ProbabilisticParams {
    /// Create new probabilistic params.
    pub fn new(hypothesis: impl Into<String>, prior: f64) -> Self {
        Self {
            hypothesis: hypothesis.into(),
            prior,
            evidence: Vec::new(),
            session_id: None,
        }
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add evidence with likelihoods.
    pub fn with_evidence(
        mut self,
        description: impl Into<String>,
        likelihood_true: f64,
        likelihood_false: f64,
    ) -> Self {
        self.evidence.push(BayesianEvidence {
            description: description.into(),
            likelihood_if_true: Some(likelihood_true),
            likelihood_if_false: Some(likelihood_false),
        });
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
    // EvidenceParams Tests
    // ========================================================================

    #[test]
    fn test_evidence_params_new() {
        let params = EvidenceParams::new("Test claim");
        assert_eq!(params.claim, "Test claim");
        assert!(params.evidence.is_empty());
        assert!(params.session_id.is_none());
        assert!(params.context.is_none());
    }

    #[test]
    fn test_evidence_params_with_session() {
        let params = EvidenceParams::new("Claim").with_session("sess-123");
        assert_eq!(params.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_evidence_params_with_evidence() {
        let params = EvidenceParams::new("Claim")
            .with_evidence("Evidence 1")
            .with_evidence("Evidence 2");
        assert_eq!(params.evidence.len(), 2);
        assert_eq!(params.evidence[0].content, "Evidence 1");
    }

    #[test]
    fn test_evidence_params_with_sourced_evidence() {
        let params = EvidenceParams::new("Claim").with_sourced_evidence(
            "Statistical data",
            "Research paper",
            SourceType::Primary,
        );
        assert_eq!(params.evidence.len(), 1);
        assert_eq!(
            params.evidence[0].source,
            Some("Research paper".to_string())
        );
        assert_eq!(params.evidence[0].source_type, Some(SourceType::Primary));
    }

    #[test]
    fn test_evidence_params_with_context() {
        let params = EvidenceParams::new("Claim").with_context("Additional context");
        assert_eq!(params.context, Some("Additional context".to_string()));
    }

    // ========================================================================
    // ProbabilisticParams Tests
    // ========================================================================

    #[test]
    fn test_probabilistic_params_new() {
        let params = ProbabilisticParams::new("Hypothesis", 0.5);
        assert_eq!(params.hypothesis, "Hypothesis");
        assert_eq!(params.prior, 0.5);
        assert!(params.evidence.is_empty());
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_probabilistic_params_with_session() {
        let params = ProbabilisticParams::new("H", 0.5).with_session("sess-456");
        assert_eq!(params.session_id, Some("sess-456".to_string()));
    }

    #[test]
    fn test_probabilistic_params_with_evidence() {
        let params = ProbabilisticParams::new("H", 0.5).with_evidence("Evidence", 0.8, 0.2);
        assert_eq!(params.evidence.len(), 1);
        assert_eq!(params.evidence[0].description, "Evidence");
        assert_eq!(params.evidence[0].likelihood_if_true, Some(0.8));
        assert_eq!(params.evidence[0].likelihood_if_false, Some(0.2));
    }

    // ========================================================================
    // SourceType Tests
    // ========================================================================

    #[test]
    fn test_source_type_display() {
        assert_eq!(format!("{}", SourceType::Primary), "primary");
        assert_eq!(format!("{}", SourceType::Secondary), "secondary");
        assert_eq!(format!("{}", SourceType::Anecdotal), "anecdotal");
        assert_eq!(format!("{}", SourceType::Expert), "expert");
        assert_eq!(format!("{}", SourceType::Statistical), "statistical");
    }

    #[test]
    fn test_source_type_serialize() {
        let json = serde_json::to_string(&SourceType::Primary).unwrap();
        assert_eq!(json, "\"primary\"");
    }

    #[test]
    fn test_source_type_deserialize() {
        let st: SourceType = serde_json::from_str("\"expert\"").unwrap();
        assert_eq!(st, SourceType::Expert);
    }

    // ========================================================================
    // Result Type Tests
    // ========================================================================

    #[test]
    fn test_support_level_serialize() {
        let sl = SupportLevel {
            level: "strong".to_string(),
            confidence: 0.85,
            explanation: "Well supported".to_string(),
        };
        let json = serde_json::to_string(&sl).unwrap();
        assert!(json.contains("strong"));
        assert!(json.contains("0.85"));
    }

    #[test]
    fn test_evidence_analysis_serialize() {
        let ea = EvidenceAnalysis {
            evidence_id: "e1".to_string(),
            content_summary: "Summary".to_string(),
            relevance: 0.9,
            credibility: 0.8,
            weight: 0.72,
            supports_claim: true,
            notes: "Good evidence".to_string(),
        };
        let json = serde_json::to_string(&ea).unwrap();
        assert!(json.contains("e1"));
        assert!(json.contains("0.72"));
    }

    #[test]
    fn test_bayesian_update_step_serialize() {
        let step = BayesianUpdateStep {
            evidence: "Test".to_string(),
            prior: 0.5,
            posterior: 0.7,
            likelihood_ratio: 2.33,
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("2.33"));
    }

    #[test]
    fn test_uncertainty_metrics_serialize() {
        let um = UncertaintyMetrics {
            entropy_before: 1.0,
            entropy_after: 0.88,
            information_gained: 0.12,
        };
        let json = serde_json::to_string(&um).unwrap();
        assert!(json.contains("0.88"));
        assert!(json.contains("0.12"));
    }

    #[test]
    fn test_probability_interpretation_serialize() {
        let pi = ProbabilityInterpretation {
            verbal: "likely".to_string(),
            recommendation: "Proceed".to_string(),
            caveats: vec!["Limited data".to_string()],
        };
        let json = serde_json::to_string(&pi).unwrap();
        assert!(json.contains("likely"));
        assert!(json.contains("Limited data"));
    }

    // ========================================================================
    // Entropy Calculation Tests
    // ========================================================================

    #[test]
    fn test_entropy_calculation() {
        // Create a minimal EvidenceMode for testing
        // Note: In actual use, these would use the mode's calculate_entropy method
        // Here we test the formula directly

        // Helper function for entropy calculation
        fn entropy(p: f64) -> f64 {
            if p <= 0.0 || p >= 1.0 {
                0.0
            } else {
                -(p * p.log2() + (1.0 - p) * (1.0 - p).log2())
            }
        }

        // Entropy should be 0 at extremes
        assert_eq!(entropy(0.0), 0.0);
        assert_eq!(entropy(1.0), 0.0);

        // Maximum entropy at p=0.5
        let entropy_half = entropy(0.5);
        assert!((entropy_half - 1.0).abs() < 0.0001); // Should be approximately 1 bit
    }

    // ========================================================================
    // EvidenceInput Tests
    // ========================================================================

    #[test]
    fn test_evidence_input_serialize() {
        let ei = EvidenceInput {
            content: "Data shows X".to_string(),
            source: Some("Journal".to_string()),
            source_type: Some(SourceType::Statistical),
            date: Some("2023-01-15".to_string()),
        };
        let json = serde_json::to_string(&ei).unwrap();
        assert!(json.contains("Data shows X"));
        assert!(json.contains("Journal"));
        assert!(json.contains("statistical"));
    }

    #[test]
    fn test_evidence_input_deserialize() {
        let json = r#"{"content":"Test content","source":"Source","source_type":"primary","date":"2023-01-01"}"#;
        let ei: EvidenceInput = serde_json::from_str(json).unwrap();
        assert_eq!(ei.content, "Test content");
        assert_eq!(ei.source, Some("Source".to_string()));
        assert_eq!(ei.source_type, Some(SourceType::Primary));
    }

    #[test]
    fn test_evidence_input_minimal() {
        let ei = EvidenceInput {
            content: "Minimal".to_string(),
            source: None,
            source_type: None,
            date: None,
        };
        let json = serde_json::to_string(&ei).unwrap();
        assert!(json.contains("Minimal"));
        // Optional fields should be skipped when None
        assert!(!json.contains("source"));
    }

    // ========================================================================
    // EvidenceParams Serialization Tests
    // ========================================================================

    #[test]
    fn test_evidence_params_serialize() {
        let params = EvidenceParams::new("Claim")
            .with_evidence("E1")
            .with_session("s1")
            .with_context("ctx");
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Claim"));
        assert!(json.contains("E1"));
        assert!(json.contains("s1"));
        assert!(json.contains("ctx"));
    }

    #[test]
    fn test_evidence_params_deserialize() {
        let json = r#"{"claim":"Test","evidence":[{"content":"E1"}]}"#;
        let params: EvidenceParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.claim, "Test");
        assert_eq!(params.evidence.len(), 1);
        assert_eq!(params.evidence[0].content, "E1");
    }

    #[test]
    fn test_evidence_params_round_trip() {
        let original = EvidenceParams::new("Round trip test")
            .with_evidence("Evidence 1")
            .with_evidence("Evidence 2")
            .with_session("session-123");

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EvidenceParams = serde_json::from_str(&json).unwrap();

        assert_eq!(original.claim, deserialized.claim);
        assert_eq!(original.evidence.len(), deserialized.evidence.len());
        assert_eq!(original.session_id, deserialized.session_id);
    }

    // ========================================================================
    // BayesianEvidence Tests
    // ========================================================================

    #[test]
    fn test_bayesian_evidence_serialize() {
        let be = BayesianEvidence {
            description: "Test evidence".to_string(),
            likelihood_if_true: Some(0.8),
            likelihood_if_false: Some(0.2),
        };
        let json = serde_json::to_string(&be).unwrap();
        assert!(json.contains("Test evidence"));
        assert!(json.contains("0.8"));
        assert!(json.contains("0.2"));
    }

    #[test]
    fn test_bayesian_evidence_deserialize() {
        let json = r#"{"description":"E","likelihood_if_true":0.9,"likelihood_if_false":0.1}"#;
        let be: BayesianEvidence = serde_json::from_str(json).unwrap();
        assert_eq!(be.description, "E");
        assert_eq!(be.likelihood_if_true, Some(0.9));
        assert_eq!(be.likelihood_if_false, Some(0.1));
    }

    #[test]
    fn test_bayesian_evidence_optional_likelihoods() {
        let be = BayesianEvidence {
            description: "Incomplete".to_string(),
            likelihood_if_true: None,
            likelihood_if_false: None,
        };
        let json = serde_json::to_string(&be).unwrap();
        assert!(json.contains("Incomplete"));
    }

    #[test]
    fn test_bayesian_evidence_round_trip() {
        let original = BayesianEvidence {
            description: "Round trip evidence".to_string(),
            likelihood_if_true: Some(0.75),
            likelihood_if_false: Some(0.25),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: BayesianEvidence = serde_json::from_str(&json).unwrap();

        assert_eq!(original.description, deserialized.description);
        assert_eq!(original.likelihood_if_true, deserialized.likelihood_if_true);
        assert_eq!(
            original.likelihood_if_false,
            deserialized.likelihood_if_false
        );
    }

    // ========================================================================
    // ProbabilisticParams Serialization Tests
    // ========================================================================

    #[test]
    fn test_probabilistic_params_serialize() {
        let params = ProbabilisticParams::new("H", 0.6)
            .with_evidence("E1", 0.8, 0.2)
            .with_session("s2");
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"H\""));
        assert!(json.contains("0.6"));
        assert!(json.contains("E1"));
    }

    #[test]
    fn test_probabilistic_params_deserialize() {
        let json = r#"{"hypothesis":"Test H","prior":0.5,"evidence":[]}"#;
        let params: ProbabilisticParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.hypothesis, "Test H");
        assert_eq!(params.prior, 0.5);
    }

    #[test]
    fn test_probabilistic_params_round_trip() {
        let original = ProbabilisticParams::new("Round trip hypothesis", 0.4)
            .with_evidence("E1", 0.9, 0.1)
            .with_evidence("E2", 0.7, 0.3);

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ProbabilisticParams = serde_json::from_str(&json).unwrap();

        assert_eq!(original.hypothesis, deserialized.hypothesis);
        assert_eq!(original.prior, deserialized.prior);
        assert_eq!(original.evidence.len(), deserialized.evidence.len());
    }

    // ========================================================================
    // Response Type Tests
    // ========================================================================

    #[test]
    fn test_probability_interval_serialize() {
        let pi = ProbabilityInterval {
            lower: 0.3,
            upper: 0.7,
            level: 0.95,
        };
        let json = serde_json::to_string(&pi).unwrap();
        assert!(json.contains("0.3"));
        assert!(json.contains("0.7"));
        assert!(json.contains("0.95"));
    }

    #[test]
    fn test_chain_weakness_serialize() {
        let cw = ChainWeakness {
            from: "Node A".to_string(),
            to: "Node B".to_string(),
            weakness: "Weak assumption".to_string(),
            impact: 0.6,
        };
        let json = serde_json::to_string(&cw).unwrap();
        assert!(json.contains("Node A"));
        assert!(json.contains("Weak assumption"));
    }

    #[test]
    fn test_evidence_contradiction_serialize() {
        let ec = EvidenceContradiction {
            evidence_a: "E1".to_string(),
            evidence_b: "E2".to_string(),
            nature: "Direct conflict".to_string(),
            resolution: "Consider E1 more credible".to_string(),
        };
        let json = serde_json::to_string(&ec).unwrap();
        assert!(json.contains("E1"));
        assert!(json.contains("Direct conflict"));
    }

    #[test]
    fn test_gap_serialize() {
        let gap = Gap {
            gap: "Missing baseline data".to_string(),
            importance: 0.8,
            suggested_evidence: "Collect historical data".to_string(),
        };
        let json = serde_json::to_string(&gap).unwrap();
        assert!(json.contains("Missing baseline"));
        assert!(json.contains("0.8"));
    }

    // ========================================================================
    // Default and Edge Cases
    // ========================================================================

    #[test]
    fn test_credibility_factors_default() {
        let cf = CredibilityFactors::default();
        assert_eq!(cf.source_reliability, 0.0);
        assert_eq!(cf.methodology, 0.0);
        assert_eq!(cf.recency, 0.0);
        assert_eq!(cf.corroboration, 0.0);
    }

    #[test]
    fn test_source_type_equality() {
        assert_eq!(SourceType::Primary, SourceType::Primary);
        assert_ne!(SourceType::Primary, SourceType::Secondary);
        assert_ne!(SourceType::Expert, SourceType::Anecdotal);
    }

    #[test]
    fn test_all_source_types_display() {
        let types = vec![
            SourceType::Primary,
            SourceType::Secondary,
            SourceType::Anecdotal,
            SourceType::Expert,
            SourceType::Statistical,
        ];
        for t in types {
            let display = format!("{}", t);
            assert!(!display.is_empty());
        }
    }

    // ========================================================================
    // EvidenceResult Tests
    // ========================================================================

    #[test]
    fn test_evidence_result_serialize() {
        let result = EvidenceResult {
            assessment_id: "assess-1".to_string(),
            session_id: "sess-1".to_string(),
            claim: "Test claim".to_string(),
            overall_support: SupportLevel {
                level: "strong".to_string(),
                confidence: 0.9,
                explanation: "Well supported".to_string(),
            },
            evidence_analyses: vec![],
            chain_analysis: None,
            contradictions: vec![],
            gaps: vec![],
            recommendations: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("assess-1"));
        assert!(json.contains("Test claim"));
        assert!(json.contains("strong"));
    }

    #[test]
    fn test_evidence_result_with_analyses() {
        let result = EvidenceResult {
            assessment_id: "assess-2".to_string(),
            session_id: "sess-2".to_string(),
            claim: "Claim".to_string(),
            overall_support: SupportLevel {
                level: "moderate".to_string(),
                confidence: 0.7,
                explanation: "Some support".to_string(),
            },
            evidence_analyses: vec![EvidenceAnalysis {
                evidence_id: "e1".to_string(),
                content_summary: "Summary".to_string(),
                relevance: 0.8,
                credibility: 0.9,
                weight: 0.72,
                supports_claim: true,
                notes: "Notes".to_string(),
            }],
            chain_analysis: None,
            contradictions: vec![],
            gaps: vec![],
            recommendations: vec!["Get more evidence".to_string()],
        };

        assert_eq!(result.evidence_analyses.len(), 1);
        assert_eq!(result.recommendations.len(), 1);
        assert_eq!(result.evidence_analyses[0].weight, 0.72);
    }

    #[test]
    fn test_evidence_result_deserialize() {
        let json = r#"{
            "assessment_id": "a1",
            "session_id": "s1",
            "claim": "Claim",
            "overall_support": {
                "level": "weak",
                "confidence": 0.4,
                "explanation": "Limited support"
            },
            "evidence_analyses": [],
            "contradictions": [],
            "gaps": [],
            "recommendations": []
        }"#;
        let result: EvidenceResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.assessment_id, "a1");
        assert_eq!(result.overall_support.level, "weak");
        assert_eq!(result.overall_support.confidence, 0.4);
    }

    #[test]
    fn test_evidence_result_round_trip() {
        let original = EvidenceResult {
            assessment_id: "round-trip".to_string(),
            session_id: "sess".to_string(),
            claim: "Test".to_string(),
            overall_support: SupportLevel {
                level: "strong".to_string(),
                confidence: 0.95,
                explanation: "Clear support".to_string(),
            },
            evidence_analyses: vec![],
            chain_analysis: None,
            contradictions: vec![],
            gaps: vec![],
            recommendations: vec![],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EvidenceResult = serde_json::from_str(&json).unwrap();

        assert_eq!(original.assessment_id, deserialized.assessment_id);
        assert_eq!(original.claim, deserialized.claim);
        assert_eq!(
            original.overall_support.confidence,
            deserialized.overall_support.confidence
        );
    }

    // ========================================================================
    // ProbabilisticResult Tests
    // ========================================================================

    #[test]
    fn test_probabilistic_result_serialize() {
        let result = ProbabilisticResult {
            update_id: "update-1".to_string(),
            session_id: "sess-1".to_string(),
            hypothesis: "H".to_string(),
            prior: 0.5,
            posterior: 0.7,
            confidence_interval: None,
            update_steps: vec![],
            uncertainty: UncertaintyMetrics {
                entropy_before: 1.0,
                entropy_after: 0.88,
                information_gained: 0.12,
            },
            interpretation: ProbabilityInterpretation {
                verbal: "likely".to_string(),
                recommendation: "Proceed".to_string(),
                caveats: vec![],
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("update-1"));
        assert!(json.contains("0.5"));
        assert!(json.contains("0.7"));
        assert!(json.contains("likely"));
    }

    #[test]
    fn test_probabilistic_result_with_interval() {
        let result = ProbabilisticResult {
            update_id: "update-2".to_string(),
            session_id: "sess-2".to_string(),
            hypothesis: "Test H".to_string(),
            prior: 0.3,
            posterior: 0.6,
            confidence_interval: Some(ProbabilityInterval {
                lower: 0.5,
                upper: 0.7,
                level: 0.95,
            }),
            update_steps: vec![BayesianUpdateStep {
                evidence: "E".to_string(),
                prior: 0.3,
                posterior: 0.6,
                likelihood_ratio: 3.0,
            }],
            uncertainty: UncertaintyMetrics {
                entropy_before: 0.88,
                entropy_after: 0.97,
                information_gained: -0.09,
            },
            interpretation: ProbabilityInterpretation {
                verbal: "possible".to_string(),
                recommendation: "Gather more evidence".to_string(),
                caveats: vec!["Limited data".to_string()],
            },
        };

        assert!(result.confidence_interval.is_some());
        let ci = result.confidence_interval.unwrap();
        assert_eq!(ci.lower, 0.5);
        assert_eq!(ci.upper, 0.7);
        assert_eq!(ci.level, 0.95);
    }

    #[test]
    fn test_probabilistic_result_deserialize() {
        let json = r#"{
            "update_id": "u1",
            "session_id": "s1",
            "hypothesis": "H",
            "prior": 0.5,
            "posterior": 0.8,
            "update_steps": [],
            "uncertainty": {
                "entropy_before": 1.0,
                "entropy_after": 0.72,
                "information_gained": 0.28
            },
            "interpretation": {
                "verbal": "highly_likely",
                "recommendation": "Act",
                "caveats": []
            }
        }"#;
        let result: ProbabilisticResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.update_id, "u1");
        assert_eq!(result.prior, 0.5);
        assert_eq!(result.posterior, 0.8);
        assert_eq!(result.interpretation.verbal, "highly_likely");
    }

    #[test]
    fn test_probabilistic_result_round_trip() {
        let original = ProbabilisticResult {
            update_id: "round".to_string(),
            session_id: "s".to_string(),
            hypothesis: "Test hypothesis".to_string(),
            prior: 0.4,
            posterior: 0.75,
            confidence_interval: Some(ProbabilityInterval {
                lower: 0.65,
                upper: 0.85,
                level: 0.95,
            }),
            update_steps: vec![],
            uncertainty: UncertaintyMetrics {
                entropy_before: 0.97,
                entropy_after: 0.81,
                information_gained: 0.16,
            },
            interpretation: ProbabilityInterpretation {
                verbal: "likely".to_string(),
                recommendation: "Test".to_string(),
                caveats: vec![],
            },
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ProbabilisticResult = serde_json::from_str(&json).unwrap();

        assert_eq!(original.hypothesis, deserialized.hypothesis);
        assert_eq!(original.prior, deserialized.prior);
        assert_eq!(original.posterior, deserialized.posterior);
    }

    // ========================================================================
    // InferentialChain Tests
    // ========================================================================

    #[test]
    fn test_inferential_chain_serialize() {
        let chain = InferentialChain {
            primary_chain: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            weak_links: vec![ChainWeakness {
                from: "A".to_string(),
                to: "B".to_string(),
                weakness: "Assumption".to_string(),
                impact: 0.5,
            }],
            redundant_evidence: vec!["E1".to_string(), "E2".to_string()],
        };
        let json = serde_json::to_string(&chain).unwrap();
        assert!(json.contains("\"A\""));
        assert!(json.contains("Assumption"));
        assert!(json.contains("E1"));
    }

    #[test]
    fn test_inferential_chain_deserialize() {
        let json = r#"{
            "primary_chain": ["X", "Y", "Z"],
            "weak_links": [],
            "redundant_evidence": []
        }"#;
        let chain: InferentialChain = serde_json::from_str(json).unwrap();
        assert_eq!(chain.primary_chain.len(), 3);
        assert_eq!(chain.primary_chain[0], "X");
    }

    #[test]
    fn test_inferential_chain_round_trip() {
        let original = InferentialChain {
            primary_chain: vec!["Step1".to_string(), "Step2".to_string()],
            weak_links: vec![],
            redundant_evidence: vec!["Evidence".to_string()],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: InferentialChain = serde_json::from_str(&json).unwrap();

        assert_eq!(original.primary_chain, deserialized.primary_chain);
        assert_eq!(original.redundant_evidence, deserialized.redundant_evidence);
    }

    // ========================================================================
    // Edge Cases and Boundary Values
    // ========================================================================

    #[test]
    fn test_evidence_params_empty_claim() {
        let params = EvidenceParams::new("");
        assert_eq!(params.claim, "");
    }

    #[test]
    fn test_probabilistic_params_boundary_priors() {
        let params_zero = ProbabilisticParams::new("H", 0.0);
        assert_eq!(params_zero.prior, 0.0);

        let params_one = ProbabilisticParams::new("H", 1.0);
        assert_eq!(params_one.prior, 1.0);
    }

    #[test]
    fn test_support_level_round_trip() {
        let original = SupportLevel {
            level: "contradictory".to_string(),
            confidence: 0.65,
            explanation: "Mixed evidence".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SupportLevel = serde_json::from_str(&json).unwrap();

        assert_eq!(original.level, deserialized.level);
        assert_eq!(original.confidence, deserialized.confidence);
        assert_eq!(original.explanation, deserialized.explanation);
    }

    #[test]
    fn test_evidence_analysis_round_trip() {
        let original = EvidenceAnalysis {
            evidence_id: "id-123".to_string(),
            content_summary: "Summary text".to_string(),
            relevance: 0.95,
            credibility: 0.88,
            weight: 0.8360,
            supports_claim: false,
            notes: "Detailed notes".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EvidenceAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(original.evidence_id, deserialized.evidence_id);
        assert_eq!(original.relevance, deserialized.relevance);
        assert_eq!(original.supports_claim, deserialized.supports_claim);
    }

    #[test]
    fn test_bayesian_update_step_round_trip() {
        let original = BayesianUpdateStep {
            evidence: "Strong evidence".to_string(),
            prior: 0.25,
            posterior: 0.67,
            likelihood_ratio: 5.33,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: BayesianUpdateStep = serde_json::from_str(&json).unwrap();

        assert_eq!(original.evidence, deserialized.evidence);
        assert_eq!(original.prior, deserialized.prior);
        assert_eq!(original.posterior, deserialized.posterior);
        assert_eq!(original.likelihood_ratio, deserialized.likelihood_ratio);
    }

    #[test]
    fn test_uncertainty_metrics_round_trip() {
        let original = UncertaintyMetrics {
            entropy_before: 0.95,
            entropy_after: 0.72,
            information_gained: 0.23,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: UncertaintyMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(original.entropy_before, deserialized.entropy_before);
        assert_eq!(original.entropy_after, deserialized.entropy_after);
        assert_eq!(original.information_gained, deserialized.information_gained);
    }

    #[test]
    fn test_probability_interpretation_round_trip() {
        let original = ProbabilityInterpretation {
            verbal: "almost_certain".to_string(),
            recommendation: "Execute plan".to_string(),
            caveats: vec!["Caveat 1".to_string(), "Caveat 2".to_string()],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ProbabilityInterpretation = serde_json::from_str(&json).unwrap();

        assert_eq!(original.verbal, deserialized.verbal);
        assert_eq!(original.recommendation, deserialized.recommendation);
        assert_eq!(original.caveats.len(), deserialized.caveats.len());
    }

    #[test]
    fn test_probability_interval_round_trip() {
        let original = ProbabilityInterval {
            lower: 0.45,
            upper: 0.65,
            level: 0.90,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ProbabilityInterval = serde_json::from_str(&json).unwrap();

        assert_eq!(original.lower, deserialized.lower);
        assert_eq!(original.upper, deserialized.upper);
        assert_eq!(original.level, deserialized.level);
    }

    #[test]
    fn test_chain_weakness_round_trip() {
        let original = ChainWeakness {
            from: "Premise A".to_string(),
            to: "Conclusion B".to_string(),
            weakness: "Logical leap".to_string(),
            impact: 0.75,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ChainWeakness = serde_json::from_str(&json).unwrap();

        assert_eq!(original.from, deserialized.from);
        assert_eq!(original.to, deserialized.to);
        assert_eq!(original.weakness, deserialized.weakness);
        assert_eq!(original.impact, deserialized.impact);
    }

    #[test]
    fn test_evidence_contradiction_round_trip() {
        let original = EvidenceContradiction {
            evidence_a: "Source 1".to_string(),
            evidence_b: "Source 2".to_string(),
            nature: "Conflicting dates".to_string(),
            resolution: "Use more recent".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EvidenceContradiction = serde_json::from_str(&json).unwrap();

        assert_eq!(original.evidence_a, deserialized.evidence_a);
        assert_eq!(original.evidence_b, deserialized.evidence_b);
        assert_eq!(original.nature, deserialized.nature);
        assert_eq!(original.resolution, deserialized.resolution);
    }

    #[test]
    fn test_gap_round_trip() {
        let original = Gap {
            gap: "Missing control group data".to_string(),
            importance: 0.92,
            suggested_evidence: "Conduct controlled study".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Gap = serde_json::from_str(&json).unwrap();

        assert_eq!(original.gap, deserialized.gap);
        assert_eq!(original.importance, deserialized.importance);
        assert_eq!(original.suggested_evidence, deserialized.suggested_evidence);
    }

    // ========================================================================
    // Builder Chain Tests
    // ========================================================================

    #[test]
    fn test_evidence_params_builder_chain() {
        let params = EvidenceParams::new("Complex claim")
            .with_session("s1")
            .with_context("Context info")
            .with_evidence("E1")
            .with_sourced_evidence("E2", "Source", SourceType::Expert)
            .with_evidence("E3");

        assert_eq!(params.claim, "Complex claim");
        assert_eq!(params.session_id, Some("s1".to_string()));
        assert_eq!(params.context, Some("Context info".to_string()));
        assert_eq!(params.evidence.len(), 3);
        assert_eq!(params.evidence[1].source_type, Some(SourceType::Expert));
    }

    #[test]
    fn test_probabilistic_params_builder_chain() {
        let params = ProbabilisticParams::new("H", 0.5)
            .with_session("sess")
            .with_evidence("E1", 0.9, 0.1)
            .with_evidence("E2", 0.8, 0.2)
            .with_evidence("E3", 0.7, 0.3);

        assert_eq!(params.hypothesis, "H");
        assert_eq!(params.prior, 0.5);
        assert_eq!(params.session_id, Some("sess".to_string()));
        assert_eq!(params.evidence.len(), 3);
        assert_eq!(params.evidence[0].likelihood_if_true, Some(0.9));
        assert_eq!(params.evidence[2].likelihood_if_false, Some(0.3));
    }

    // ========================================================================
    // Clone and Debug Trait Tests
    // ========================================================================

    #[test]
    fn test_source_type_copy() {
        let st1 = SourceType::Primary;
        let st2 = st1; // Copy trait - no clone needed
        assert_eq!(st1, st2);
    }

    #[test]
    fn test_evidence_params_clone() {
        let params1 = EvidenceParams::new("Claim").with_evidence("E1");
        let params2 = params1.clone();
        assert_eq!(params1.claim, params2.claim);
        assert_eq!(params1.evidence.len(), params2.evidence.len());
    }

    #[test]
    fn test_probabilistic_params_clone() {
        let params1 = ProbabilisticParams::new("H", 0.6).with_evidence("E", 0.8, 0.2);
        let params2 = params1.clone();
        assert_eq!(params1.hypothesis, params2.hypothesis);
        assert_eq!(params1.prior, params2.prior);
        assert_eq!(params1.evidence.len(), params2.evidence.len());
    }

    #[test]
    fn test_evidence_result_debug() {
        let result = EvidenceResult {
            assessment_id: "a1".to_string(),
            session_id: "s1".to_string(),
            claim: "Claim".to_string(),
            overall_support: SupportLevel {
                level: "strong".to_string(),
                confidence: 0.9,
                explanation: "Good".to_string(),
            },
            evidence_analyses: vec![],
            chain_analysis: None,
            contradictions: vec![],
            gaps: vec![],
            recommendations: vec![],
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("a1"));
        assert!(debug_str.contains("Claim"));
    }

    // ========================================================================
    // Multiple Evidence Items Tests
    // ========================================================================

    #[test]
    fn test_evidence_params_multiple_evidence_types() {
        let params = EvidenceParams::new("Test")
            .with_evidence("Simple evidence")
            .with_sourced_evidence("Expert opinion", "Dr. Smith", SourceType::Expert)
            .with_sourced_evidence("Data", "Study", SourceType::Statistical);

        assert_eq!(params.evidence.len(), 3);
        assert!(params.evidence[0].source.is_none());
        assert_eq!(params.evidence[1].source, Some("Dr. Smith".to_string()));
        assert_eq!(
            params.evidence[2].source_type,
            Some(SourceType::Statistical)
        );
    }

    #[test]
    fn test_probabilistic_params_multiple_evidence() {
        let params = ProbabilisticParams::new("Hypothesis", 0.3)
            .with_evidence("Strong evidence", 0.95, 0.05)
            .with_evidence("Moderate evidence", 0.7, 0.3)
            .with_evidence("Weak evidence", 0.6, 0.4);

        assert_eq!(params.evidence.len(), 3);
        assert_eq!(params.evidence[0].likelihood_if_true, Some(0.95));
        assert_eq!(params.evidence[1].likelihood_if_true, Some(0.7));
        assert_eq!(params.evidence[2].likelihood_if_false, Some(0.4));
    }

    // ========================================================================
    // Empty and None Value Tests
    // ========================================================================

    #[test]
    fn test_evidence_params_all_none() {
        let params = EvidenceParams {
            claim: "Claim".to_string(),
            evidence: vec![],
            session_id: None,
            context: None,
        };
        assert!(params.session_id.is_none());
        assert!(params.context.is_none());
        assert!(params.evidence.is_empty());
    }

    #[test]
    fn test_bayesian_evidence_all_none() {
        let be = BayesianEvidence {
            description: "Test".to_string(),
            likelihood_if_true: None,
            likelihood_if_false: None,
        };
        assert!(be.likelihood_if_true.is_none());
        assert!(be.likelihood_if_false.is_none());
    }

    #[test]
    fn test_evidence_result_empty_collections() {
        let result = EvidenceResult {
            assessment_id: "a".to_string(),
            session_id: "s".to_string(),
            claim: "c".to_string(),
            overall_support: SupportLevel {
                level: "insufficient".to_string(),
                confidence: 0.1,
                explanation: "No evidence".to_string(),
            },
            evidence_analyses: vec![],
            chain_analysis: None,
            contradictions: vec![],
            gaps: vec![],
            recommendations: vec![],
        };
        assert!(result.evidence_analyses.is_empty());
        assert!(result.contradictions.is_empty());
        assert!(result.gaps.is_empty());
        assert!(result.recommendations.is_empty());
    }

    #[test]
    fn test_probabilistic_result_no_confidence_interval() {
        let result = ProbabilisticResult {
            update_id: "u".to_string(),
            session_id: "s".to_string(),
            hypothesis: "h".to_string(),
            prior: 0.5,
            posterior: 0.6,
            confidence_interval: None,
            update_steps: vec![],
            uncertainty: UncertaintyMetrics {
                entropy_before: 1.0,
                entropy_after: 0.97,
                information_gained: 0.03,
            },
            interpretation: ProbabilityInterpretation {
                verbal: "possible".to_string(),
                recommendation: "wait".to_string(),
                caveats: vec![],
            },
        };
        assert!(result.confidence_interval.is_none());
        assert!(result.update_steps.is_empty());
    }

    // ========================================================================
    // Entropy Edge Cases
    // ========================================================================

    #[test]
    fn test_entropy_boundary_values() {
        fn entropy(p: f64) -> f64 {
            if p <= 0.0 || p >= 1.0 {
                0.0
            } else {
                -(p * p.log2() + (1.0 - p) * (1.0 - p).log2())
            }
        }

        // Test exact boundaries
        assert_eq!(entropy(0.0), 0.0);
        assert_eq!(entropy(1.0), 0.0);

        // Test near boundaries
        let near_zero = entropy(0.001);
        assert!(near_zero > 0.0 && near_zero < 0.1);

        let near_one = entropy(0.999);
        assert!(near_one > 0.0 && near_one < 0.1);

        // Test various probabilities
        let e25 = entropy(0.25);
        let e50 = entropy(0.5);
        let e75 = entropy(0.75);

        // Entropy at 0.5 should be maximum (1.0)
        assert!((e50 - 1.0).abs() < 0.0001);
        // Entropy should be symmetric around 0.5
        assert!((e25 - e75).abs() < 0.0001);
    }

    // ========================================================================
    // Source Type Serialization Edge Cases
    // ========================================================================

    #[test]
    fn test_source_type_all_values_serialize_deserialize() {
        let types = vec![
            SourceType::Primary,
            SourceType::Secondary,
            SourceType::Anecdotal,
            SourceType::Expert,
            SourceType::Statistical,
        ];

        for source_type in types {
            let json = serde_json::to_string(&source_type).unwrap();
            let deserialized: SourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(source_type, deserialized);
        }
    }

    // ========================================================================
    // Complex Nested Structure Tests
    // ========================================================================

    #[test]
    fn test_evidence_result_complex_structure() {
        let result = EvidenceResult {
            assessment_id: "complex-1".to_string(),
            session_id: "sess-complex".to_string(),
            claim: "Complex claim".to_string(),
            overall_support: SupportLevel {
                level: "moderate".to_string(),
                confidence: 0.75,
                explanation: "Mixed evidence".to_string(),
            },
            evidence_analyses: vec![
                EvidenceAnalysis {
                    evidence_id: "e1".to_string(),
                    content_summary: "First".to_string(),
                    relevance: 0.9,
                    credibility: 0.8,
                    weight: 0.72,
                    supports_claim: true,
                    notes: "Strong".to_string(),
                },
                EvidenceAnalysis {
                    evidence_id: "e2".to_string(),
                    content_summary: "Second".to_string(),
                    relevance: 0.7,
                    credibility: 0.6,
                    weight: 0.42,
                    supports_claim: false,
                    notes: "Weak".to_string(),
                },
            ],
            chain_analysis: Some(InferentialChain {
                primary_chain: vec!["A".to_string(), "B".to_string(), "C".to_string()],
                weak_links: vec![ChainWeakness {
                    from: "B".to_string(),
                    to: "C".to_string(),
                    weakness: "Assumption".to_string(),
                    impact: 0.4,
                }],
                redundant_evidence: vec!["e3".to_string()],
            }),
            contradictions: vec![EvidenceContradiction {
                evidence_a: "e1".to_string(),
                evidence_b: "e2".to_string(),
                nature: "Conflict".to_string(),
                resolution: "Prefer e1".to_string(),
            }],
            gaps: vec![Gap {
                gap: "Missing baseline".to_string(),
                importance: 0.8,
                suggested_evidence: "Get baseline".to_string(),
            }],
            recommendations: vec!["Rec 1".to_string(), "Rec 2".to_string()],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: EvidenceResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.evidence_analyses.len(), 2);
        assert_eq!(deserialized.evidence_analyses.len(), 2);
        assert!(result.chain_analysis.is_some());
        assert!(deserialized.chain_analysis.is_some());
        assert_eq!(result.contradictions.len(), 1);
        assert_eq!(result.gaps.len(), 1);
        assert_eq!(result.recommendations.len(), 2);
    }

    #[test]
    fn test_probabilistic_result_complex_structure() {
        let result = ProbabilisticResult {
            update_id: "complex-update".to_string(),
            session_id: "sess".to_string(),
            hypothesis: "Complex hypothesis".to_string(),
            prior: 0.3,
            posterior: 0.8,
            confidence_interval: Some(ProbabilityInterval {
                lower: 0.7,
                upper: 0.9,
                level: 0.95,
            }),
            update_steps: vec![
                BayesianUpdateStep {
                    evidence: "E1".to_string(),
                    prior: 0.3,
                    posterior: 0.5,
                    likelihood_ratio: 2.33,
                },
                BayesianUpdateStep {
                    evidence: "E2".to_string(),
                    prior: 0.5,
                    posterior: 0.8,
                    likelihood_ratio: 6.0,
                },
            ],
            uncertainty: UncertaintyMetrics {
                entropy_before: 0.88,
                entropy_after: 0.72,
                information_gained: 0.16,
            },
            interpretation: ProbabilityInterpretation {
                verbal: "highly_likely".to_string(),
                recommendation: "Act on hypothesis".to_string(),
                caveats: vec!["Caveat 1".to_string(), "Caveat 2".to_string()],
            },
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ProbabilisticResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.update_steps.len(), 2);
        assert_eq!(deserialized.update_steps.len(), 2);
        assert!(result.confidence_interval.is_some());
        assert_eq!(result.interpretation.caveats.len(), 2);
    }
}
