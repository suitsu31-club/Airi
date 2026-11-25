//! Strings encrypted at rest.
//!
//! [`EncryptedString<Alg>`] holds only ciphertext; the type parameter names
//! the [`SymmetricEncryption`] scheme that produced it, so a value can't be
//! decrypted with another scheme by mistake. [`DefaultSymmetricEncryption`] is
//! the scheme new code uses.
//!
//! Every value is bound to *associated data*: bytes that are authenticated but
//! not stored, such as the ID of the row holding the ciphertext. Decrypting
//! with other associated data fails, so a ciphertext copied into another row
//! is useless there.

use chacha20poly1305::aead::{AeadInOut, Generate, KeyInit, Tag};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Type};
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use zeroize::Zeroizing;

/// A combination of algorithms that encrypts bytes under a secret key.
pub trait SymmetricEncryption {
    /// The secret key.
    type Key;

    /// Encrypt `plaintext`, bound to `associated_data`. The output carries
    /// everything [`decrypt`](Self::decrypt) needs besides the key and the
    /// associated data (nonce, tag, …).
    fn encrypt(
        key: &Self::Key,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, EncryptionError>;

    /// Decrypt the output of [`encrypt`](Self::encrypt).
    fn decrypt(
        key: &Self::Key,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, EncryptionError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("the operating system's random number generator failed")]
    Random,
    #[error("encryption failed")]
    Encrypt,
    /// The ciphertext is truncated or altered, or was encrypted under another
    /// key or other associated data.
    #[error("decryption failed")]
    Decrypt,
    #[error("the decrypted bytes are not UTF-8")]
    NotUtf8,
}

/// A 256-bit secret key, wiped from memory on drop.
#[derive(Clone)]
pub struct Key256(Zeroizing<[u8; 32]>);

impl From<[u8; 32]> for Key256 {
    fn from(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
}

impl TryFrom<&[u8]> for Key256 {
    type Error = std::array::TryFromSliceError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut key = Zeroizing::new([0; 32]);
        key.copy_from_slice(<&[u8; 32]>::try_from(bytes)?);
        Ok(Self(key))
    }
}

impl Debug for Key256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Key256(..)")
    }
}

/// XChaCha20-Poly1305 with a random 192-bit nonce per value.
///
/// Layout: `nonce (24 bytes) || ciphertext || tag (16 bytes)`. A random nonce
/// of this size never repeats in practice, so one key can encrypt any number
/// of values.
#[derive(Debug)]
pub enum DefaultSymmetricEncryption {}

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

impl SymmetricEncryption for DefaultSymmetricEncryption {
    type Key = Key256;

    fn encrypt(
        key: &Key256,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        let cipher = XChaCha20Poly1305::new((&*key.0).into());
        let nonce = XNonce::try_generate().map_err(|_| EncryptionError::Random)?;

        let mut output = Vec::with_capacity(
            NONCE_LEN
                .saturating_add(plaintext.len())
                .saturating_add(TAG_LEN),
        );
        output.extend_from_slice(&nonce);
        output.extend_from_slice(plaintext);
        let (_, body) = output.split_at_mut(NONCE_LEN);
        let tag = cipher
            .encrypt_inout_detached(&nonce, associated_data, body.into())
            .map_err(|_| EncryptionError::Encrypt)?;
        output.extend_from_slice(&tag);
        Ok(output)
    }

    fn decrypt(
        key: &Key256,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, EncryptionError> {
        let (nonce, rest) = ciphertext
            .split_at_checked(NONCE_LEN)
            .ok_or(EncryptionError::Decrypt)?;
        let body_len = rest
            .len()
            .checked_sub(TAG_LEN)
            .ok_or(EncryptionError::Decrypt)?;
        let (body, tag) = rest.split_at(body_len);
        let nonce = XNonce::try_from(nonce).map_err(|_| EncryptionError::Decrypt)?;
        let tag = Tag::<XChaCha20Poly1305>::try_from(tag).map_err(|_| EncryptionError::Decrypt)?;

        let cipher = XChaCha20Poly1305::new((&*key.0).into());
        let mut plaintext = Zeroizing::new(body.to_vec());
        cipher
            .decrypt_inout_detached(
                &nonce,
                associated_data,
                plaintext.as_mut_slice().into(),
                &tag,
            )
            .map_err(|_| EncryptionError::Decrypt)?;
        Ok(plaintext)
    }
}

/// A UTF-8 string encrypted with `Alg`. Holds only the ciphertext.
///
/// Stored in the database as the raw ciphertext (`bytea` in PostgreSQL).
pub struct EncryptedString<Alg> {
    ciphertext: Vec<u8>,
    algorithm: PhantomData<fn() -> Alg>,
}

impl<Alg: SymmetricEncryption> EncryptedString<Alg> {
    /// Encrypt `plaintext`, bound to `associated_data`.
    pub fn encrypt(
        key: &Alg::Key,
        plaintext: &str,
        associated_data: &[u8],
    ) -> Result<Self, EncryptionError> {
        Alg::encrypt(key, plaintext.as_bytes(), associated_data).map(Self::from_ciphertext)
    }

