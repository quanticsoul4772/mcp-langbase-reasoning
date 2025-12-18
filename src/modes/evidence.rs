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
use tracing::{debug, info, warn};

use super::{extract_json_from_completion, serialize_for_log};
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
    /// Storage backend for persisting data.
    storage: SqliteStorage,
    /// Langbase client for LLM-powered reasoning.
    langbase: LangbaseClient,
    /// The Langbase pipe name for evidence assessment.
    evidence_pipe: String,
    /// The Langbase pipe name for Bayesian updates.
    bayesian_pipe: String,
}

impl EvidenceMode {
    /// Create a new evidence mode handler.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        Self {
            storage,
            langbase,
            evidence_pipe: config
                .pipes
                .evidence
                .as_ref()
                .and_then(|e| e.evidence_pipe.clone())
                .unwrap_or_else(|| "evidence-assessor-v1".to_string()),
            bayesian_pipe: config
                .pipes
                .evidence
                .as_ref()
                .and_then(|e| e.bayesian_pipe.clone())
                .unwrap_or_else(|| "bayesian-updater-v1".to_string()),
        }
    }

    /// Assess evidence for a claim.
    pub async fn assess_evidence(&self, params: EvidenceParams) -> AppResult<EvidenceResult> {
        let start = Instant::now();

        // Validate input
        self.validate_evidence_params(&params)?;

        // Get or create session
        let session = self
            .storage
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
        .with_pipe(&self.evidence_pipe);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.evidence_pipe, messages);
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
            stored_evidence =
                stored_evidence.with_chain_analysis(serde_json::to_value(chain).unwrap_or_default());
        }

        if let Err(e) = self.storage.create_evidence_assessment(&stored_evidence).await {
            warn!(
                error = %e,
                assessment_id = %assessment_id,
                "Failed to persist evidence assessment to storage"
            );
        }

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.assess_evidence output"),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

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
            .storage
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
        .with_pipe(&self.bayesian_pipe);

        // Call Langbase pipe
        let request = PipeRequest::new(&self.bayesian_pipe, messages);
        let response = match self.langbase.call_pipe(request).await {
            Ok(resp) => resp,
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                invocation = invocation.failure(e.to_string(), latency);
                self.storage.log_invocation(&invocation).await?;

                // Fallback: calculate locally
                warn!(error = %e, "Langbase call failed, using local Bayesian calculation");
                let (posterior, steps) = self.calculate_bayesian_update(&params);
                let entropy_before = self.calculate_entropy(params.prior);
                let entropy_after = self.calculate_entropy(posterior);

                let update_id = uuid::Uuid::new_v4().to_string();
                return Ok(ProbabilisticResult {
                    update_id,
                    session_id: session.id,
                    hypothesis: params.hypothesis,
                    prior: params.prior,
                    posterior,
                    confidence_interval: None,
                    update_steps: steps,
                    uncertainty: UncertaintyMetrics {
                        entropy_before,
                        entropy_after,
                        information_gained: entropy_before - entropy_after,
                    },
                    interpretation: ProbabilityInterpretation {
                        verbal: self.probability_to_verbal(posterior),
                        recommendation: "Local calculation used due to API unavailability".to_string(),
                        caveats: vec!["Likelihood ratios may be estimates".to_string()],
                    },
                });
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
            stored_probability =
                stored_probability.with_confidence_interval(Some(ci.lower), Some(ci.upper), Some(ci.level));
        }

        if let Err(e) = self.storage.create_probability_update(&stored_probability).await {
            warn!(
                error = %e,
                update_id = %update_id,
                "Failed to persist probability update to storage"
            );
        }

        // Log successful invocation
        let latency = start.elapsed().as_millis() as i64;
        invocation = invocation.success(
            serialize_for_log(&result, "reasoning.probabilistic output"),
            latency,
        );
        self.storage.log_invocation(&invocation).await?;

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
        params: &ProbabilisticParams,
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

        serde_json::from_str::<BayesianResponse>(json_str).or_else(|e| {
            warn!(error = %e, "Failed to parse Bayesian response, using local calculation");
            // Fallback to local calculation
            let (posterior, steps) = self.calculate_bayesian_update(params);
            let entropy_before = self.calculate_entropy(params.prior);
            let entropy_after = self.calculate_entropy(posterior);

            Ok(BayesianResponse {
                prior: params.prior,
                posterior,
                confidence_interval: None,
                update_steps: steps
                    .into_iter()
                    .map(|s| UpdateStepResponse {
                        evidence: s.evidence,
                        prior_before: s.prior,
                        likelihood_ratio: s.likelihood_ratio,
                        posterior_after: s.posterior,
                        explanation: String::new(),
                    })
                    .collect(),
                uncertainty_analysis: Some(UncertaintyAnalysis {
                    entropy_before,
                    entropy_after,
                    information_gained: entropy_before - entropy_after,
                    remaining_uncertainty: "Calculated locally".to_string(),
                }),
                sensitivity: None,
                interpretation: Interpretation {
                    verbal_probability: self.probability_to_verbal(posterior),
                    recommendation: "Based on provided likelihood ratios".to_string(),
                    caveats: vec![],
                },
            })
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

    /// Perform local Bayesian update calculation.
    fn calculate_bayesian_update(
        &self,
        params: &ProbabilisticParams,
    ) -> (f64, Vec<BayesianUpdateStep>) {
        let mut current_prior = params.prior;
        let mut steps = Vec::new();

        for evidence in &params.evidence {
            let likelihood_true = evidence.likelihood_if_true.unwrap_or(0.7);
            let likelihood_false = evidence.likelihood_if_false.unwrap_or(0.3);

            // Avoid division by zero
            let denominator =
                likelihood_true * current_prior + likelihood_false * (1.0 - current_prior);
            let posterior = if denominator > 0.0 {
                (likelihood_true * current_prior) / denominator
            } else {
                current_prior
            };

            let likelihood_ratio = if likelihood_false > 0.0 {
                likelihood_true / likelihood_false
            } else {
                likelihood_true * 10.0 // High ratio if false likelihood is 0
            };

            steps.push(BayesianUpdateStep {
                evidence: evidence.description.clone(),
                prior: current_prior,
                posterior,
                likelihood_ratio,
            });

            current_prior = posterior;
        }

        (current_prior, steps)
    }

    /// Convert probability to verbal description.
    fn probability_to_verbal(&self, p: f64) -> String {
        match p {
            p if p >= 0.95 => "almost_certain".to_string(),
            p if p >= 0.85 => "highly_likely".to_string(),
            p if p >= 0.70 => "likely".to_string(),
            p if p >= 0.50 => "possible".to_string(),
            p if p >= 0.30 => "unlikely".to_string(),
            p if p >= 0.15 => "highly_unlikely".to_string(),
            _ => "almost_impossible".to_string(),
        }
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
        assert_eq!(params.evidence[0].source, Some("Research paper".to_string()));
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
}
