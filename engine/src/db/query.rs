use diesel::{QueryDsl, SelectableHelper};
use diesel_async::{AsyncPgConnection, RunQueryDsl};

use crate::error::AppError;
use crate::schema::balances;

use super::rows::{BalanceRow, IdempotencyRow};

pub async fn load_all_balances(conn: &mut AsyncPgConnection) -> Result<Vec<BalanceRow>, AppError> {
    let rows = balances::table
        .select(BalanceRow::as_select())
        .load(conn)
        .await?;
    Ok(rows)
}

/// All persisted idempotency answers, loaded at boot so retries are served
/// from memory (and never re-applied) even after a restart.
pub async fn load_idempotency_keys(
    conn: &mut AsyncPgConnection,
) -> Result<Vec<IdempotencyRow>, AppError> {
    use crate::schema::idempotency_keys;

    let rows = idempotency_keys::table
        .select(IdempotencyRow::as_select())
        .load(conn)
        .await?;
    Ok(rows)
}
