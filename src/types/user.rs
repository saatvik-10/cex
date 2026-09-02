use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Non-sensitive user info returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct SignUpInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SignInInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserSummary,
}
