# Self-Improvement Pipe Integration Strategy

## Executive Summary

After analysis using Graph-of-Thoughts, Tree reasoning, Divergent thinking, and Decision analysis, the recommended strategy is:

**START WITH EXISTING PIPES** (v1.0) → **SPECIALIZE ONLY IF NEEDED** (v1.1+)

This approach scored highest on development speed (0.9) while maintaining good effectiveness (0.85).

---

## 1. Decision Analysis Results

| Option | Score | Rank |
|--------|-------|------|
| Hybrid: One new diagnosis pipe + existing | 0.775 | 1 |
| Use ONLY existing pipes with tailored prompts | 0.770 | 2 |
| Create self-improvement preset workflow | 0.720 | 3 |
| Create dedicated self-improvement-v1 pipe | 0.700 | 4 |

**Key Insight**: The difference between Hybrid (0.775) and Existing-Only (0.770) is minimal. Given the maintenance burden trade-off, we recommend **starting with existing pipes** and only creating new ones if metrics indicate a need.

---

## 2. Pipe Usage by Phase

### Phase: MONITOR (No Pipe Needed)
```
Input: SQLite invocations table
Output: HealthReport, DetectedTrigger[]

No Langbase pipe required - purely metrics-based.
```

### Phase: ANALYZER - Diagnosis Generation
```
Pipe: reflection-v1
Purpose: Analyze system metrics and identify root cause

Prompt Template:
───────────────────────────────────────────────────────────
You are analyzing the health of an MCP reasoning server.

CURRENT METRICS:
- Error Rate: {error_rate:.2%} (baseline: {baseline_error:.2%}, threshold: {threshold_error:.2%})
- Latency P95: {latency_p95}ms (baseline: {baseline_latency}ms, threshold: {threshold_latency}ms)
- Quality Score: {quality:.2f} (baseline: {baseline_quality:.2f}, minimum: {min_quality:.2f})
- Fallback Rate: {fallback_rate:.2%} (baseline: {baseline_fallback:.2%})

RECENT FAILURES (last hour):
{failure_summary}

TRIGGERED BY: {trigger_metric} exceeded threshold

Analyze the root cause and provide a diagnosis in this exact JSON format:
{
  "suspected_cause": "brief description of likely root cause",
  "severity": "info|warning|high|critical",
  "confidence": 0.0-1.0,
  "evidence": ["list", "of", "supporting", "observations"],
  "recommended_action_type": "adjust_param|toggle_feature|restart_service|clear_cache|scale_resource|no_op",
  "action_target": "specific parameter or resource name if applicable",
  "rationale": "why this action should help"
}
───────────────────────────────────────────────────────────
```

### Phase: ANALYZER - Action Selection
```
Pipe: decision-framework-v1
Purpose: Select optimal action from allowlist

Prompt Template:
───────────────────────────────────────────────────────────
You are selecting the optimal self-improvement action for an MCP server.

DIAGNOSIS:
{diagnosis_json}

AVAILABLE ACTIONS (from allowlist):
{allowlist_summary}

HISTORICAL EFFECTIVENESS:
{action_history_summary}

CONSTRAINTS:
- Actions must be reversible (except cache clear, restart)
- Maximum change per action is limited by step size
- System is currently in: {system_state} (normal/degraded/critical)

Evaluate options using these criteria:
1. Expected Impact (weight: 0.4) - How much improvement is likely?
2. Risk Level (weight: 0.3) - What could go wrong?
3. Historical Success (weight: 0.2) - Has this worked before?
4. Reversibility (weight: 0.1) - Can we undo this?

Respond in this exact JSON format:
{
  "question": "Which action best addresses {trigger_metric}?",
  "selected_option": "exact action from allowlist",
  "scores": {
    "expected_impact": 0.0-1.0,
    "risk_level": 0.0-1.0,
    "historical_success": 0.0-1.0,
    "reversibility": 0.0-1.0
  },
  "total_score": 0.0-1.0,
  "rationale": "explanation of selection",
  "alternatives_considered": ["other", "options", "evaluated"]
}
───────────────────────────────────────────────────────────
```

