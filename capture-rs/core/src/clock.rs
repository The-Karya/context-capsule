// context-capsule — core: times — ISO-8601 UTC for files, local-time session ids.

use chrono::{DateTime, FixedOffset, Local, Utc};
use std::time::SystemTime;

/// ISO-8601 UTC, whole seconds: 2026-10-19T10:01:03Z
pub fn iso(t: SystemTime) -> String {
    DateTime::<Utc>::from(t).format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Session id in local time: YYYYMMDD-HHMMSS
pub fn session_id(t: SystemTime) -> String {
    DateTime::<Local>::from(t).format("%Y%m%d-%H%M%S").to_string()
}

/// Session id at a fixed UTC offset (seconds east), for tests and for platforms that know better.
pub fn session_id_at_offset(t: SystemTime, offset_seconds: i32) -> String {
    let tz = FixedOffset::east_opt(offset_seconds).unwrap_or(FixedOffset::east_opt(0).unwrap());
    DateTime::<Utc>::from(t).with_timezone(&tz).format("%Y%m%d-%H%M%S").to_string()
}

/// Milliseconds since 1970 (the `at` of control commands).
pub fn millis(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
