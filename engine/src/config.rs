use std::env;
use std::time::Duration;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    MissingEnv(String),
    #[error("invalid value for env var {key}: {value}")]
    InvalidEnv { key: String, value: String },
}

/// Engine configuration loaded from the environment.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Postgres checkpoint sink for balances.
    pub database_url: String,
    /// Redis used for the command/result streams.
    pub redis_url: String,
    /// Path to the balance write-ahead log.
    pub wal_path: String,
    /// How often the worker fsyncs the WAL and checkpoints to Postgres.
    pub balance_flush_interval_ms: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let try_required =
            |key: &str| env::var(key).map_err(|_| ConfigError::MissingEnv(key.to_string()));

        let database_url = try_required("DATABASE_URL")?;

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());

        let wal_path = std::env::var("WAL_PATH").unwrap_or_else(|_| "wal.log".into());

        let balance_flush_interval_ms: u64 = std::env::var("BALANCE_FLUSH_INTERVAL_MS")
            .unwrap_or_else(|_| "100".into())
            .parse::<u64>()
            .map_err(|e: std::num::ParseIntError| ConfigError::InvalidEnv {
                key: "BALANCE_FLUSH_INTERVAL_MS".into(),
                value: e.to_string(),
            })?;

        Ok(Self {
            database_url,
            redis_url,
            wal_path,
            balance_flush_interval_ms,
        })
    }

    pub fn flush_interval(&self) -> Duration {
        Duration::from_millis(self.balance_flush_interval_ms)
    }
}
