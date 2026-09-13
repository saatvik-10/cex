use bigdecimal::BigDecimal;
use diesel::prelude::*;
use uuid::Uuid;

use crate::engine::types::Asset;
use crate::schema::{balances, users};

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = users)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = balances)]
pub struct BalanceRow {
    pub user_id: Uuid,
    pub asset: String,
    pub amount: BigDecimal,
}

/// A balance mutation ready to be journaled to the WAL and applied to Postgres.
#[derive(Debug, Clone)]
pub struct BalanceRecord {
    pub user_id: Uuid,
    pub asset: Asset,
    pub amount: BigDecimal,
}
