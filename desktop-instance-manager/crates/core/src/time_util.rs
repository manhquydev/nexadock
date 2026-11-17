//! Pure-Rust UTC RFC 3339 and compact backup timestamp helpers.

use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current UTC timestamp formatted as RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`).
#[must_use]
pub fn now_rfc3339_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_seconds_rfc3339(secs)
}

/// Returns a filename-safe compact UTC timestamp (`YYYYMMDDTHHMMSSZ`).
#[must_use]
pub fn now_compact_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_seconds_compact(secs)
}

/// Formats seconds since the Unix epoch into `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn format_unix_seconds_rfc3339(unix_secs: u64) -> String {
    let (year, month, day, hour, minute, second) = decompose_unix_seconds(unix_secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Formats seconds since the Unix epoch into compact `YYYYMMDDTHHMMSSZ`.
#[must_use]
pub fn format_unix_seconds_compact(unix_secs: u64) -> String {
    let (year, month, day, hour, minute, second) = decompose_unix_seconds(unix_secs);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

fn decompose_unix_seconds(unix_secs: u64) -> (i64, u64, u64, u64, u64, u64) {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    // Howard Hinnant civil-from-days algorithm anchored at 1970-01-01.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u64;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u64;
    let year = y + if m <= 2 { 1 } else { 0 };

    (year, m, d, hour, minute, second)
}
