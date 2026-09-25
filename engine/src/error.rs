use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("internal server error")]
    Internal,

    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("database pool error: {0}")]
    Pool(#[from] bb8::RunError<diesel_async::pooled_connection::PoolError>),

    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}
