use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString,
    password_hash::rand_core::OsRng,
};

use crate::error::AppError;

/// Hash a plaintext password using Argon2id with a freshly generated salt.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(Into::into)
}

/// Verify a plaintext password against a stored PHC-formatted hash.
pub fn verify_password(password: &str, phc_hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(phc_hash).map_err(|_| AppError::Internal)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
