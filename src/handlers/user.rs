use actix_web::{HttpResponse, get, post, web};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{issue::issue_tokens, jwt, password, refresh},
    db::{query, store},
    engine::types::Asset,
    error::{ApiResult, AppError},
    types::user::{SignInInput, SignUpInput, UserSummary},
};

#[derive(Debug, Deserialize)]
pub struct RefreshInput {
    pub refresh_token: String,
}

fn validate_signup(input: &SignUpInput) -> Result<(), AppError> {
    if input.username.trim().is_empty() {
        return Err(AppError::BadRequest("username is required".into()));
    }
    if input.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

#[post("/signup")]
pub async fn sign_up(
    body: web::Json<SignUpInput>,
    state: web::Data<AppState>,
) -> ApiResult<HttpResponse> {
    validate_signup(&body)?;

    let mut conn = state.pool.get().await?;
    let password_hash = password::hash_password(&body.password)?;

    let user = match query::insert_user(&mut conn, &body.username, &password_hash).await {
        Ok(user) => user,
        Err(AppError::Database(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        ))) => return Err(AppError::Conflict("username already taken".into())),
        Err(e) => return Err(e),
    };
    query::seed_balances(&mut conn, user.id).await?;

    // Keep the in-memory caches in sync with the write.
    state.users.insert(user.username.clone(), user.id);
    for asset in Asset::ALL {
        state
            .balances
            .insert((user.id, asset), bigdecimal::BigDecimal::from(0));
    }

    let summary = UserSummary {
        id: user.id,
        username: user.username,
    };
    let auth = issue_tokens(&mut conn, &state.config, &summary).await?;

    Ok(HttpResponse::Ok().json(auth))
}

#[post("/signin")]
pub async fn sign_in(
    body: web::Json<SignInInput>,
    state: web::Data<AppState>,
) -> ApiResult<HttpResponse> {
    let mut conn = state.pool.get().await?;

    let user = query::find_user_by_username(&mut conn, &body.username).await?;
    let Some(user) = user else {
        return Err(AppError::Unauthorized);
    };

    if !password::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }

    let summary = UserSummary {
        id: user.id,
        username: user.username,
    };
    let auth = issue_tokens(&mut conn, &state.config, &summary).await?;

    Ok(HttpResponse::Ok().json(auth))
}

#[post("/refresh")]
pub async fn refresh_tokens(
    body: web::Json<RefreshInput>,
    state: web::Data<AppState>,
) -> ApiResult<HttpResponse> {
    let (user_id, kind) = jwt::verify(&body.refresh_token, &state.config.jwt_secret)?;
    if kind != jwt::TokenKind::Refresh {
        return Err(AppError::Unauthorized);
    }

    let mut conn = state.pool.get().await?;
    let token_hash = refresh::hash_token(&body.refresh_token);

    // The token must still be active in the DB (not revoked, not expired).
    let stored = store::find_active_refresh_token(&mut conn, &token_hash).await?;
    let Some(stored_user) = stored else {
        return Err(AppError::Unauthorized);
    };
    if stored_user != user_id {
        return Err(AppError::Unauthorized);
    }

    // Rotate: revoke the old token, issue a fresh pair.
    store::revoke_refresh_token(&mut conn, &token_hash).await?;

    let user = query::find_user_by_id(&mut conn, user_id).await?;
    let Some(user) = user else {
        return Err(AppError::Unauthorized);
    };

    let summary = UserSummary {
        id: user.id,
        username: user.username,
    };
    let auth = issue_tokens(&mut conn, &state.config, &summary).await?;

    Ok(HttpResponse::Ok().json(auth))
}

#[get("/profile")]
pub async fn profile(
    auth: crate::auth::middleware::AuthUser,
    state: web::Data<AppState>,
) -> ApiResult<HttpResponse> {
    let user_id: Uuid = auth.0;

    let user = state
        .users
        .iter()
        .find(|entry| *entry.value() == user_id)
        .map(|entry| UserSummary {
            id: *entry.value(),
            username: entry.key().clone(),
        });

    let Some(user) = user else {
        return Err(AppError::NotFound);
    };

    Ok(HttpResponse::Ok().json(user))
}
