//! Circuit breaker implementation for the self-improvement system.
//!
//! The circuit breaker prevents cascading failures from repeated bad
//! self-improvement actions. It uses the standard closed/open/half-open
//! state machine pattern.
//!
//! # States
//!
//! - **Closed**: Normal operation, actions are allowed
//! - **Open**: Blocking all actions after too many failures
//! - **Half-Open**: Testing recovery, allowing one action

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::config::CircuitBreakerConfig;

/// State of the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    /// Normal operation - actions allowed
    Closed,
    /// Blocking all actions - too many failures
    Open,
    /// Testing recovery - allowing one action
    HalfOpen,
}

impl CircuitState {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        }
    }
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for CircuitState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "closed" => Ok(CircuitState::Closed),
            "open" => Ok(CircuitState::Open),
            "half_open" => Ok(CircuitState::HalfOpen),
            _ => Err(format!("Unknown circuit state: {}", s)),
        }
    }
}

/// Circuit breaker to prevent cascading failures from repeated bad
/// self-improvement actions.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current state
    state: CircuitState,

    /// Number of consecutive failures
    consecutive_failures: u32,

    /// Number of consecutive successes
    consecutive_successes: u32,

    /// Total failures since creation
    total_failures: u32,

    /// Total successes since creation
    total_successes: u32,

    /// Time of last failure
    last_failure: Option<DateTime<Utc>>,

    /// Time of last success
    last_success: Option<DateTime<Utc>>,

    /// Time of last state change
    last_state_change: DateTime<Utc>,

    /// Configuration
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            total_failures: 0,
            total_successes: 0,
            last_failure: None,
            last_success: None,
            last_state_change: Utc::now(),
            config,
        }
    }

    /// Create a circuit breaker from database state.
    #[allow(clippy::too_many_arguments)]
    pub fn from_db_state(
        state: CircuitState,
        consecutive_failures: u32,
        consecutive_successes: u32,
        total_failures: u32,
        total_successes: u32,
        last_failure: Option<DateTime<Utc>>,
        last_success: Option<DateTime<Utc>>,
        last_state_change: DateTime<Utc>,
        config: CircuitBreakerConfig,
    ) -> Self {
        Self {
            state,
            consecutive_failures,
            consecutive_successes,
            total_failures,
            total_successes,
            last_failure,
            last_success,
            last_state_change,
            config,
        }
    }

    /// Check if an action can be executed.
    ///
    /// Returns `true` if the circuit is closed or half-open (after recovery timeout).
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(last_fail) = self.last_failure {
                    let elapsed = Utc::now() - last_fail;
                    let timeout = chrono::Duration::seconds(self.config.recovery_timeout_secs as i64);
                    if elapsed >= timeout {
                        self.transition_to(CircuitState::HalfOpen);
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true, // Allow one test execution
        }
    }

    /// Record a successful action.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.consecutive_successes += 1;
        self.total_successes += 1;
        self.last_success = Some(Utc::now());

        match self.state {
            CircuitState::HalfOpen => {
                if self.consecutive_successes >= self.config.success_threshold {
                    self.transition_to(CircuitState::Closed);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
                tracing::warn!("Success recorded while circuit is open - transitioning to half-open");
                self.transition_to(CircuitState::HalfOpen);
            }
            CircuitState::Closed => {}
        }
    }

    /// Record a failed action.
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.total_failures += 1;
        self.last_failure = Some(Utc::now());

        match self.state {
            CircuitState::Closed => {
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                // Failed during recovery - go back to open
                self.transition_to(CircuitState::Open);
            }
            CircuitState::Open => {}
        }
    }

    /// Transition to a new state.
    fn transition_to(&mut self, new_state: CircuitState) {
        tracing::info!(
            from = %self.state,
            to = %new_state,
            consecutive_failures = self.consecutive_failures,
            consecutive_successes = self.consecutive_successes,
            "Circuit breaker state transition"
        );
        self.state = new_state;
        self.last_state_change = Utc::now();
    }

    /// Get the current state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Get consecutive failures count.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Get consecutive successes count.
    pub fn consecutive_successes(&self) -> u32 {
        self.consecutive_successes
    }

    /// Get total failures count.
    pub fn total_failures(&self) -> u32 {
        self.total_failures
    }

    /// Get total successes count.
    pub fn total_successes(&self) -> u32 {
        self.total_successes
    }

    /// Get last failure time.
    pub fn last_failure(&self) -> Option<DateTime<Utc>> {
        self.last_failure
    }

    /// Get last success time.
    pub fn last_success(&self) -> Option<DateTime<Utc>> {
        self.last_success
    }

    /// Get last state change time.
    pub fn last_state_change(&self) -> DateTime<Utc> {
        self.last_state_change
    }

    /// Get the configuration.
    pub fn config(&self) -> &CircuitBreakerConfig {
        &self.config
    }

    /// Check if the circuit is open (blocking actions).
    pub fn is_open(&self) -> bool {
        self.state == CircuitState::Open
    }

    /// Check if the circuit is closed (allowing actions).
    pub fn is_closed(&self) -> bool {
        self.state == CircuitState::Closed
    }

    /// Get time until recovery attempt (if open).
    pub fn time_until_recovery(&self) -> Option<chrono::Duration> {
        if self.state != CircuitState::Open {
            return None;
        }

        self.last_failure.map(|last_fail| {
            let timeout = chrono::Duration::seconds(self.config.recovery_timeout_secs as i64);
            let elapsed = Utc::now() - last_fail;
            if elapsed >= timeout {
                chrono::Duration::zero()
            } else {
                timeout - elapsed
            }
        })
    }

    /// Manually reset the circuit breaker to closed state.
    pub fn reset(&mut self) {
        tracing::info!(
            from = %self.state,
            "Circuit breaker manually reset to closed"
        );
        self.state = CircuitState::Closed;
        self.consecutive_failures = 0;
        self.consecutive_successes = 0;
        self.last_state_change = Utc::now();
    }

    /// Get a summary of the current state for display.
    pub fn summary(&self) -> CircuitBreakerSummary {
        CircuitBreakerSummary {
            state: self.state,
            consecutive_failures: self.consecutive_failures,
            consecutive_successes: self.consecutive_successes,
            total_failures: self.total_failures,
            total_successes: self.total_successes,
            time_until_recovery: self.time_until_recovery(),
            last_state_change: self.last_state_change,
        }
    }
}

