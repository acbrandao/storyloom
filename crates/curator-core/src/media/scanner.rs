//! Recursive, read-only discovery of media files. Never modifies anything on disk.

use super::media_types;
use crate::database::models::MediaType;
use crate::filesystem::timestamps;
use std::path::Path;
use tokio_util::sync::CancellationToken;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: String,
    pub filename: String,
    pub folder: String,
    pub extension: String,
    pub media_type: MediaType,
    pub mime_type: Option<String>,
    pub size: i64,
    pub mtime_ms: i64,
    pub birth_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Inaccessible,
    Unsupported,
    Empty,
}

impl IssueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueKind::Inaccessible => "inaccessible",
            IssueKind::Unsupported => "unsupported",
            IssueKind::Empty => "empty",
        }
    }
}

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCounts {
    pub media_found: u64,
    pub unsupported: u64,
    pub inaccessible: u64,
    pub empty: u64,
}

/// Folder names that never contain user media (OS metadata, NAS indexes, recycle bins).
const SKIP_DIRS: &[&str] = &[
    "@eaDir", "$RECYCLE.BIN", "System Volume Information", "#recycle", "#snapshot", "node_modules", "Thumbs", ".thumbnails",
];
/// Files ignored silently (not reported as unsupported).
const IGNORED_FILES: &[&str] = &["thumbs.db", "desktop.ini", "icon\r"];
const IGNORED_EXTENSIONS: &[&str] = &["xmp", "thm", "aae", "lrf", "sav", "ini", "db", "txt", "json", "plist", "log", "csv", "pdf"];

fn is_skipped_dir(e: &DirEntry) -> bool {
    if e.depth() == 0 {
        return false;
    }
    let name = e.file_name().to_string_lossy();
    name.starts_with('.') || SKIP_DIRS.iter().any(|d| name.eq_ignore_ascii_case(d)) || name.ends_with(".photoslibrary") || name.ends_with(".app")
}

/// Walk `root` and call `on_file` for every supported media file found.
pub fn scan_directory(
    root: &Path,
    cancel: &CancellationToken,
    mut on_file: impl FnMut(ScannedFile),
    mut on_issue: impl FnMut(&str, IssueKind, Option<String>),
) -> ScanCounts {
    let mut counts = ScanCounts::default();
    let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| !(e.file_type().is_dir() && is_skipped_dir(e)));
    for entry in walker {
        if cancel.is_cancelled() {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                let p = err.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                counts.inaccessible += 1;
                on_issue(&p, IssueKind::Inaccessible, Some(err.to_string()));
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        // macOS AppleDouble resource forks ("._IMG_0001.JPG") and hidden files.
        if filename.starts_with('.') || IGNORED_FILES.contains(&filename.to_ascii_lowercase().as_str()) {
            continue;
        }
        let path = entry.path();
        let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let Some(media_type) = media_types::classify(&extension) else {
            if !extension.is_empty() && !IGNORED_EXTENSIONS.contains(&extension.as_str()) {
                counts.unsupported += 1;
                on_issue(&path.to_string_lossy(), IssueKind::Unsupported, Some(format!(".{extension} files are not supported")));
            }
            continue;
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(err) => {
                counts.inaccessible += 1;
                on_issue(&path.to_string_lossy(), IssueKind::Inaccessible, Some(err.to_string()));
                continue;
            }
        };
        if meta.len() == 0 {
            counts.empty += 1;
            on_issue(&path.to_string_lossy(), IssueKind::Empty, Some("file is empty (possibly an incomplete copy)".into()));
            continue;
        }
        let ts = timestamps::from_metadata(&meta);
        counts.media_found += 1;
        on_file(ScannedFile {
            path: path.to_string_lossy().to_string(),
            filename,
            folder: path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            mime_type: media_types::mime_for(&extension).map(str::to_string),
            extension,
            media_type,
            size: meta.len() as i64,
            mtime_ms: ts.modified_ms,
            birth_ms: ts.created_ms,
        });
    }
    counts
}
