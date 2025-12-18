//! Configuration management for the MCP server.
//!
//! This module provides configuration structures loaded from environment variables.
//! See [`Config::from_env`] for the main entry point.

use std::env;
use std::path::PathBuf;

use crate::error::AppError;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Langbase API configuration.
    pub langbase: LangbaseConfig,
    /// Database configuration.
    pub database: DatabaseConfig,
    /// Logging configuration.
    pub logging: LoggingConfig,
    /// HTTP request configuration.
    pub request: RequestConfig,
    /// Langbase pipe name configuration.
    pub pipes: PipeConfig,
}

/// Langbase API configuration.
#[derive(Debug, Clone)]
pub struct LangbaseConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Base URL for the Langbase API.
    pub base_url: String,
}

/// Database configuration.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Path to the SQLite database file.
    pub path: PathBuf,
    /// Maximum number of database connections.
    pub max_connections: u32,
}

/// Logging configuration.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Log level (e.g., "info", "debug", "warn").
    pub level: String,
    /// Log output format.
    pub format: LogFormat,
}

/// Log output format.
#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    /// Human-readable pretty format.
    Pretty,
    /// Machine-readable JSON format.
    Json,
}

/// HTTP request configuration.
#[derive(Debug, Clone)]
pub struct RequestConfig {
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Delay between retries in milliseconds.
    pub retry_delay_ms: u64,
}

/// Langbase pipe name configuration.
#[derive(Debug, Clone)]
pub struct PipeConfig {
    /// Pipe name for linear reasoning mode.
    pub linear: String,
    /// Pipe name for tree reasoning mode.
    pub tree: String,
    /// Pipe name for divergent reasoning mode.
    pub divergent: String,
    /// Pipe name for reflection mode.
    pub reflection: String,
    /// Pipe name for auto mode routing.
    pub auto_router: String,
    /// Optional pipe name for auto mode.
    pub auto: Option<String>,
    /// Optional pipe name for backtracking mode.
    pub backtracking: Option<String>,
    /// Optional Graph-of-Thoughts pipe configuration.
    pub got: Option<GotPipeConfig>,
    /// Optional detection pipe configuration.
    pub detection: Option<DetectionPipeConfig>,
    /// Optional decision framework pipe configuration.
    pub decision: Option<DecisionPipeConfig>,
    /// Optional evidence assessment pipe configuration.
    pub evidence: Option<EvidencePipeConfig>,
}

/// Detection pipe configuration for bias and fallacy analysis.
#[derive(Debug, Clone)]
pub struct DetectionPipeConfig {
    /// Pipe name for bias detection.
    pub bias_pipe: Option<String>,
    /// Pipe name for fallacy detection.
    pub fallacy_pipe: Option<String>,
}

/// Graph-of-Thoughts pipe configuration.
#[derive(Debug, Clone)]
pub struct GotPipeConfig {
    /// Pipe name for generating new nodes.
    pub generate_pipe: Option<String>,
    /// Pipe name for scoring nodes.
    pub score_pipe: Option<String>,
    /// Pipe name for aggregating nodes.
    pub aggregate_pipe: Option<String>,
    /// Pipe name for refining nodes.
    pub refine_pipe: Option<String>,
    /// Maximum number of nodes in the graph.
    pub max_nodes: Option<usize>,
    /// Maximum depth of the graph.
    pub max_depth: Option<usize>,
    /// Default number of continuations (k).
    pub default_k: Option<usize>,
    /// Score threshold for pruning nodes.
    pub prune_threshold: Option<f64>,
}

/// Decision framework pipe configuration.
#[derive(Debug, Clone)]
pub struct DecisionPipeConfig {
    /// Pipe name for multi-criteria decision analysis.
    pub decision_pipe: Option<String>,
    /// Pipe name for stakeholder perspective analysis.
    pub perspective_pipe: Option<String>,
}

