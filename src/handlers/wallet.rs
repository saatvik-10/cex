use actix_web::{HttpResponse, get, post, web};
use bigdecimal::{BigDecimal, Zero};

use crate::{
    AppState,
    auth::middleware::AuthUser,
    engine::types::Asset,
    error::{ApiResult, AppError},
    types::wallet::{BalanceEntry, BalanceResponse, DepositInput},
};

/// Format an amount for the wire. Postgres stores NUMERIC(36, 18), so we emit
/// 18 fractional digits to match; zero is emitted as "0" for readability.
fn fmt_amount(amount: &BigDecimal) -> String {
    if amount.is_zero() {
        "0".to_string()
    } else {
        amount.with_scale(18).to_string()
    }
}

#[get("/balance")]
pub async fn get_balance(auth: AuthUser, state: web::Data<AppState>) -> ApiResult<HttpResponse> {
    let user_id = auth.0;

    // Read from the single balance worker; no DB, no locks.
    let rows = state.balance.get_balances(user_id).await?;

    let balances = rows
        .into_iter()
        .map(|(asset, amount)| BalanceEntry {
            asset: asset.as_str().to_string(),
            amount: fmt_amount(&amount),
        })
        .collect();

    Ok(HttpResponse::Ok().json(BalanceResponse { balances })) 
}

#[post("/{asset}")]
pub async fn deposit(
    asset: web::Path<String>,
    body: web::Json<DepositInput>,
    auth: AuthUser,
    state: web::Data<AppState>,
) -> ApiResult<HttpResponse> {
    let asset = Asset::parse(&asset.into_inner())
        .ok_or_else(|| AppError::BadRequest("unsupported asset".into()))?;

    if body.amount <= 0 {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }

    let user_id = auth.0;
    let new_amount = state
        .balance
        .deposit(user_id, asset, body.amount.clone())
        .await?;

    Ok(HttpResponse::Ok().json(BalanceEntry {
        asset: asset.as_str().to_string(),
        amount: fmt_amount(&new_amount),
    }))
}