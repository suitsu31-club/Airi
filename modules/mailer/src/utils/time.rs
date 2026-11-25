//! Timestamps as the `mailer` tables store them: UTC, without an offset.

use std::time::SystemTime;
use time::{Duration, OffsetDateTime, PrimitiveDateTime, UtcOffset};

/// `secs` seconds before `at`, in UTC; `None` before the earliest date
/// `time` represents.
pub fn utc_secs_before(at: SystemTime, secs: u32) -> Option<PrimitiveDateTime> {
    let at = OffsetDateTime::from(at).to_offset(UtcOffset::UTC);
    PrimitiveDateTime::new(at.date(), at.time()).checked_sub(Duration::seconds(i64::from(secs)))
}
