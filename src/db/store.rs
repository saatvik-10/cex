use chrono::{DateTime, Utc};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, insert_into, update};
use diesel::upsert::excluded;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

use crate::{error::AppError, schema::refresh_tokens, db::rows::BalanceRecord};

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

/// Apply a batch of balance mutations, overwriting each (user_id, asset) row.
pub async fn upsert_balances(
    conn: &mut AsyncPgConnection,
    records: &[BalanceRecord],
) -> Result<(), AppError> {
    if records.is_empty() {
        return Ok(());
    }

    insert_into(crate::schema::balances::table)
        .values(
            records
                .iter()
                .map(|r| {
                    (
                        crate::schema::balances::user_id.eq(r.user_id),
                        crate::schema::balances::asset.eq(r.asset.as_str()),
                        crate::schema::balances::amount.eq(r.amount.clone()),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .on_conflict((crate::schema::balances::user_id, crate::schema::balances::asset))
        .do_update()
        .set(crate::schema::balances::amount.eq(excluded(crate::schema::balances::amount)))
        .execute(conn)
        .await?;

    Ok(())
}
