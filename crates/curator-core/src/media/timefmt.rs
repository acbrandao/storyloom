//! Date/time conventions used throughout the catalog.
//!
//! Capture times are kept as the camera's **local wall-clock** time ("YYYY-MM-DDTHH:MM:SS[.fff]")
//! plus an optional UTC offset ("+02:00"). The two are never merged, because many cameras record
//! local time without any timezone, and converting would silently shift the event.

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone, Utc};

const LOCAL_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y:%m:%d %H:%M:%S%.f", // EXIF
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M",
];

/// Parse a local (offset-less) date/time in any of the accepted layouts.
pub fn parse_local(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim().trim_end_matches('\0');
    LOCAL_FORMATS.iter().find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
}

/// Canonical string form, keeping sub-second precision only when present.
pub fn format_local(dt: &NaiveDateTime) -> String {
    let base = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
    let nanos = dt.and_utc().timestamp_subsec_nanos();
    if nanos == 0 {
        base
    } else {
        let ms = nanos / 1_000_000;
        let rem = nanos % 1_000_000;
        if rem == 0 {
            format!("{base}.{ms:03}")
        } else {
            format!("{base}.{:06}", nanos / 1000)
        }
    }
}

pub fn normalize_local(s: &str) -> Option<String> {
    parse_local(s).map(|d| format_local(&d))
}

/// Parse "+02:00", "-0400", "Z" or "+00:00".
pub fn parse_offset(s: &str) -> Option<FixedOffset> {
    let s = s.trim().trim_end_matches('\0');
    if s.eq_ignore_ascii_case("z") {
        return FixedOffset::east_opt(0);
    }
    let (sign, rest) = match s.chars().next()? {
        '+' => (1, &s[1..]),
        '-' => (-1, &s[1..]),
        _ => return None,
    };
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 4 && digits.len() != 2 {
        return None;
    }
    let h: i32 = digits[0..2].parse().ok()?;
    let m: i32 = if digits.len() == 4 { digits[2..4].parse().ok()? } else { 0 };
    if h > 14 || m > 59 {
        return None;
    }
    FixedOffset::east_opt(sign * (h * 3600 + m * 60))
}

pub fn format_offset(o: &FixedOffset) -> String {
    let secs = o.local_minus_utc();
    let sign = if secs < 0 { '-' } else { '+' };
    let secs = secs.abs();
    format!("{sign}{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

/// Unix milliseconds → wall-clock time in this computer's timezone (used only for
/// provisional sorting of files without capture metadata, never stored as a capture time).
pub fn ms_to_local_naive_string(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
        None => "1970-01-01T00:00:00".into(),
    }
}

/// Express a UTC instant in this computer's timezone: (local wall-clock, offset at that instant).
pub fn utc_to_system_local(dt: &DateTime<Utc>) -> (String, String) {
    let local = dt.with_timezone(&Local);
    (format_local(&local.naive_local()), format_offset(local.offset()))
}

pub fn ms_to_iso_utc(ms: i64) -> Option<String> {
    Utc.timestamp_millis_opt(ms).single().map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_and_iso() {
        assert_eq!(normalize_local("2026:07:14 15:32:45").unwrap(), "2026-07-14T15:32:45");
        assert_eq!(normalize_local("2026-07-14T15:32:45.120").unwrap(), "2026-07-14T15:32:45.120");
        assert_eq!(normalize_local("2026-07-14 15:32").unwrap(), "2026-07-14T15:32:00");
        assert!(parse_local("0000:00:00 00:00:00").is_none());
        assert!(parse_local("garbage").is_none());
    }

    #[test]
    fn parses_offsets() {
        assert_eq!(format_offset(&parse_offset("+02:00").unwrap()), "+02:00");
        assert_eq!(format_offset(&parse_offset("-0430").unwrap()), "-04:30");
        assert_eq!(format_offset(&parse_offset("Z").unwrap()), "+00:00");
        assert!(parse_offset("02:00").is_none());
        assert!(parse_offset("+25:00").is_none());
    }
}
