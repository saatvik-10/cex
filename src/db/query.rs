use bigdecimal::BigDecimal;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, SelectableHelper, insert_into};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

use crate::{
    engine::types::Asset,
    error::AppError,
    schema::{balances, users},
};

use super::rows::{BalanceRow, UserRow};

pub async fn insert_user(
    conn: &mut AsyncPgConnection,
    username: &str,
    password_hash: &str,
) -> Result<UserRow, AppError> {
    let row = insert_into(users::table)
        .values((
            users::username.eq(username),
            users::password_hash.eq(password_hash),
        ))
        .returning(UserRow::as_returning())
        .get_result(conn)
        .await?;
    Ok(row)
}

/// Create the initial zero-amount balance rows for a new user.
pub async fn seed_balances(conn: &mut AsyncPgConnection, user_id: Uuid) -> Result<(), AppError> {
    for asset in Asset::ALL {
        insert_into(balances::table)
            .values((
                balances::user_id.eq(user_id),
                balances::asset.eq(asset.as_str()),
                balances::amount.eq(BigDecimal::from(0)),
            ))
            .execute(conn)
            .await?;
    }
    Ok(())
}

pub async fn find_user_by_username(
    conn: &mut AsyncPgConnection,
    username: &str,
) -> Result<Option<UserRow>, AppError> {
    let row = users::table
        .filter(users::username.eq(username))
        .select(UserRow::as_select())
        .first(conn)
        .await
        .optional()?;
    Ok(row)
}

pub async fn find_user_by_id(
    conn: &mut AsyncPgConnection,
    user_id: Uuid,
) -> Result<Option<UserRow>, AppError> {
    let row = users::table
        .filter(users::id.eq(user_id))
        .select(UserRow::as_select())
        .first(conn)
        .await
        .optional()?;
    Ok(row)
}

pub async fn load_all_users(conn: &mut AsyncPgConnection) -> Result<Vec<UserRow>, AppError> {
    let rows = users::table.select(UserRow::as_select()).load(conn).await?;
    Ok(rows)
}

pub async fn load_all_balances(conn: &mut AsyncPgConnection) -> Result<Vec<BalanceRow>, AppError> {
    let rows = balances::table
        .select(BalanceRow::as_select())
        .load(conn)
        .await?;
    Ok(rows)
}
