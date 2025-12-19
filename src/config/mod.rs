//! Configuration management for the MCP server.
//!
//! This module provides configuration structures loaded from environment variables.
//! See [`Config::from_env`] for the main entry point.

use std::env;
use std::path::PathBuf;

use tracing::{debug, warn};

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
    /// Error handling behavior configuration.
    pub error_handling: ErrorHandlingConfig,
}

/// Error handling behavior configuration for strict mode.
#[derive(Debug, Clone)]
pub struct ErrorHandlingConfig {
    /// When true, parsing errors return Err instead of fallback values.
    /// Recommended for integration testing to catch actual failures.
    pub strict_mode: bool,

    /// When true, API failures return Err instead of local calculations.
    /// Recommended to ensure pipe coverage and detect integration issues.
    pub require_pipe_response: bool,

    /// Maximum number of fallback usages before forcing error.
    /// 0 = unlimited (default behavior), >0 = limit per session.
    pub max_fallback_count: u32,
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
    /// Consolidated pipe name for all detection operations (prompts passed dynamically).
    pub pipe: Option<String>,
}

/// Graph-of-Thoughts pipe configuration.
#[derive(Debug, Clone)]
pub struct GotPipeConfig {
    /// Consolidated pipe name for all GoT operations (prompts passed dynamically).
    pub pipe: Option<String>,
    /// Maximum number of nodes in the graph.
    pub max_nodes: Option<usize>,
    /// Maximum depth of the graph.
    pub max_depth: Option<usize>,
    /// Default number of continuations (k).
    pub default_k: Option<usize>,
    /// Score threshold for pruning nodes.
    pub prune_threshold: Option<f64>,
}

/// Decision framework pipe configuration (consolidated - prompts passed dynamically).
#[derive(Debug, Clone)]
pub struct DecisionPipeConfig {
    /// Consolidated pipe name for decision analysis operations.
    pub pipe: Option<String>,
}

/// Evidence assessment pipe configuration (consolidated - prompts passed dynamically).
#[derive(Debug, Clone)]
pub struct EvidencePipeConfig {
    /// Consolidated pipe name for evidence assessment operations.
    pub pipe: Option<String>,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, AppError> {
        // Load .env file if present, with discriminated error handling
        match dotenvy::dotenv() {
            Ok(path) => {
                debug!(path = %path.display(), "Loaded .env file");
            }
            Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                // .env file not found - this is normal, use environment variables
                debug!("No .env file found, using environment variables");
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to load .env file - check file permissions and syntax"
                );
            }
        }

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
            let pipe = env::var("PIPE_GOT").ok();
            let max_nodes = env::var("GOT_MAX_NODES").ok().and_then(|s| s.parse().ok());
            let max_depth = env::var("GOT_MAX_DEPTH").ok().and_then(|s| s.parse().ok());
            let default_k = env::var("GOT_DEFAULT_K").ok().and_then(|s| s.parse().ok());
            let prune_threshold = env::var("GOT_PRUNE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok());

            // Only create config if any value is set
            if pipe.is_some()
                || max_nodes.is_some()
                || max_depth.is_some()
                || default_k.is_some()
                || prune_threshold.is_some()
            {
                Some(GotPipeConfig {
                    pipe,
                    max_nodes,
                    max_depth,
                    default_k,
                    prune_threshold,
                })
            } else {
                None
            }
        };

        // Detection pipe config - read from env var (filter empty strings)
        let detection_pipe_env = env::var("PIPE_DETECTION")
            .ok()
            .filter(|s| !s.is_empty());
        debug!(
            pipe_detection_env = ?detection_pipe_env,
            "Loading PIPE_DETECTION from environment"
        );
        let detection_config = Some(DetectionPipeConfig {
            pipe: detection_pipe_env,
        });

        // Decision pipe config - read from env var (filter empty strings)
        let decision_pipe_env = env::var("PIPE_DECISION_FRAMEWORK")
            .ok()
            .filter(|s| !s.is_empty());
        debug!(
            pipe_decision_env = ?decision_pipe_env,
            "Loading PIPE_DECISION_FRAMEWORK from environment"
        );
        let decision_config = Some(DecisionPipeConfig {
            pipe: decision_pipe_env.clone(),
        });

        // Evidence pipe config - uses same env var as decision
        let evidence_config = Some(EvidencePipeConfig {
            pipe: decision_pipe_env,
        });

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

        // Error handling configuration
        let error_handling = ErrorHandlingConfig {
            strict_mode: env::var("STRICT_MODE")
                .map(|v| v.to_lowercase() == "true" || v == "1")
                .unwrap_or(false),
            require_pipe_response: env::var("REQUIRE_PIPE_RESPONSE")
                .map(|v| v.to_lowercase() == "true" || v == "1")
                .unwrap_or(false),
            max_fallback_count: env::var("MAX_FALLBACK_COUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        };

        if error_handling.strict_mode {
            debug!("Strict mode enabled - parse errors will propagate");
        }
        if error_handling.require_pipe_response {
            debug!("Require pipe response enabled - no local calculation fallbacks");
        }

        Ok(Config {
            langbase,
            database,
            logging,
            request,
            pipes,
            error_handling,
        })
    }
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            require_pipe_response: false,
            max_fallback_count: 0,
        }
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
            pipe: Some("detection-v1".to_string()),
        }
    }
}

