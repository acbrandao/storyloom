//! Capture-time arithmetic for derived clips:
//!
//! ```text
//! New Capture Time = Original Capture Time + Clip In-Point
//! ```
//!
//! The source's timezone offset (if any) is carried over unchanged and sub-second precision
//! is preserved. If the source has no known capture time the clip has none either; the
//! export date is never substituted.

use crate::media::timefmt;
use chrono::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipCaptureTime {
    pub local: String,
    pub offset: Option<String>,
}

/// `source_local`: "YYYY-MM-DDTHH:MM:SS[.fff]"; `in_point_seconds` may be fractional.
pub fn clip_capture_time(source_local: &str, source_offset: Option<&str>, in_point_seconds: f64) -> Option<ClipCaptureTime> {
    if !in_point_seconds.is_finite() || in_point_seconds < 0.0 {
        return None;
    }
    let base = timefmt::parse_local(source_local)?;
    let micros = (in_point_seconds * 1_000_000.0).round() as i64;
    let dt = base.checked_add_signed(Duration::microseconds(micros))?;
    Some(ClipCaptureTime {
        local: timefmt::format_local(&dt),
        offset: source_offset.and_then(timefmt::parse_offset).map(|o| timefmt::format_offset(&o)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example() {
        let c = clip_capture_time("2026-07-14T15:32:45", None, 4.0 * 60.0 + 30.0).unwrap();
        assert_eq!(c.local, "2026-07-14T15:37:15");
        assert_eq!(c.offset, None);
    }

    #[test]
    fn keeps_offset_and_precision() {
        let c = clip_capture_time("2026-07-14T15:32:45.250", Some("-03:00"), 72.5).unwrap();
        assert_eq!(c.local, "2026-07-14T15:33:57.750");
        assert_eq!(c.offset.as_deref(), Some("-03:00"));
    }

    #[test]
    fn crosses_midnight_and_month() {
        let c = clip_capture_time("2026-07-31T23:59:30", Some("+02:00"), 45.0).unwrap();
        assert_eq!(c.local, "2026-08-01T00:00:15");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(clip_capture_time("not a date", None, 1.0).is_none());
        assert!(clip_capture_time("2026-07-14T15:32:45", None, -1.0).is_none());
        assert!(clip_capture_time("2026-07-14T15:32:45", None, f64::NAN).is_none());
    }

    #[test]
    fn zero_in_point_is_identity() {
        let c = clip_capture_time("2026-07-14T15:32:45", Some("+00:00"), 0.0).unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        assert_eq!(c.offset.as_deref(), Some("+00:00"));
    }
}
