use sha2::{Digest, Sha256};

/// Hash a raw refresh token for storage. We never store the raw token; only
/// its SHA-256 digest, so a DB leak does not expose usable tokens.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
