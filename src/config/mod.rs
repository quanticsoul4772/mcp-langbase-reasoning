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

        let pipes = PipeConfig {
            linear: env::var("PIPE_LINEAR").unwrap_or_else(|_| "linear-reasoning-v1".to_string()),
            tree: env::var("PIPE_TREE").unwrap_or_else(|_| "tree-reasoning-v1".to_string()),
            divergent: env::var("PIPE_DIVERGENT")
                .unwrap_or_else(|_| "divergent-reasoning-v1".to_string()),
            reflection: env::var("PIPE_REFLECTION").unwrap_or_else(|_| "reflection-v1".to_string()),
            auto_router: env::var("PIPE_AUTO").unwrap_or_else(|_| "mode-router-v1".to_string()),
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