impl Default for GotPipeConfig {
    fn default() -> Self {
        Self {
            pipe: Some("got-reasoning-v1".to_string()),
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
            pipe: Some("decision-framework-v1".to_string()),
        }
    }
}

impl Default for EvidencePipeConfig {
    fn default() -> Self {
        Self {
            pipe: Some("decision-framework-v1".to_string()),
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
        assert_eq!(config.pipe, Some("detection-v1".to_string()));
    }

    #[test]
    fn test_got_pipe_config_default() {
        let config = GotPipeConfig::default();
        assert_eq!(config.pipe, Some("got-reasoning-v1".to_string()));
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
        assert_eq!(
            config.pipe,
            Some("decision-framework-v1".to_string())
        );
    }

    #[test]
    fn test_evidence_pipe_config_default() {
        let config = EvidencePipeConfig::default();
        assert_eq!(
            config.pipe,
            Some("decision-framework-v1".to_string())
        );
    }

    // Note: Config::from_env() tests are in tests/config_env_test.rs
    // because they require serial execution and full env var control.
    // Unit tests here focus on Default impls and type behavior.

    #[test]
    fn test_database_config_struct() {
        let config = DatabaseConfig {
            path: PathBuf::from("/test/path.db"),
            max_connections: 10,
        };
        assert_eq!(config.path, PathBuf::from("/test/path.db"));
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn test_langbase_config_struct() {
        let config = LangbaseConfig {
            api_key: "test-key".to_string(),
            base_url: "https://test.api.com".to_string(),
        };
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://test.api.com");
    }

    #[test]
    fn test_logging_config_struct() {
        let config_pretty = LoggingConfig {
            level: "debug".to_string(),
            format: LogFormat::Pretty,
        };
        assert_eq!(config_pretty.level, "debug");
        assert_eq!(config_pretty.format, LogFormat::Pretty);

        let config_json = LoggingConfig {
            level: "info".to_string(),
            format: LogFormat::Json,
        };
        assert_eq!(config_json.level, "info");
        assert_eq!(config_json.format, LogFormat::Json);
    }

    #[test]
    fn test_request_config_struct() {
        let config = RequestConfig {
            timeout_ms: 60000,
            max_retries: 5,
            retry_delay_ms: 2000,
        };
        assert_eq!(config.timeout_ms, 60000);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 2000);
    }

    #[test]
    fn test_pipe_config_struct_all_fields() {
        let config = PipeConfig {
            linear: "linear-v1".to_string(),
            tree: "tree-v1".to_string(),
            divergent: "divergent-v1".to_string(),
            reflection: "reflection-v1".to_string(),
            auto_router: "router-v1".to_string(),
            auto: Some("auto-v1".to_string()),
            backtracking: Some("backtrack-v1".to_string()),
            got: Some(GotPipeConfig::default()),
            detection: Some(DetectionPipeConfig::default()),
            decision: Some(DecisionPipeConfig::default()),
            evidence: Some(EvidencePipeConfig::default()),
        };

        assert_eq!(config.linear, "linear-v1");
        assert_eq!(config.tree, "tree-v1");
        assert_eq!(config.divergent, "divergent-v1");
        assert_eq!(config.reflection, "reflection-v1");
        assert_eq!(config.auto_router, "router-v1");
        assert_eq!(config.auto, Some("auto-v1".to_string()));
        assert_eq!(config.backtracking, Some("backtrack-v1".to_string()));
        assert!(config.got.is_some());
        assert!(config.detection.is_some());
        assert!(config.decision.is_some());
        assert!(config.evidence.is_some());
    }

    #[test]
    fn test_detection_pipe_config_struct() {
        let config = DetectionPipeConfig {
            pipe: Some("detection-v1".to_string()),
        };
        assert_eq!(config.pipe, Some("detection-v1".to_string()));
    }

    #[test]
    fn test_detection_pipe_config_none_values() {
        let config = DetectionPipeConfig { pipe: None };
        assert!(config.pipe.is_none());
    }

    #[test]
    fn test_got_pipe_config_struct_all_fields() {
        let config = GotPipeConfig {
            pipe: Some("got-reasoning-v1".to_string()),
            max_nodes: Some(50),
            max_depth: Some(5),
            default_k: Some(2),
            prune_threshold: Some(0.5),
        };

        assert_eq!(config.pipe, Some("got-reasoning-v1".to_string()));
        assert_eq!(config.max_nodes, Some(50));
        assert_eq!(config.max_depth, Some(5));
        assert_eq!(config.default_k, Some(2));
        assert_eq!(config.prune_threshold, Some(0.5));
    }

    #[test]
    fn test_got_pipe_config_none_values() {
        let config = GotPipeConfig {
            pipe: None,
            max_nodes: None,
            max_depth: None,
            default_k: None,
            prune_threshold: None,
        };

        assert!(config.pipe.is_none());
        assert!(config.max_nodes.is_none());
        assert!(config.max_depth.is_none());
        assert!(config.default_k.is_none());
        assert!(config.prune_threshold.is_none());
    }

    #[test]
    fn test_decision_pipe_config_struct() {
        let config = DecisionPipeConfig {
            pipe: Some("decision-framework-v1".to_string()),
        };
        assert_eq!(config.pipe, Some("decision-framework-v1".to_string()));
    }

    #[test]
    fn test_decision_pipe_config_none_values() {
        let config = DecisionPipeConfig { pipe: None };
        assert!(config.pipe.is_none());
    }

    #[test]
    fn test_evidence_pipe_config_struct() {
        let config = EvidencePipeConfig {
            pipe: Some("decision-framework-v1".to_string()),
        };
        assert_eq!(config.pipe, Some("decision-framework-v1".to_string()));
    }

    #[test]
    fn test_evidence_pipe_config_none_values() {
        let config = EvidencePipeConfig { pipe: None };
        assert!(config.pipe.is_none());
    }

    #[test]
    fn test_config_struct_clone() {
        let config = RequestConfig::default();
        let cloned = config.clone();
        assert_eq!(config.timeout_ms, cloned.timeout_ms);
        assert_eq!(config.max_retries, cloned.max_retries);
        assert_eq!(config.retry_delay_ms, cloned.retry_delay_ms);
    }

    #[test]
    fn test_pipe_config_clone() {
        let config = PipeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.linear, cloned.linear);
        assert_eq!(config.tree, cloned.tree);
        assert_eq!(config.divergent, cloned.divergent);
    }

