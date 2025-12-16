use std::env;
use std::path::PathBuf;

use crate::error::AppError;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    pub langbase: LangbaseConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub request: RequestConfig,
    pub pipes: PipeConfig,
}

/// Langbase API configuration
#[derive(Debug, Clone)]
pub struct LangbaseConfig {
    pub api_key: String,
    pub base_url: String,
}

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: PathBuf,
    pub max_connections: u32,
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
}

/// Log output format
#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    Pretty,
    Json,
}

/// HTTP request configuration
#[derive(Debug, Clone)]
pub struct RequestConfig {
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

/// Langbase pipe name configuration
#[derive(Debug, Clone)]
pub struct PipeConfig {
    pub linear: String,
    pub tree: String,
    pub divergent: String,
    pub reflection: String,
    pub auto_router: String,
    pub auto: Option<String>,
    pub backtracking: Option<String>,
    pub got: Option<GotPipeConfig>,
}

/// Graph-of-Thoughts pipe configuration
#[derive(Debug, Clone)]
pub struct GotPipeConfig {
    pub generate_pipe: Option<String>,
    pub score_pipe: Option<String>,
    pub aggregate_pipe: Option<String>,
    pub refine_pipe: Option<String>,
    pub max_nodes: Option<usize>,
    pub max_depth: Option<usize>,
    pub default_k: Option<usize>,
    pub prune_threshold: Option<f64>,
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
            let prune_threshold = env::var("GOT_PRUNE_THRESHOLD").ok().and_then(|s| s.parse().ok());

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

    // Note: Config::from_env() tests are in tests/config_env_test.rs
    // because they require serial execution and full env var control.
    // Unit tests here focus on Default impls and type behavior.
}
