use actix_web::{HttpResponse, get, post, web};
use bigdecimal::BigDecimal;
use diesel::deserialize::QueryableByName;
use diesel::sql_query;
use diesel::sql_types::{Numeric, Text, Uuid as SqlUuid};
use diesel_async::RunQueryDsl;

/// Result of the atomic credit query.
#[derive(QueryableByName)]
struct AmountRow {
    #[diesel(sql_type = Numeric)]
    amount: BigDecimal,
}

use crate::{
    AppState,
    auth::middleware::AuthUser,
    engine::types::Asset,
    error::{ApiResult, AppError},
    types::wallet::{BalanceEntry, BalanceResponse, OnrampInput},
};

//RN NO DB CALL IMPLEMENTED...ONLY THE CACHE PART
#[get("/balance")]
pub async fn get_balance(auth: AuthUser, state: web::Data<AppState>) -> ApiResult<HttpResponse> {
    let user_id = auth.0;

    // Fast path: read from the in-memory cache, falling back to the DB.
    let entries = Asset::ALL
        .iter()
        .map(|asset| {
            let amount = match state.balances.get(&(user_id, *asset)) {
                Some(v) => v.clone(),
                None => BigDecimal::from(0),
            };
            BalanceEntry {
                asset: asset.as_str().to_string(),
                amount: amount.to_string(),
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(BalanceResponse { balances: entries }))
}

#[post("/onramp")]
pub async fn onramp(
    body: web::Json<OnrampInput>,
    auth: AuthUser,
    state: web::Data<AppState>,
) -> ApiResult<HttpResponse> {
    #[allow(clippy::cmp_owned)] // BigDecimal has no const ZERO; owned compare is fine here.
    if body.amount <= BigDecimal::from(0) {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }

    let user_id = auth.0;
    let asset = body.asset;
    let amount = body.amount.clone();

    let mut conn = state.pool.get().await?;

    // Atomic credit. Returns the resulting balance for cache synchronisation.
    let row: AmountRow = sql_query(
        "UPDATE balances SET amount = amount + $1 \
         WHERE user_id = $2 AND asset = $3 \
         RETURNING amount",
    )
    .bind::<Numeric, _>(&amount)
    .bind::<SqlUuid, _>(user_id)
    .bind::<Text, _>(asset.as_str())
    .get_result(&mut conn)
    .await?;
    let new_amount = row.amount;

    // Update the cache to match the source of truth.
    state.balances.insert((user_id, asset), new_amount.clone());

    Ok(HttpResponse::Ok().json(BalanceEntry {
        asset: asset.as_str().to_string(),
        amount: new_amount.to_string(),
    }))
}
