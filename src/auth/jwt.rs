use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Access,
    Refresh,
}

impl TokenKind {
    fn as_str(&self) -> &'static str {
        match self {
            TokenKind::Access => "access",
            TokenKind::Refresh => "refresh",
        }
    }

    fn from_str(s: &str) -> Option<TokenKind> {
        match s {
            "access" => Some(TokenKind::Access),
            "refresh" => Some(TokenKind::Refresh),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    /// Unique token id so two tokens minted in the same second never collide
    /// (e.g. refresh-token rotation would otherwise produce an identical token).
    jti: String,
    typ: String,
    iat: usize,
    exp: usize,
}

fn encode(
    user_id: Uuid,
    kind: TokenKind,
    secret: &str,
    ttl_seconds: i64,
) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        jti: Uuid::new_v4().to_string(),
        typ: kind.as_str().to_string(),
        iat: now as usize,
        exp: (now + ttl_seconds) as usize,
    };

    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(Into::into)
}

/// Create an HS256 JWT with the given kind and subject.
pub fn create_token(
    user_id: Uuid,
    kind: TokenKind,
    secret: &str,
    ttl_seconds: i64,
) -> Result<String, AppError> {
    encode(user_id, kind, secret, ttl_seconds)
}

/// Verify a JWT and return its subject (user id) and kind.
pub fn verify(token: &str, secret: &str) -> Result<(Uuid, TokenKind), AppError> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| AppError::Unauthorized)?;

    let user_id = Uuid::parse_str(&data.claims.sub).map_err(|_| AppError::Unauthorized)?;
    let kind = TokenKind::from_str(&data.claims.typ).ok_or(AppError::Unauthorized)?;

    Ok((user_id, kind))
}
