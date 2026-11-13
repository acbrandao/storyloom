//! Safe, verified file operations. Source files are only ever *read*.
//!
//! `safe_copy`:
//!  1. copies into a hidden temporary file in the destination folder while hashing the source,
//!  2. flushes to disk and re-hashes the copy (BLAKE3) to verify integrity,
//!  3. applies the source's filesystem timestamps and verifies them,
//!  4. moves the temporary file into place without ever overwriting an existing file.

use super::timestamps::{self, FileTimestamps, TimestampReport};
use crate::core::errors::{CuratorError, Result};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyReport {
    pub source: String,
    pub destination: String,
    pub bytes: u64,
    pub source_hash: String,
    pub destination_hash: String,
    pub source_timestamps: FileTimestamps,
    pub timestamps: TimestampReport,
}

const BUF: usize = 1 << 20;

/// Copy `src` to exactly `dest`. Fails (without touching anything) if `dest` exists.
pub fn safe_copy(src: &Path, dest: &Path, cancel: &CancellationToken) -> Result<CopyReport> {
    if dest.exists() {
        return Err(CuratorError::Safety(format!("destination already exists: {}", dest.display())));
    }
    if same_file(src, dest) {
        return Err(CuratorError::Safety("source and destination are the same file".into()));
    }
    let parent = dest.parent().ok_or_else(|| CuratorError::invalid("destination has no parent folder"))?;
    fs::create_dir_all(parent)?;
    let src_meta = fs::metadata(src)?;
    let src_ts = timestamps::from_metadata(&src_meta);

    let tmp = temp_path(dest);
    let result = (|| -> Result<CopyReport> {
        let mut input = File::open(src)?;
        let mut out = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; BUF];
        let mut bytes = 0u64;
        loop {
            if cancel.is_cancelled() {
                return Err(CuratorError::Cancelled);
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            bytes += n as u64;
        }
        out.sync_all()?;
        drop(out);
        let source_hash = hasher.finalize().to_hex().to_string();
        if bytes != src_meta.len() {
            return Err(CuratorError::Safety(format!("source changed size while copying ({} vs {} bytes)", bytes, src_meta.len())));
        }
        let destination_hash = crate::media::fingerprints::hash_file(&tmp, cancel)?;
        if destination_hash != source_hash {
            return Err(CuratorError::Safety("verification failed: the copy does not match the original".into()));
        }
        // Permissions: keep read/write bits of the source.
        let _ = fs::set_permissions(&tmp, src_meta.permissions());
        let ts_report = timestamps::apply(&tmp, &src_ts)?;
        place_without_overwrite(&tmp, dest)?;
        // Re-verify after placing (rename must not alter timestamps, but never assume).
        let actual = timestamps::read(dest)?;
        let ts_report = TimestampReport { actual, ..ts_report };
        Ok(CopyReport {
            source: src.to_string_lossy().into(),
            destination: dest.to_string_lossy().into(),
            bytes,
            source_hash,
            destination_hash,
            source_timestamps: src_ts,
            timestamps: ts_report,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Atomically move `tmp` to `dest`, failing if `dest` exists. Uses a hard link where the
/// filesystem supports it (atomic no-clobber), otherwise an existence check + rename.
fn place_without_overwrite(tmp: &Path, dest: &Path) -> Result<()> {
    match fs::hard_link(tmp, dest) {
        Ok(()) => {
            fs::remove_file(tmp)?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(CuratorError::Safety(format!("destination appeared during copy: {}", dest.display())))
        }
        Err(_) => {
            // e.g. exFAT/FAT/SMB volumes without hard links.
            if dest.exists() {
                return Err(CuratorError::Safety(format!("destination appeared during copy: {}", dest.display())));
            }
            fs::rename(tmp, dest)?;
            Ok(())
        }
    }
}

fn temp_path(dest: &Path) -> PathBuf {
    let name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    dest.with_file_name(format!(".{name}.mcpartial-{nonce:x}"))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Pick a free filename in `dir`: "name.ext", then "name (2).ext", "name (3).ext"...
pub fn unique_destination(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(filename);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for i in 2.. {
        let c = dir.join(format!("{stem} ({i}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    unreachable!()
}