    #[test]
    fn test_log_format_debug() {
        let pretty = LogFormat::Pretty;
        let json = LogFormat::Json;
        assert!(format!("{:?}", pretty).contains("Pretty"));
        assert!(format!("{:?}", json).contains("Json"));
    }

    #[test]
    fn test_database_config_debug() {
        let config = DatabaseConfig {
            path: PathBuf::from("/test.db"),
            max_connections: 5,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("DatabaseConfig"));
        assert!(debug_str.contains("test.db"));
    }

    #[test]
    fn test_langbase_config_debug() {
        let config = LangbaseConfig {
            api_key: "key123".to_string(),
            base_url: "https://api.test.com".to_string(),
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("LangbaseConfig"));
        assert!(debug_str.contains("key123"));
    }

    #[test]
    fn test_got_pipe_config_default_values() {
        let config = GotPipeConfig::default();

        // Verify consolidated pipe has expected default value
        assert_eq!(config.pipe.as_deref(), Some("got-reasoning-v1"));

        // Verify all numeric fields have expected default values
        assert_eq!(config.max_nodes, Some(100));
        assert_eq!(config.max_depth, Some(10));
        assert_eq!(config.default_k, Some(3));
        assert_eq!(config.prune_threshold, Some(0.3));
    }

    #[test]
    fn test_detection_pipe_config_default_values() {
        let config = DetectionPipeConfig::default();
        assert_eq!(config.pipe.as_deref(), Some("detection-v1"));
    }

    #[test]
    fn test_decision_pipe_config_default_values() {
        let config = DecisionPipeConfig::default();
        assert_eq!(config.pipe.as_deref(), Some("decision-framework-v1"));
    }

    #[test]
    fn test_evidence_pipe_config_default_values() {
        let config = EvidencePipeConfig::default();
        assert_eq!(config.pipe.as_deref(), Some("decision-framework-v1"));
    }