    /// Decrypt with the `associated_data` given to [`encrypt`](Self::encrypt).
    pub fn decrypt(
        &self,
        key: &Alg::Key,
        associated_data: &[u8],
    ) -> Result<Zeroizing<String>, EncryptionError> {
        let mut bytes = Alg::decrypt(key, &self.ciphertext, associated_data)?;
        String::from_utf8(std::mem::take(&mut *bytes))
            .map(Zeroizing::new)
            .map_err(|error| {
                drop(Zeroizing::new(error.into_bytes()));
                EncryptionError::NotUtf8
            })
    }
}

impl<Alg> EncryptedString<Alg> {
    /// Wrap ciphertext produced by `Alg`, e.g. read back from storage.
    pub const fn from_ciphertext(ciphertext: Vec<u8>) -> Self {
        Self {
            ciphertext,
            algorithm: PhantomData,
        }
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

impl<Alg> Clone for EncryptedString<Alg> {
    fn clone(&self) -> Self {
        Self::from_ciphertext(self.ciphertext.clone())
    }
}

impl<Alg> PartialEq for EncryptedString<Alg> {
    fn eq(&self, other: &Self) -> bool {
        self.ciphertext == other.ciphertext
    }
}

impl<Alg> Eq for EncryptedString<Alg> {}

impl<Alg> Debug for EncryptedString<Alg> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "EncryptedString({} bytes)", self.ciphertext.len())
    }
}

impl<DB: Database, Alg> Type<DB> for EncryptedString<Alg>
where
    Vec<u8>: Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <Vec<u8> as Type<DB>>::type_info()
    }

    fn compatible(ty: &DB::TypeInfo) -> bool {
        <Vec<u8> as Type<DB>>::compatible(ty)
    }
}

impl<'q, DB: Database, Alg> Encode<'q, DB> for EncryptedString<Alg>
where
    Vec<u8>: Encode<'q, DB>,
{
    fn encode_by_ref(&self, buf: &mut DB::ArgumentBuffer) -> Result<IsNull, BoxDynError> {
        self.ciphertext.encode_by_ref(buf)
    }
}

impl<'r, DB: Database, Alg> Decode<'r, DB> for EncryptedString<Alg>
where
    Vec<u8>: Decode<'r, DB>,
{
    fn decode(value: DB::ValueRef<'r>) -> Result<Self, BoxDynError> {
        Vec::<u8>::decode(value).map(Self::from_ciphertext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Secret = EncryptedString<DefaultSymmetricEncryption>;

    fn key(byte: u8) -> Key256 {
        Key256::from([byte; 32])
    }

    #[test]
    fn decrypts_with_the_same_key_and_associated_data() {
        let secret = Secret::encrypt(&key(1), "hunter2", b"row-1").unwrap();
        assert_eq!(
            secret.decrypt(&key(1), b"row-1").unwrap().as_str(),
            "hunter2"
        );
    }

    #[test]
    fn ciphertext_does_not_contain_the_plaintext_and_is_randomized() {
        let a = Secret::encrypt(&key(1), "hunter2", b"row-1").unwrap();
        let b = Secret::encrypt(&key(1), "hunter2", b"row-1").unwrap();
        assert_ne!(a, b);
        assert!(!a.ciphertext().windows(7).any(|window| window == b"hunter2"));
    }

    #[test]
    fn rejects_another_key_other_associated_data_and_tampering() {
        let secret = Secret::encrypt(&key(1), "hunter2", b"row-1").unwrap();
        assert!(matches!(
            secret.decrypt(&key(2), b"row-1"),
            Err(EncryptionError::Decrypt)
        ));
        assert!(matches!(
            secret.decrypt(&key(1), b"row-2"),
            Err(EncryptionError::Decrypt)
        ));

        let mut altered = secret.ciphertext().to_vec();
        if let Some(byte) = altered.last_mut() {
            *byte ^= 1;
        }
        assert!(matches!(
            Secret::from_ciphertext(altered).decrypt(&key(1), b"row-1"),
            Err(EncryptionError::Decrypt)
        ));
    }

    #[test]
    fn rejects_truncated_ciphertext() {
        for len in [0, NONCE_LEN, NONCE_LEN + TAG_LEN - 1] {
            let truncated = Secret::from_ciphertext(vec![0; len]);
            assert!(matches!(
                truncated.decrypt(&key(1), b""),
                Err(EncryptionError::Decrypt)
            ));
        }
    }

    #[test]
    fn empty_string_round_trips() {
        let secret = Secret::encrypt(&key(1), "", b"").unwrap();
        assert_eq!(secret.ciphertext().len(), NONCE_LEN + TAG_LEN);
        assert_eq!(secret.decrypt(&key(1), b"").unwrap().as_str(), "");
    }
}
