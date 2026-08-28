use bigdecimal::BigDecimal;
use diesel::prelude::*;
use uuid::Uuid;

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