### Phase: EXECUTOR - Validate Decision (Optional)
```
Pipe: detection-v1
Purpose: Check our own reasoning for biases/fallacies
When: Configurable - can skip for speed or always run for safety

Prompt Template:
───────────────────────────────────────────────────────────
Analyze this self-improvement decision for cognitive biases or logical fallacies:

DIAGNOSIS:
{diagnosis_summary}

SELECTED ACTION:
{action_summary}

RATIONALE:
{rationale}

HISTORICAL CONTEXT:
- Last 3 actions taken: {recent_actions}
- Current circuit breaker state: {circuit_state}

Check for:
1. Confirmation bias - Are we only seeing what we want to see?
2. Anchoring - Are we over-relying on the first metric we noticed?
3. Recency bias - Are we overweighting recent events?
4. Sunk cost fallacy - Are we continuing a failing strategy?
5. Availability heuristic - Are we choosing familiar actions over better ones?

Respond in JSON:
{
  "biases_detected": [
    {"type": "bias_name", "severity": 1-5, "evidence": "...", "remediation": "..."}
  ],
  "fallacies_detected": [
    {"type": "fallacy_name", "severity": 1-5, "evidence": "...", "remediation": "..."}
  ],
  "overall_quality": 0.0-1.0,
  "should_proceed": true|false,
  "warnings": ["any", "concerns"]
}
───────────────────────────────────────────────────────────
```

### Phase: LEARNER - Outcome Synthesis
```
Pipe: reflection-v1
Purpose: Learn from action outcomes

Prompt Template:
───────────────────────────────────────────────────────────
Analyze the outcome of this self-improvement action:

ACTION TAKEN:
{action_summary}

METRICS BEFORE:
- Error Rate: {pre_error:.2%}
- Latency P95: {pre_latency}ms
- Quality Score: {pre_quality:.2f}

METRICS AFTER (stabilization period: {stabilization_mins} minutes):
- Error Rate: {post_error:.2%}
- Latency P95: {post_latency}ms
- Quality Score: {post_quality:.2f}

CALCULATED REWARD:
- Error improvement: {error_reward:+.3f}
- Latency improvement: {latency_reward:+.3f}
- Quality improvement: {quality_reward:+.3f}
- Composite reward: {total_reward:+.3f}

OUTCOME: {outcome} (success/failed/rolled_back)
{rollback_reason}

Synthesize lessons learned in JSON:
{
  "outcome_assessment": "brief summary of what happened",
  "root_cause_accuracy": 0.0-1.0,
  "action_effectiveness": 0.0-1.0,
  "lessons": [
    "specific actionable insight 1",
    "specific actionable insight 2"
  ],
  "recommendations": {
    "repeat_action": true|false,
    "adjust_parameters": {...},
    "avoid_patterns": ["pattern to avoid"]
  },
  "confidence": 0.0-1.0
}
───────────────────────────────────────────────────────────
```

---

## 3. Pipe Call Sequence

```
Normal Flow (4 API calls):
┌─────────┐     ┌───────────────┐     ┌────────────────────┐     ┌─────────────┐
│ Monitor │────▶│ reflection-v1 │────▶│ decision-framework │────▶│ [Execute]   │
│ (local) │     │ (diagnosis)   │     │ (action select)    │     │ (local)     │
└─────────┘     └───────────────┘     └────────────────────┘     └──────┬──────┘
                                                                        │
                                                                        ▼
                                                                 ┌─────────────┐
                                                                 │ reflection-v1│
                                                                 │ (learning)   │
                                                                 └─────────────┘

With Validation (5 API calls):
┌─────────┐     ┌───────────────┐     ┌────────────────────┐     ┌─────────────┐
│ Monitor │────▶│ reflection-v1 │────▶│ decision-framework │────▶│ detection-v1│
│ (local) │     │ (diagnosis)   │     │ (action select)    │     │ (validate)  │
└─────────┘     └───────────────┘     └────────────────────┘     └──────┬──────┘
                                                                        │
                                                           ┌────────────┴────────────┐
                                                           │ should_proceed == true? │
                                                           └────────────┬────────────┘
                                                                        │
                                                   ┌────────────────────┴───────────────────┐
                                                   ▼                                        ▼
                                            ┌─────────────┐                          ┌───────────┐
                                            │ [Execute]   │                          │ [Abort]   │
                                            └──────┬──────┘                          └───────────┘
                                                   │
                                                   ▼
                                            ┌─────────────┐
                                            │ reflection-v1│
                                            │ (learning)   │
                                            └─────────────┘
```

---

## 4. When to Create New Pipes

### Trigger Criteria (create new pipe if ANY are true)

| Criterion | Threshold | Measurement |
|-----------|-----------|-------------|
| Diagnosis JSON parse failure rate | > 20% over 50+ diagnoses | `diagnosis_parse_failures / total_diagnoses` |
| Root cause accuracy | < 50% when validated against outcomes | Manual review of predicted vs actual |
| Diagnosis latency | > 10 seconds average | `AVG(diagnosis_latency_ms)` |
| Action selection quality | < 40% positive reward rate | `positive_rewards / total_actions` |

