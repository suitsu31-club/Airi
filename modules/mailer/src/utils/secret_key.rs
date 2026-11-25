//! `MAILER_SECRET_KEY`: the key every SMTP credential is encrypted with.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::env::VarError;
use type_safe_data::encryption::Key256;
use zeroize::Zeroizing;

pub const SECRET_KEY_VAR: &str = "MAILER_SECRET_KEY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SecretKeyError {
    #[error("MAILER_SECRET_KEY is not valid UTF-8")]
    NotUnicode,
    #[error("MAILER_SECRET_KEY is not standard base64")]
    NotBase64,
    #[error("MAILER_SECRET_KEY must decode to 32 bytes")]
    WrongLength,
}

/// The key in `MAILER_SECRET_KEY`: 32 bytes in standard base64. `None` while
/// it is unset or empty, so an install without SMTP credentials runs without
/// it.
pub fn secret_key_from_env() -> Result<Option<Key256>, SecretKeyError> {
    match std::env::var(SECRET_KEY_VAR) {
        Ok(encoded) => decode_secret_key(&Zeroizing::new(encoded)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(SecretKeyError::NotUnicode),
    }
}

fn decode_secret_key(encoded: &str) -> Result<Option<Key256>, SecretKeyError> {
    let encoded = encoded.trim();
    if encoded.is_empty() {
        return Ok(None);
    }
    let bytes = Zeroizing::new(
        STANDARD
            .decode(encoded)
            .map_err(|_| SecretKeyError::NotBase64)?,
    );
    Key256::try_from(bytes.as_slice())
        .map(Some)
        .map_err(|_| SecretKeyError::WrongLength)
}
