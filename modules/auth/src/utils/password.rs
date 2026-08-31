//! Password hashing, isolated behind a trait so the algorithm can be swapped.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use std::sync::LazyLock;

/// Error raised when hashing a password fails.
#[derive(Debug, thiserror::Error)]
pub enum PasswordHashError {
    /// The underlying hasher rejected the input.
    #[error("failed to hash password: {0}")]
    Hash(String),
}

/// A password hashing/verification strategy.
pub trait PasswordAlgorithm: Send + Sync {
    /// Hash a plaintext password into a storable PHC string.
    fn hash_password(&self, plain: &str) -> Result<String, PasswordHashError>;

    /// Verify a plaintext password against a stored hash. Returns `false` on any
    /// parse/verify error.
    fn verify_password(&self, plain: &str, hash: &str) -> bool;

    /// Verify against an optional stored hash. When the hash is absent (e.g. the
    /// account does not exist) a dummy verification runs to keep timing roughly
    /// constant, and the result is always `false`.
    fn verify_password_or_dummy(&self, plain: &str, hash: Option<&str>) -> bool {
        match hash {
            Some(h) => self.verify_password(plain, h),
            None => {
                let _ = self.verify_password(plain, dummy_hash());
                false
            }
        }
    }
}

/// Argon2id-based password algorithm.
#[derive(Clone, Default)]
pub struct Argon2PasswordAlgorithm;

impl PasswordAlgorithm for Argon2PasswordAlgorithm {
    fn hash_password(&self, plain: &str) -> Result<String, PasswordHashError> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(plain.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| PasswordHashError::Hash(e.to_string()))
    }

    fn verify_password(&self, plain: &str, hash: &str) -> bool {
        match PasswordHash::new(hash) {
            Ok(parsed) => Argon2::default()
                .verify_password(plain.as_bytes(), &parsed)
                .is_ok(),
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse stored password hash");
                false
            }
        }
    }
}

/// A lazily computed, valid Argon2 hash used for constant-time verification
/// against missing accounts.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"dummy-password-for-constant-time", &salt)
        .map(|hash| hash.to_string())
        .unwrap_or_default()
});

fn dummy_hash() -> &'static str {
    DUMMY_HASH.as_str()
}