    #[test]
    fn test_request_config_default_values() {
        let config = RequestConfig::default();

        // Verify all fields have expected defaults
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_pipe_config_default_values() {
        let config = PipeConfig::default();

        // Verify all required string fields
        assert_eq!(config.linear, "linear-reasoning-v1");
        assert_eq!(config.tree, "tree-reasoning-v1");
        assert_eq!(config.divergent, "divergent-reasoning-v1");
        assert_eq!(config.reflection, "reflection-v1");
        assert_eq!(config.auto_router, "mode-router-v1");

        // Verify all optional fields are None by default
        assert!(config.auto.is_none());
        assert!(config.backtracking.is_none());
        assert!(config.got.is_none());
        assert!(config.detection.is_none());
        assert!(config.decision.is_none());
        assert!(config.evidence.is_none());
    }

    #[test]
    fn test_log_format_clone() {
        let original = LogFormat::Pretty;
        let cloned = original.clone();
        assert_eq!(original, cloned);

        let original_json = LogFormat::Json;
        let cloned_json = original_json.clone();
        assert_eq!(original_json, cloned_json);
    }

    #[test]
    fn test_detection_pipe_config_clone() {
        let config = DetectionPipeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.pipe, cloned.pipe);
    }

    #[test]
    fn test_got_pipe_config_clone() {
        let config = GotPipeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.pipe, cloned.pipe);
        assert_eq!(config.max_nodes, cloned.max_nodes);
        assert_eq!(config.prune_threshold, cloned.prune_threshold);
    }

    #[test]
    fn test_decision_pipe_config_clone() {
        let config = DecisionPipeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.pipe, cloned.pipe);
    }

    #[test]
    fn test_evidence_pipe_config_clone() {
        let config = EvidencePipeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.pipe, cloned.pipe);
    }

    #[test]
    fn test_database_config_clone() {
        let config = DatabaseConfig {
            path: PathBuf::from("/test.db"),
            max_connections: 10,
        };
        let cloned = config.clone();
        assert_eq!(config.path, cloned.path);
        assert_eq!(config.max_connections, cloned.max_connections);
    }

    #[test]
    fn test_langbase_config_clone() {
        let config = LangbaseConfig {
            api_key: "test-key".to_string(),
            base_url: "https://test.com".to_string(),
        };
        let cloned = config.clone();
        assert_eq!(config.api_key, cloned.api_key);
        assert_eq!(config.base_url, cloned.base_url);
    }

    #[test]
    fn test_logging_config_clone() {
        let config = LoggingConfig {
            level: "debug".to_string(),
            format: LogFormat::Pretty,
        };
        let cloned = config.clone();
        assert_eq!(config.level, cloned.level);
        assert_eq!(config.format, cloned.format);
    }

    #[test]
    fn test_request_config_clone() {
        let config = RequestConfig {
            timeout_ms: 5000,
            max_retries: 2,
            retry_delay_ms: 500,
        };
        let cloned = config.clone();
        assert_eq!(config.timeout_ms, cloned.timeout_ms);
        assert_eq!(config.max_retries, cloned.max_retries);
        assert_eq!(config.retry_delay_ms, cloned.retry_delay_ms);
    }

    // Tests for ErrorHandlingConfig

    #[test]
    fn test_error_handling_config_default() {
        let config = ErrorHandlingConfig::default();
        assert!(!config.strict_mode);
        assert!(!config.require_pipe_response);
        assert_eq!(config.max_fallback_count, 0);
    }

    #[test]
    fn test_error_handling_config_struct() {
        let config = ErrorHandlingConfig {
            strict_mode: true,
            require_pipe_response: true,
            max_fallback_count: 10,
        };
        assert!(config.strict_mode);
        assert!(config.require_pipe_response);
        assert_eq!(config.max_fallback_count, 10);
    }

    #[test]
    fn test_error_handling_config_clone() {
        let config = ErrorHandlingConfig {
            strict_mode: true,
            require_pipe_response: false,
            max_fallback_count: 5,
        };
        let cloned = config.clone();
        assert_eq!(config.strict_mode, cloned.strict_mode);
        assert_eq!(config.require_pipe_response, cloned.require_pipe_response);
        assert_eq!(config.max_fallback_count, cloned.max_fallback_count);
    }

    #[test]
    fn test_error_handling_config_debug() {
        let config = ErrorHandlingConfig {
            strict_mode: true,
            require_pipe_response: true,
            max_fallback_count: 3,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ErrorHandlingConfig"));
        assert!(debug_str.contains("strict_mode: true"));
        assert!(debug_str.contains("require_pipe_response: true"));
        assert!(debug_str.contains("max_fallback_count: 3"));
    }
}
