//! Email addresses.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Type};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// A syntactically valid mailbox address, `local-part@domain`.
///
/// Accepts the addresses SMTP relays and mail clients handle everywhere:
///
/// - the local part is a dot-atom of at most 64 characters: ASCII letters,
///   digits and ``!#$%&'*+-/=?^_`{|}~``, in dot-separated non-empty runs;
/// - the domain is a host name of at most 253 characters: dot-separated
///   labels of 1–63 ASCII letters, digits and hyphens, not starting or ending
///   with a hyphen;
/// - the whole address is at most 254 characters.
///
/// Quoted local parts, IP-literal domains and non-ASCII addresses
/// (SMTPUTF8) are rejected; internationalized domains must be given in their
/// `xn--` form. The domain is stored lowercase, the local part as given.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EmailAddress(String);

const MAX_ADDRESS_LEN: usize = 254;
const MAX_LOCAL_LEN: usize = 64;
const MAX_DOMAIN_LEN: usize = 253;
const MAX_LABEL_LEN: usize = 63;

impl EmailAddress {
    /// The address, or `None` if `s` is not a valid address.
    pub fn new(s: impl AsRef<str>) -> Option<Self> {
        let s = s.as_ref();
        if s.len() > MAX_ADDRESS_LEN {
            return None;
        }
        let (local, domain) = s.split_once('@')?;
        if !is_local_part(local) || !is_domain(domain) {
            return None;
        }
        let mut address = s.to_owned();
        let (_, domain) = address.split_at_mut(local.len());
        domain.make_ascii_lowercase();
        Some(Self(address))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_local_part(local: &str) -> bool {
    const SPECIALS: &[u8] = b"!#$%&'*+-/=?^_`{|}~";
    local.len() <= MAX_LOCAL_LEN
        && local.split('.').all(|atom| {
            !atom.is_empty()
                && atom
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || SPECIALS.contains(&b))
        })
}

fn is_domain(domain: &str) -> bool {
    domain.len() <= MAX_DOMAIN_LEN
        && domain.split('.').all(|label| {
            label.len() <= MAX_LABEL_LEN
                && !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Returned when a string is not a valid [`EmailAddress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid email address")]
pub struct InvalidEmailAddress;

impl FromStr for EmailAddress {
    type Err = InvalidEmailAddress;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s).ok_or(InvalidEmailAddress)
    }
}

impl AsRef<str> for EmailAddress {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for EmailAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<EmailAddress> for String {
    fn from(value: EmailAddress) -> Self {
        value.0
    }
}

impl Serialize for EmailAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EmailAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Self::new(s).ok_or_else(|| serde::de::Error::custom(InvalidEmailAddress))
    }
}

/// Stored as text; a stored value that is not a valid address fails to decode.
impl<DB: Database> Type<DB> for EmailAddress
where
    String: Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <String as Type<DB>>::type_info()
    }

    fn compatible(ty: &DB::TypeInfo) -> bool {
        <String as Type<DB>>::compatible(ty)
    }
}

impl<'q, DB: Database> Encode<'q, DB> for EmailAddress
where
    String: Encode<'q, DB>,
{
    fn encode_by_ref(&self, buf: &mut DB::ArgumentBuffer) -> Result<IsNull, BoxDynError> {
        self.0.encode_by_ref(buf)
    }
}

impl<'r, DB: Database> Decode<'r, DB> for EmailAddress
where
    &'r str: Decode<'r, DB>,
{
    fn decode(value: DB::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let s = <&str>::decode(value)?;
        Ok(Self::from_str(s)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_addresses() {
        for address in [
            "user@example.com",
            "first.last@sub.example.co.jp",
            "user+tag@example.com",
            "o'brien@example.org",
            "x@localhost",
            "a!#$%&'*+-/=?^_`{|}~b@xn--bcher-kva.example",
        ] {
            assert_eq!(
                EmailAddress::new(address).map(String::from).as_deref(),
                Some(address),
                "{address}"
            );
        }
    }

    #[test]
    fn rejects_malformed_addresses() {
        let long_label = format!("user@{}.com", "a".repeat(64));
        let long_local = format!("{}@example.com", "a".repeat(65));
        // Local part and domain are each within their limits, the whole is not.
        let long_address = format!("{}@{}", "a".repeat(64), vec!["b".repeat(62); 4].join("."));
        for address in [
            "",
            "user",
            "@example.com",
            "user@",
            "user@@example.com",
            "a@b@example.com",
            ".user@example.com",
            "user.@example.com",
            "us..er@example.com",
            "\"quoted\"@example.com",
            "user name@example.com",
            "user@exam ple.com",
            "user@-example.com",
            "user@example-.com",
            "user@example..com",
            "user@example.com.",
            "user@[127.0.0.1]",
            "user@exämple.com",
            "üser@example.com",
            " user@example.com",
            "user@example.com\n",
            &long_label,
            &long_local,
            &long_address,
        ] {
            assert_eq!(EmailAddress::new(address), None, "{address:?}");
        }
    }

    #[test]
    fn lowercases_only_the_domain() {
        let address = EmailAddress::new("John.Doe@Example.COM").unwrap();
        assert_eq!(address.as_str(), "John.Doe@example.com");
    }

    #[test]
    fn deserializing_validates() {
        let address: EmailAddress = serde_json::from_str(r#""a@Example.com""#).unwrap();
        assert_eq!(address.as_str(), "a@example.com");
        assert!(serde_json::from_str::<EmailAddress>(r#""not an address""#).is_err());
    }
}
