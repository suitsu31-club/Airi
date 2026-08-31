//! Small date/time helpers shared across the module.
//!
//! Sessions and other rows store naive `PrimitiveDateTime` values interpreted as
//! UTC; these helpers bridge to `OffsetDateTime`, unix seconds, and
//! `PgInterval`.

use sqlx::postgres::types::PgInterval;
use time::{OffsetDateTime, PrimitiveDateTime};

/// The current time as a naive (UTC) `PrimitiveDateTime`.
pub fn now_primitive() -> PrimitiveDateTime {
    let now = OffsetDateTime::now_utc();
    PrimitiveDateTime::new(now.date(), now.time())
}

/// Convert a `PrimitiveDateTime` (assumed UTC) to unix seconds.
pub fn to_unix(dt: PrimitiveDateTime) -> u64 {
    dt.assume_utc().unix_timestamp().max(0) as u64
}

/// Convert unix seconds to a naive (UTC) `PrimitiveDateTime`.
pub fn from_unix(secs: u64) -> PrimitiveDateTime {
    let odt =
        OffsetDateTime::from_unix_timestamp(secs as i64).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    PrimitiveDateTime::new(odt.date(), odt.time())
}

/// Build a `PgInterval` spanning the given number of seconds.
pub fn pg_interval_from_secs(secs: u64) -> PgInterval {
    PgInterval {
        months: 0,
        days: 0,
        microseconds: (secs as i64).saturating_mul(1_000_000),
    }
}
