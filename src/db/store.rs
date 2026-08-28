use chrono::{DateTime, Utc};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, insert_into, update};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

use crate::{error::AppError, schema::refresh_tokens};

pub async fn insert_refresh_token(
    conn: &mut AsyncPgConnection,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    insert_into(refresh_tokens::table)
        .values((
            refresh_tokens::user_id.eq(user_id),
            refresh_tokens::token_hash.eq(token_hash),
            refresh_tokens::expires_at.eq(expires_at),
            refresh_tokens::revoked.eq(false),
        ))
        .execute(conn)
        .await?;
    Ok(())
}

/// Returns the user id for a single active (unrevoked, unexpired) refresh token.
pub async fn find_active_refresh_token(
    conn: &mut AsyncPgConnection,
    token_hash: &str,
) -> Result<Option<Uuid>, AppError> {
    let user_id = refresh_tokens::table
        .filter(refresh_tokens::token_hash.eq(token_hash))
        .filter(refresh_tokens::revoked.eq(false))
        .filter(refresh_tokens::expires_at.gt(Utc::now()))
        .select(refresh_tokens::user_id)
        .first::<Uuid>(conn)
        .await
        .optional()?;
    Ok(user_id)
}

/// Mark every matching unrevoked token as revoked (used on rotation).
pub async fn revoke_refresh_token(
    conn: &mut AsyncPgConnection,
    token_hash: &str,
) -> Result<(), AppError> {
    update(refresh_tokens::table)
        .filter(refresh_tokens::token_hash.eq(token_hash))
        .filter(refresh_tokens::revoked.eq(false))
        .set(refresh_tokens::revoked.eq(true))
        .execute(conn)
        .await?;
    Ok(())
}
