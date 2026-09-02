use chrono::{Duration, Utc};
use diesel_async::AsyncPgConnection;

use crate::{
    config::AppConfig,
    db::store,
    error::AppError,
    types::user::{AuthResponse, UserSummary},
};

use super::{jwt, refresh};

/// Create an access + refresh pair, persist the refresh token digest, and
/// return the response. Shared by signup, signin, and refresh.
pub async fn issue_tokens(
    conn: &mut AsyncPgConnection,
    config: &AppConfig,
    user: &UserSummary,
) -> Result<AuthResponse, AppError> {
    let access_token = jwt::create_token(
        user.id,
        jwt::TokenKind::Access,
        &config.jwt_secret,
        config.access_ttl_seconds,
    )?;

    let refresh_token = jwt::create_token(
        user.id,
        jwt::TokenKind::Refresh,
        &config.jwt_secret,
        config.refresh_ttl_seconds,
    )?;

    let expires_at = Utc::now() + Duration::seconds(config.refresh_ttl_seconds);
    store::insert_refresh_token(
        conn,
        user.id,
        &refresh::hash_token(&refresh_token),
        expires_at,
    )
    .await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
        user: user.clone(),
    })
}
