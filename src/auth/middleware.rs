use actix_web::{FromRequest, HttpRequest, dev::Payload, http::header::AUTHORIZATION, web};
use futures_util::future::LocalBoxFuture;
use uuid::Uuid;

use crate::{AppState, error::AppError};

/// Extractor for protected routes. Verifies the `Authorization: Bearer <jwt>`
/// header and resolves the caller's user id.
pub struct AuthUser(pub Uuid);

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<Self, AppError>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(auth_from_request(req))
    }
}

async fn auth_from_request(req: HttpRequest) -> Result<AuthUser, AppError> {
    let state = req
        .app_data::<web::Data<AppState>>()
        .ok_or(AppError::Internal)?;

    let header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or(AppError::Unauthorized)?;

    let (user_id, kind) = crate::auth::jwt::verify(token, &state.config.jwt_secret)?;
    if kind != crate::auth::jwt::TokenKind::Access {
        return Err(AppError::Unauthorized);
    }

    Ok(AuthUser(user_id))
}