### Success Metrics (stay with existing pipes if ALL are met)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Diagnosis parse success | > 80% | JSON parsing success rate |
| Positive action outcomes | > 60% | Actions with reward > 0 |
| Bias detection catches | > 10% | Decisions flagged by detection-v1 |
| Learning insights actionable | > 70% | Manual review |

### If New Pipe Needed: self-diagnosis-v1 Specification

```yaml
name: self-diagnosis-v1
description: Specialized system health diagnosis for MCP server self-improvement
model: gpt-4o-mini  # or claude-3-haiku for cost efficiency

system_message: |
  You are a specialized diagnostic system for an MCP (Model Context Protocol)
  reasoning server. Your role is to analyze system health metrics and identify
  root causes of performance degradation.

  You understand:
  - Error rates and their relationship to API reliability
  - Latency patterns and their causes (network, model, database)
  - Quality scores from reasoning modes (reflection, detection, etc.)
  - Fallback usage as an indicator of pipe reliability

  You always respond in valid JSON matching the provided schema.
  You are conservative - prefer "no_op" over risky changes.
  You consider historical patterns when diagnosing issues.

output_schema:
  type: object
  required: [suspected_cause, severity, confidence, recommended_action]
  properties:
    suspected_cause:
      type: string
    severity:
      type: string
      enum: [info, warning, high, critical]
    confidence:
      type: number
      minimum: 0
      maximum: 1
    evidence:
      type: array
      items:
        type: string
    recommended_action:
      type: object
      required: [type, rationale]
      properties:
        type:
          type: string
          enum: [adjust_param, toggle_feature, restart_service, clear_cache, scale_resource, no_op]
        target:
          type: string
        value:
          type: [string, number, boolean]
        rationale:
          type: string
```

---

## 5. Implementation in Rust

### Pipe Client Extension

```rust
// src/self_improvement/pipes.rs

use crate::langbase::LangbaseClient;

/// Self-improvement pipe operations using existing pipes
pub struct SelfImprovementPipes {
    langbase: LangbaseClient,
    config: PipeConfig,
}

impl SelfImprovementPipes {
    /// Generate diagnosis using reflection-v1
    pub async fn generate_diagnosis(
        &self,
        health_report: &HealthReport,
        trigger: &DetectedTrigger,
    ) -> Result<DiagnosisResponse, PipeError> {
        let prompt = self.build_diagnosis_prompt(health_report, trigger);

        let response = self.langbase
            .call_pipe(&self.config.reflection_pipe, prompt)
            .await?;

        // Parse JSON from response
        let diagnosis: DiagnosisResponse = serde_json::from_str(
            &extract_json(&response.completion)?
        ).map_err(|e| PipeError::ParseFailed {
            pipe: "reflection-v1".into(),
            error: e.to_string(),
        })?;

        Ok(diagnosis)
    }

    /// Select action using decision-framework-v1
    pub async fn select_action(
        &self,
        diagnosis: &SelfDiagnosis,
        allowlist: &ActionAllowlist,
        history: &[ActionEffectiveness],
    ) -> Result<ActionSelectionResponse, PipeError> {
        let prompt = self.build_action_selection_prompt(diagnosis, allowlist, history);

        let response = self.langbase
            .call_pipe(&self.config.decision_pipe, prompt)
            .await?;

        let selection: ActionSelectionResponse = serde_json::from_str(
            &extract_json(&response.completion)?
        )?;

        Ok(selection)
    }

    /// Validate decision using detection-v1 (optional)
    pub async fn validate_decision(
        &self,
        diagnosis: &SelfDiagnosis,
        action: &SuggestedAction,
        context: &ValidationContext,
    ) -> Result<ValidationResponse, PipeError> {
        let prompt = self.build_validation_prompt(diagnosis, action, context);

        let response = self.langbase
            .call_pipe(&self.config.detection_pipe, prompt)
            .await?;

        let validation: ValidationResponse = serde_json::from_str(
            &extract_json(&response.completion)?
        )?;

        Ok(validation)
    }

    /// Synthesize learning using reflection-v1
    pub async fn synthesize_learning(
        &self,
        action: &ExecutedAction,
        pre_metrics: &MetricsSnapshot,
        post_metrics: &MetricsSnapshot,
        reward: &NormalizedReward,
    ) -> Result<LearningResponse, PipeError> {
        let prompt = self.build_learning_prompt(action, pre_metrics, post_metrics, reward);

        let response = self.langbase
            .call_pipe(&self.config.reflection_pipe, prompt)
            .await?;

        let learning: LearningResponse = serde_json::from_str(
            &extract_json(&response.completion)?
        )?;

        Ok(learning)
    }
}
```

### Response Types