/// Evidence assessment pipe configuration.
#[derive(Debug, Clone)]
pub struct EvidencePipeConfig {
    /// Pipe name for evidence evaluation.
    pub evidence_pipe: Option<String>,
    /// Pipe name for Bayesian probability updates.
    pub bayesian_pipe: Option<String>,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, AppError> {
        // Load .env file if present (ignore errors if not found)
        let _ = dotenvy::dotenv();

        let langbase = LangbaseConfig {
            api_key: env::var("LANGBASE_API_KEY").map_err(|_| AppError::Config {
                message: "LANGBASE_API_KEY is required".to_string(),
            })?,
            base_url: env::var("LANGBASE_BASE_URL")
                .unwrap_or_else(|_| "https://api.langbase.com".to_string()),
        };

        let database = DatabaseConfig {
            path: PathBuf::from(
                env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/reasoning.db".to_string()),
            ),
            max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
        };

        let logging = LoggingConfig {
            level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            format: match env::var("LOG_FORMAT")
                .unwrap_or_else(|_| "pretty".to_string())
                .to_lowercase()
                .as_str()
            {
                "json" => LogFormat::Json,
                _ => LogFormat::Pretty,
            },
        };

        let request = RequestConfig {
            timeout_ms: env::var("REQUEST_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30000),
            max_retries: env::var("MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            retry_delay_ms: env::var("RETRY_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
        };

        // Build GoT pipe config if any GoT env vars are set
        let got_config = {
            let generate = env::var("PIPE_GOT_GENERATE").ok();
            let score = env::var("PIPE_GOT_SCORE").ok();
            let aggregate = env::var("PIPE_GOT_AGGREGATE").ok();
            let refine = env::var("PIPE_GOT_REFINE").ok();
            let max_nodes = env::var("GOT_MAX_NODES").ok().and_then(|s| s.parse().ok());
            let max_depth = env::var("GOT_MAX_DEPTH").ok().and_then(|s| s.parse().ok());
            let default_k = env::var("GOT_DEFAULT_K").ok().and_then(|s| s.parse().ok());
            let prune_threshold = env::var("GOT_PRUNE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok());

            // Only create config if any value is set
            if generate.is_some()
                || score.is_some()
                || aggregate.is_some()
                || refine.is_some()
                || max_nodes.is_some()
                || max_depth.is_some()
                || default_k.is_some()
                || prune_threshold.is_some()
            {
                Some(GotPipeConfig {
                    generate_pipe: generate,
                    score_pipe: score,
                    aggregate_pipe: aggregate,
                    refine_pipe: refine,
                    max_nodes,
                    max_depth,
                    default_k,
                    prune_threshold,
                })
            } else {
                None
            }
        };

        // Build detection pipe config if any detection env vars are set
        let detection_config = {
            let bias_pipe = env::var("PIPE_DETECT_BIASES").ok();
            let fallacy_pipe = env::var("PIPE_DETECT_FALLACIES").ok();

            // Only create config if any value is set
            if bias_pipe.is_some() || fallacy_pipe.is_some() {
                Some(DetectionPipeConfig {
                    bias_pipe,
                    fallacy_pipe,
                })
            } else {
                None
            }
        };

        // Build decision pipe config if any decision env vars are set
        let decision_config = {
            let decision_pipe = env::var("PIPE_DECISION").ok();
            let perspective_pipe = env::var("PIPE_PERSPECTIVE").ok();

            if decision_pipe.is_some() || perspective_pipe.is_some() {
                Some(DecisionPipeConfig {
                    decision_pipe,
                    perspective_pipe,
                })
            } else {
                None
            }
        };

        // Build evidence pipe config if any evidence env vars are set
        let evidence_config = {
            let evidence_pipe = env::var("PIPE_EVIDENCE").ok();
            let bayesian_pipe = env::var("PIPE_BAYESIAN").ok();

            if evidence_pipe.is_some() || bayesian_pipe.is_some() {
                Some(EvidencePipeConfig {
                    evidence_pipe,
                    bayesian_pipe,
                })
            } else {
                None
            }
        };

        let pipes = PipeConfig {
            linear: env::var("PIPE_LINEAR").unwrap_or_else(|_| "linear-reasoning-v1".to_string()),
            tree: env::var("PIPE_TREE").unwrap_or_else(|_| "tree-reasoning-v1".to_string()),
            divergent: env::var("PIPE_DIVERGENT")
                .unwrap_or_else(|_| "divergent-reasoning-v1".to_string()),
            reflection: env::var("PIPE_REFLECTION").unwrap_or_else(|_| "reflection-v1".to_string()),
            auto_router: env::var("PIPE_AUTO").unwrap_or_else(|_| "mode-router-v1".to_string()),
            auto: env::var("PIPE_AUTO").ok(),
            backtracking: env::var("PIPE_BACKTRACKING").ok(),
            got: got_config,
            detection: detection_config,
            decision: decision_config,
            evidence: evidence_config,
        };

        Ok(Config {
            langbase,
            database,
            logging,
            request,
            pipes,
        })
    }
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl Default for PipeConfig {
    fn default() -> Self {
        Self {
            linear: "linear-reasoning-v1".to_string(),
            tree: "tree-reasoning-v1".to_string(),
            divergent: "divergent-reasoning-v1".to_string(),
            reflection: "reflection-v1".to_string(),
            auto_router: "mode-router-v1".to_string(),
            auto: None,
            backtracking: None,
            got: None,
            detection: None,
            decision: None,
            evidence: None,
        }
    }
}

impl Default for DetectionPipeConfig {
    fn default() -> Self {
        Self {
            bias_pipe: Some("detect-biases-v1".to_string()),
            fallacy_pipe: Some("detect-fallacies-v1".to_string()),
        }
    }
}

impl Default for GotPipeConfig {
    fn default() -> Self {
        Self {
            generate_pipe: Some("got-generate-v1".to_string()),
            score_pipe: Some("got-score-v1".to_string()),
            aggregate_pipe: Some("got-aggregate-v1".to_string()),
            refine_pipe: Some("got-refine-v1".to_string()),
            max_nodes: Some(100),
            max_depth: Some(10),
            default_k: Some(3),
            prune_threshold: Some(0.3),
        }
    }
}

impl Default for DecisionPipeConfig {
    fn default() -> Self {
        Self {
            decision_pipe: Some("decision-maker-v1".to_string()),
            perspective_pipe: Some("perspective-analyzer-v1".to_string()),
        }
    }
}

impl Default for EvidencePipeConfig {
    fn default() -> Self {
        Self {
            evidence_pipe: Some("evidence-assessor-v1".to_string()),
            bayesian_pipe: Some("bayesian-updater-v1".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_config_default() {
        let config = RequestConfig::default();
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_pipe_config_default() {
        let config = PipeConfig::default();
        assert_eq!(config.linear, "linear-reasoning-v1");
        assert_eq!(config.tree, "tree-reasoning-v1");
        assert_eq!(config.divergent, "divergent-reasoning-v1");
        assert_eq!(config.reflection, "reflection-v1");
        assert_eq!(config.auto_router, "mode-router-v1");
        assert!(config.auto.is_none());
        assert!(config.backtracking.is_none());
        assert!(config.got.is_none());
        assert!(config.detection.is_none());
        assert!(config.decision.is_none());
        assert!(config.evidence.is_none());
    }

    #[test]
    fn test_detection_pipe_config_default() {
        let config = DetectionPipeConfig::default();
        assert_eq!(config.bias_pipe, Some("detect-biases-v1".to_string()));
        assert_eq!(config.fallacy_pipe, Some("detect-fallacies-v1".to_string()));
    }

    #[test]
    fn test_got_pipe_config_default() {
        let config = GotPipeConfig::default();
        assert_eq!(config.generate_pipe, Some("got-generate-v1".to_string()));
        assert_eq!(config.score_pipe, Some("got-score-v1".to_string()));
        assert_eq!(config.aggregate_pipe, Some("got-aggregate-v1".to_string()));
        assert_eq!(config.refine_pipe, Some("got-refine-v1".to_string()));
        assert_eq!(config.max_nodes, Some(100));
        assert_eq!(config.max_depth, Some(10));
        assert_eq!(config.default_k, Some(3));
        assert_eq!(config.prune_threshold, Some(0.3));
    }

    #[test]
    fn test_log_format_variants() {
        assert_eq!(LogFormat::Pretty, LogFormat::Pretty);
        assert_eq!(LogFormat::Json, LogFormat::Json);
        assert_ne!(LogFormat::Pretty, LogFormat::Json);
    }

    #[test]
    fn test_decision_pipe_config_default() {
        let config = DecisionPipeConfig::default();
        assert_eq!(config.decision_pipe, Some("decision-maker-v1".to_string()));
        assert_eq!(
            config.perspective_pipe,
            Some("perspective-analyzer-v1".to_string())
        );
    }

    #[test]
    fn test_evidence_pipe_config_default() {
        let config = EvidencePipeConfig::default();
        assert_eq!(
            config.evidence_pipe,
            Some("evidence-assessor-v1".to_string())
        );
        assert_eq!(config.bayesian_pipe, Some("bayesian-updater-v1".to_string()));
    }

    // Note: Config::from_env() tests are in tests/config_env_test.rs
    // because they require serial execution and full env var control.
    // Unit tests here focus on Default impls and type behavior.
}
