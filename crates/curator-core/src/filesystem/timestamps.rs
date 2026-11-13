//! Filesystem timestamp reading, applying and verification.
//!
//! These are *filesystem* timestamps (birth/creation and modification). They are kept
//! strictly separate from *capture* timestamps (when the photo/video was taken), which
//! live in embedded metadata and the catalog.

use crate::platform::{self, BirthTimeSupport};
use filetime::FileTime;
use serde::Serialize;
use std::fs::Metadata;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTimestamps {
    /// Birth/creation time in unix ms, when the filesystem records one.
    pub created_ms: Option<i64>,
    pub modified_ms: i64,
}

fn system_time_ms(t: std::time::SystemTime) -> i64 {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

pub fn from_metadata(meta: &Metadata) -> FileTimestamps {
    FileTimestamps {
        created_ms: meta.created().ok().map(system_time_ms),
        modified_ms: meta.modified().map(system_time_ms).unwrap_or(0),
    }
}

pub fn read(path: &Path) -> std::io::Result<FileTimestamps> {
    Ok(from_metadata(&std::fs::metadata(path)?))
}

/// What happened when applying timestamps to a file. Serialized into operation journals.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampReport {
    pub requested: FileTimestamps,
    pub actual: FileTimestamps,
    pub modified_preserved: bool,
    /// None when the source had no birth time to preserve.
    pub created_preserved: Option<bool>,
    pub birth_time_support: Option<BirthTimeSupport>,
    /// Human-readable explanation for anything that could not be preserved.
    pub warnings: Vec<String>,
}

impl TimestampReport {
    pub fn fully_preserved(&self) -> bool {
        self.modified_preserved && self.created_preserved.unwrap_or(true)
    }
}

/// Many filesystems store coarser times (FAT: 2 s modification resolution; SMB/exFAT vary).
const TOLERANCE_MS: i64 = 2000;

/// Apply `ts` to `path` and verify by reading back. Never claims success it cannot verify.
pub fn apply(path: &Path, ts: &FileTimestamps) -> std::io::Result<TimestampReport> {
    let mut warnings = vec![];
    // Modification time first: on macOS lowering mtime below the birth time also lowers the
    // birth time, so the birth time is applied last.
    let mtime = FileTime::from_unix_time(ts.modified_ms.div_euclid(1000), (ts.modified_ms.rem_euclid(1000) * 1_000_000) as u32);
    filetime::set_file_mtime(path, mtime)?;
    let birth_support = ts.created_ms.map(|c| platform::set_birth_time(path, c));
    match &birth_support {
        Some(BirthTimeSupport::Unsupported(why)) => warnings.push(format!("Creation date could not be preserved: {why}")),
        Some(BirthTimeSupport::Failed(why)) => warnings.push(format!("Setting the creation date failed: {why}")),
        _ => {}
    }
    let actual = read(path)?;
    let modified_preserved = (actual.modified_ms - ts.modified_ms).abs() <= TOLERANCE_MS;
    if !modified_preserved {
        warnings.push("Modification date could not be preserved on this volume".into());
    }
    let created_preserved = ts.created_ms.map(|want| match actual.created_ms {
        Some(got) => (got - want).abs() <= TOLERANCE_MS,
        None => false,
    });
    if created_preserved == Some(false) && !matches!(birth_support, Some(BirthTimeSupport::Unsupported(_)) | Some(BirthTimeSupport::Failed(_))) {
        warnings.push("Creation date was set but the volume reports a different value".into());
    }
    Ok(TimestampReport { requested: *ts, actual, modified_preserved, created_preserved, birth_time_support: birth_support, warnings })
}

/// Compare two timestamp sets within filesystem tolerance.
pub fn verify(expected: &FileTimestamps, path: &Path) -> std::io::Result<bool> {
    let actual = read(path)?;
    let m = (actual.modified_ms - expected.modified_ms).abs() <= TOLERANCE_MS;
    let c = match (expected.created_ms, actual.created_ms) {
        (Some(a), Some(b)) => (a - b).abs() <= TOLERANCE_MS,
        (None, _) => true,
        (Some(_), None) => false,
    };
    Ok(m && c)
}