```rust
#[derive(Debug, Deserialize)]
pub struct DiagnosisResponse {
    pub suspected_cause: String,
    pub severity: String,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub recommended_action_type: String,
    pub action_target: Option<String>,
    pub rationale: String,
}

#[derive(Debug, Deserialize)]
pub struct ActionSelectionResponse {
    pub selected_option: String,
    pub scores: ActionScores,
    pub total_score: f64,
    pub rationale: String,
    pub alternatives_considered: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ValidationResponse {
    pub biases_detected: Vec<BiasDetection>,
    pub fallacies_detected: Vec<FallacyDetection>,
    pub overall_quality: f64,
    pub should_proceed: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LearningResponse {
    pub outcome_assessment: String,
    pub root_cause_accuracy: f64,
    pub action_effectiveness: f64,
    pub lessons: Vec<String>,
    pub recommendations: LearningRecommendations,
    pub confidence: f64,
}
```

---

## 6. Configuration

```rust
// Add to SelfImprovementConfig

#[derive(Debug, Clone)]
pub struct PipeConfig {
    /// Pipe for diagnosis generation (default: reflection-v1)
    pub diagnosis_pipe: String,

    /// Pipe for action selection (default: decision-framework-v1)
    pub decision_pipe: String,

    /// Pipe for decision validation (default: detection-v1)
    pub detection_pipe: String,

    /// Pipe for learning synthesis (default: reflection-v1)
    pub learning_pipe: String,

    /// Whether to run validation step
    pub enable_validation: bool,

    /// Timeout for pipe calls
    pub pipe_timeout_ms: u64,
}

impl Default for PipeConfig {
    fn default() -> Self {
        Self {
            diagnosis_pipe: "reflection-v1".to_string(),
            decision_pipe: "decision-framework-v1".to_string(),
            detection_pipe: "detection-v1".to_string(),
            learning_pipe: "reflection-v1".to_string(),
            enable_validation: true,
            pipe_timeout_ms: 30000,
        }
    }
}
```

---

## 7. Fallback Strategy

If Langbase is unavailable during self-improvement:

```rust
impl SelfImprovementPipes {
    async fn with_fallback<T, F>(&self, pipe_call: F) -> Result<T, PipeError>
    where
        F: Future<Output = Result<T, PipeError>>,
        T: Default,
    {
        match tokio::time::timeout(
            Duration::from_millis(self.config.pipe_timeout_ms),
            pipe_call
        ).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "Pipe call failed, using fallback");
                // For self-improvement, safe fallback is NO_OP
                Err(PipeError::Unavailable {
                    pipe: "unknown".into(),
                    fallback_used: true
                })
            }
            Err(_) => {
                tracing::warn!("Pipe call timed out, using fallback");
                Err(PipeError::Timeout {
                    pipe: "unknown".into(),
                    timeout_ms: self.config.pipe_timeout_ms,
                })
            }
        }
    }
}
```

**Critical**: If diagnosis pipe fails, the self-improvement system should **not** take any action. This prevents blind changes when we can't properly analyze the situation.

---

## 8. Metrics to Track

```sql
-- Add to invocations or new table
ALTER TABLE self_improvement_actions ADD COLUMN diagnosis_pipe TEXT;
ALTER TABLE self_improvement_actions ADD COLUMN diagnosis_latency_ms INTEGER;
ALTER TABLE self_improvement_actions ADD COLUMN diagnosis_parse_success INTEGER;
ALTER TABLE self_improvement_actions ADD COLUMN action_pipe TEXT;
ALTER TABLE self_improvement_actions ADD COLUMN action_latency_ms INTEGER;
ALTER TABLE self_improvement_actions ADD COLUMN validation_performed INTEGER;
ALTER TABLE self_improvement_actions ADD COLUMN validation_passed INTEGER;
```

---

## 9. Reasoning Sessions Used

| Tool | Session ID | Key Insight |
|------|------------|-------------|
| GoT | `35fdffd6-dceb-48c9-8b1c-92be188826e7` | Integrate existing pipes with tailored prompts (0.9 confidence) |
| Tree | `8552300c-c5a0-4fb4-99d2-cdb959d615b8` | Hybrid approach recommended (0.85 confidence) |
| Divergent | `f8169542-91a0-4e1b-bbbc-9436831b75a4` | Can compose existing pipes into workflow (novelty: 0.85) |
| Decision | `08b10a00-0d22-4145-b0f2-40e357436f60` | Hybrid vs Existing-Only nearly tied (0.775 vs 0.770) |
| Linear | `a13088ba-cc5f-4fbf-8570-fba4725310ae` | Mapped exact pipe usage per phase (0.9 confidence) |