/// Summary of circuit breaker state for display.
#[derive(Debug, Clone)]
pub struct CircuitBreakerSummary {
    /// Current state
    pub state: CircuitState,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// Number of consecutive successes
    pub consecutive_successes: u32,
    /// Total failures
    pub total_failures: u32,
    /// Total successes
    pub total_successes: u32,
    /// Time until recovery attempt (if open)
    pub time_until_recovery: Option<chrono::Duration>,
    /// Time of last state change
    pub last_state_change: DateTime<Utc>,
}

impl std::fmt::Display for CircuitBreakerSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Circuit Breaker: {} ", self.state.to_string().to_uppercase())?;

        match self.state {
            CircuitState::Closed => {
                write!(f, "({} consecutive failures)", self.consecutive_failures)
            }
            CircuitState::Open => {
                if let Some(recovery) = self.time_until_recovery {
                    let secs = recovery.num_seconds();
                    if secs > 60 {
                        write!(f, "(recovery in {}m)", secs / 60)
                    } else {
                        write!(f, "(recovery in {}s)", secs)
                    }
                } else {
                    write!(f, "(recovering soon)")
                }
            }
            CircuitState::HalfOpen => {
                write!(f, "({} consecutive successes needed)",
                    2_u32.saturating_sub(self.consecutive_successes))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            recovery_timeout_secs: 60,
        }
    }

    #[test]
    fn test_initial_state_is_closed() {
        let cb = CircuitBreaker::new(test_config());
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_closed());
        assert!(!cb.is_open());
    }

    #[test]
    fn test_opens_after_threshold_failures() {
        let mut cb = CircuitBreaker::new(test_config());

        // First two failures - still closed
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        // Third failure - opens
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(cb.is_open());
    }

    #[test]
    fn test_success_resets_failure_count() {
        let mut cb = CircuitBreaker::new(test_config());

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.consecutive_failures(), 2);

        cb.record_success();
        assert_eq!(cb.consecutive_failures(), 0);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_can_execute_when_closed() {
        let mut cb = CircuitBreaker::new(test_config());
        assert!(cb.can_execute());
    }

    #[test]
    fn test_cannot_execute_when_open() {
        let mut cb = CircuitBreaker::new(test_config());
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();

        assert!(!cb.can_execute());
    }

    #[test]
    fn test_half_open_closes_after_successes() {
        let mut cb = CircuitBreaker::new(test_config());

        // Open the circuit
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Manually transition to half-open (simulating timeout)
        cb.state = CircuitState::HalfOpen;

        // First success
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Second success - closes
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_reopens_on_failure() {
        let mut cb = CircuitBreaker::new(test_config());

        // Open the circuit
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();

        // Manually transition to half-open
        cb.state = CircuitState::HalfOpen;

        // Failure - reopens
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_reset() {
        let mut cb = CircuitBreaker::new(test_config());
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open());

        cb.reset();
        assert!(cb.is_closed());
        assert_eq!(cb.consecutive_failures(), 0);
    }

    #[test]
    fn test_summary_display() {
        let cb = CircuitBreaker::new(test_config());
        let summary = cb.summary();
        let display = summary.to_string();
        assert!(display.contains("CLOSED"));
    }

    #[test]
    fn test_circuit_state_string_conversion() {
        assert_eq!(CircuitState::Closed.as_str(), "closed");
        assert_eq!(CircuitState::Open.as_str(), "open");
        assert_eq!(CircuitState::HalfOpen.as_str(), "half_open");

        assert_eq!("closed".parse::<CircuitState>().unwrap(), CircuitState::Closed);
        assert_eq!("open".parse::<CircuitState>().unwrap(), CircuitState::Open);
        assert_eq!("half_open".parse::<CircuitState>().unwrap(), CircuitState::HalfOpen);
    }
}
