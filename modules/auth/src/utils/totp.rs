//! Pure TOTP and recovery-code helpers.
//!
//! Wraps the `totp-rs` 6.0 builder API and the recovery-code crypto. These are
//! dependency-light helpers: no database, Redis, or message queue. TOTP build
//! failures (which should never happen for a well-formed 20-byte secret) map to
//! [`wakuwaku::Error::BusinessPanic`].

use rand::RngCore;
use sha2::{Digest, Sha256};

/// Issuer label shown by authenticator apps.
pub const TOTP_ISSUER: &str = "Airi";

/// Number of one-time recovery codes minted per enrollment.
pub const RECOVERY_CODE_COUNT: usize = 10;

/// Generate a fresh 160-bit (20-byte) TOTP secret.
pub fn generate_secret_bytes() -> Vec<u8> {
    totp_rs::Secret::generate().as_bytes().to_vec()
}

/// Render a raw secret as its canonical base32 string (for manual entry).
pub fn secret_base32(secret: &[u8]) -> String {
    totp_rs::Secret::from(secret.to_vec()).to_base32()
}

/// Build a labelled [`totp_rs::Totp`] (issuer + account name) for provisioning.
pub fn build_totp(secret: &[u8], account_name: &str) -> Result<totp_rs::Totp, wakuwaku::Error> {
    totp_rs::Builder::new()
        .with_secret(secret.to_vec())
        .with_issuer(Some(TOTP_ISSUER))
        .with_account_name(account_name)
        .build()
        .map_err(|e| wakuwaku::Error::BusinessPanic(anyhow::anyhow!("totp build: {e}")))
}

/// Check a submitted TOTP code against a secret. Returns `false` if the secret
/// cannot build a validator (never expected for a stored secret).
pub fn verify_totp(secret: &[u8], code: &str) -> bool {
    match totp_rs::Builder::new().with_secret(secret.to_vec()).build() {
        Ok(totp) => totp.check_current(code).is_some(),
        Err(_) => false,
    }
}

/// The `otpauth://` provisioning URI for a labelled TOTP.
pub fn otpauth_uri(totp: &totp_rs::Totp) -> Result<String, wakuwaku::Error> {
    totp.to_url()
        .map_err(|e| wakuwaku::Error::BusinessPanic(anyhow::anyhow!("totp url: {e}")))
}

/// A base64-encoded PNG QR code for a labelled TOTP.
pub fn qr_png_base64(totp: &totp_rs::Totp) -> Result<String, wakuwaku::Error> {
    totp.to_qr_base64()
        .map_err(|e| wakuwaku::Error::BusinessPanic(anyhow::anyhow!("totp qr: {e}")))
}

/// Mint [`RECOVERY_CODE_COUNT`] one-time recovery codes (16 uppercase base32
/// chars each, derived from 10 CSPRNG bytes).
pub fn generate_recovery_codes() -> Vec<String> {
    let mut codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
    for _ in 0..RECOVERY_CODE_COUNT {
        let mut bytes = [0u8; 10];
        rand::rng().fill_bytes(&mut bytes);
        codes.push(fast32::base32::RFC4648_NOPAD.encode(&bytes));
    }
    codes
}

/// Canonicalize a recovery code for hashing/comparison: uppercase, stripping
/// whitespace and `-` separators.
pub fn normalize_recovery_code(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// SHA-256 hash of a normalized recovery code (32 raw bytes, for `bytea`).
pub fn hash_recovery_code(code: &str) -> Vec<u8> {
    Sha256::digest(normalize_recovery_code(code).as_bytes()).to_vec()
}
