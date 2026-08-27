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

/// App configuration loaded from the environment.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    /// Lifetime of an access token, in seconds.
    pub access_ttl_seconds: i64,
    /// Lifetime of a refresh token, in seconds.
    pub refresh_ttl_seconds: i64,
    pub port: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let try_required =
            |key: &str| env::var(key).map_err(|_| ConfigError::MissingEnv(key.to_string()));

        let database_url = try_required("DATABASE_URL")?;
        let jwt_secret = try_required("JWT_SECRET")?;

        let access_ttl_minutes: i64 = try_required("ACCESS_TTL_MINUTES")?.parse::<i64>().map_err(
            |e: std::num::ParseIntError| ConfigError::InvalidEnv {
                key: "ACCESS_TTL_MINUTES".into(),
                value: e.to_string(),
            },
        )?;

        let refresh_ttl_days: i64 = try_required("REFRESH_TTL_DAYS")?.parse::<i64>().map_err(
            |e: std::num::ParseIntError| ConfigError::InvalidEnv {
                key: "REFRESH_TTL_DAYS".into(),
                value: e.to_string(),
            },
        )?;

        let port: u16 = env::var("PORT")
            .unwrap_or_else(|_| "8000".into())
            .parse()
            .unwrap_or(8000);

        Ok(Self {
            database_url,
            jwt_secret,
            access_ttl_seconds: access_ttl_minutes * 60,
            refresh_ttl_seconds: refresh_ttl_days * 24 * 60 * 60,
            port,
        })
    }

    pub fn access_ttl(&self) -> Duration {
        Duration::from_secs(self.access_ttl_seconds as u64)
    }
}
