use bigdecimal::BigDecimal;
use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::balances;
use crate::types::Asset;

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = balances)]
pub struct BalanceRow {
    pub user_id: Uuid,
    pub asset: String,
    /// Money domain of the row. The engine only ever produces demo money
    /// today; the column exists so a future REAL source can coexist.
    #[allow(dead_code)] // read implicitly via diesel Selectable/as_select
    pub source: String,
    pub amount: BigDecimal,
}

/// A balance mutation ready to be journaled to the WAL and applied to Postgres.
#[derive(Debug, Clone)]
pub struct BalanceRecord {
    pub user_id: Uuid,
    pub asset: Asset,
    pub amount: BigDecimal,
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = crate::schema::idempotency_keys)]
pub struct IdempotencyRow {
    pub user_id: Uuid,
    pub key: String,
    pub amount: BigDecimal,
}

/// A durable (user_id, key) -> resulting-amount answer persisted in the same
/// transaction as the balance checkpoint. Makes retries idempotent across
/// restarts (no double credit).
#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub user_id: Uuid,
    pub key: String,
    pub amount: BigDecimal,
}
