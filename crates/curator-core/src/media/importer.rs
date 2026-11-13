//! "Import / Ingest": copy media from a card, phone or folder into a managed library folder.
//! Sources are only read. Every copy is hash-verified, timestamp-preserving and journaled.

use super::{metadata, scanner};
use crate::core::errors::{CuratorError, Result};
use crate::filesystem::operations::{self, CopyReport};
use crate::tools::Tools;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum IngestLayout {
    /// `Library/2026/2026-07-14/IMG_0001.JPG` based on capture date ("Unknown Date" if none).
    #[default]
    ByCaptureDate,
    /// `Library/<source folder name>/<same subfolders>/IMG_0001.JPG`
    KeepFolderStructure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestItem {
    pub source: String,
    pub destination: String,
    pub size: i64,
    /// True when an identical file already exists at the destination (it will be skipped).
    pub already_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestPlan {
    pub source_dir: String,
    pub library_dir: String,
    pub layout: IngestLayout,
    /// Optional filename template applied to the copies (originals keep their names).
    #[serde(default)]
    pub rename_template: Option<String>,
    pub items: Vec<IngestItem>,
    pub total_bytes: i64,
    pub to_copy: usize,
    pub already_present: usize,
    pub free_bytes: Option<u64>,
}

/// Build a preview of what an ingest would do. Reads files only.
pub fn plan(source_dir: &Path, library_dir: &Path, layout: IngestLayout, rename_template: Option<&str>, tools: &Tools, cancel: &CancellationToken) -> Result<IngestPlan> {
    let rename_template = rename_template.map(str::trim).filter(|t| !t.is_empty());
    if let Some(t) = rename_template {
        crate::filesystem::renaming::validate_template(t)?;
    }
    if !source_dir.is_dir() {
        return Err(CuratorError::NotFound(format!("folder not found: {}", source_dir.display())));
    }
    let src_canon = source_dir.canonicalize()?;
    if let Ok(lib) = library_dir.canonicalize() {
        if lib.starts_with(&src_canon) || src_canon.starts_with(&lib) {
            return Err(CuratorError::Safety("the library folder must not be inside the source folder (or vice versa)".into()));
        }
    }
    let mut files = vec![];
    scanner::scan_directory(source_dir, cancel, |f| files.push(f), |_, _, _| {});
    if cancel.is_cancelled() {
        return Err(CuratorError::Cancelled);
    }
    let src_name = source_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Import".into());
    // Read capture metadata when the layout or the name template needs it.
    let need_meta = layout == IngestLayout::ByCaptureDate || rename_template.is_some();
    let mut with_meta: Vec<(scanner::ScannedFile, Option<metadata::ExtractedMetadata>)> = files
        .into_iter()
        .map(|f| {
            let m = if need_meta { metadata::extract(Path::new(&f.path), f.media_type, tools).ok() } else { None };
            (f, m)
        })
        .collect();
    if cancel.is_cancelled() {
        return Err(CuratorError::Cancelled);
    }
    // Chronological order so {sequence} numbers follow the story.
    with_meta.sort_by(|a, b| {
        let ka = a.1.as_ref().and_then(|m| m.capture.as_ref()).map(|c| c.local.clone());
        let kb = b.1.as_ref().and_then(|m| m.capture.as_ref()).map(|c| c.local.clone());
        ka.is_none().cmp(&kb.is_none()).then(ka.cmp(&kb)).then(a.0.path.cmp(&b.0.path))
    });
    let mut items = vec![];
    let mut planned: std::collections::HashSet<PathBuf> = Default::default();
    for (seq, (f, meta)) in with_meta.into_iter().enumerate() {
        let capture = meta.as_ref().and_then(|m| m.capture.clone());
        let rel_dir: PathBuf = match layout {
            IngestLayout::ByCaptureDate => match &capture {
                Some(c) => PathBuf::from(&c.local[0..4]).join(&c.local[0..10]),
                None => PathBuf::from("Unknown Date"),
            },
            IngestLayout::KeepFolderStructure => {
                let rel = Path::new(&f.folder).strip_prefix(source_dir).unwrap_or(Path::new(""));
                PathBuf::from(&src_name).join(rel)
            }
        };
        let filename = match rename_template {
            Some(t) => {
                let stem = Path::new(&f.filename).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let ctx = crate::filesystem::renaming::NameContext {
                    capture_local: capture.as_ref().map(|c| c.local.clone()),
                    original_stem: stem.clone(),
                    current_stem: stem,
                    camera: meta.as_ref().and_then(|m| m.camera_model.clone()),
                    media_type: f.media_type.as_str().into(),
                    sequence: seq as u64 + 1,
                    ..Default::default()
                };
                let ext = Path::new(&f.filename).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                format!("{}{ext}", crate::filesystem::renaming::render(t, &ctx)?)
            }
            None => f.filename.clone(),
        };
        let dir = library_dir.join(rel_dir);
        let mut dest = dir.join(&filename);
        let mut already_present = false;
        if dest.exists() {
            if std::fs::metadata(&dest).map(|m| m.len() as i64 == f.size).unwrap_or(false)
                && crate::media::fingerprints::hash_file(&dest, cancel)? == crate::media::fingerprints::hash_file(Path::new(&f.path), cancel)?
            {
                already_present = true;
            } else {
                dest = operations::unique_destination(&dir, &filename);
            }
        }
        // Two sources mapping to the same new name within this plan.
        while !already_present && planned.contains(&dest) {
            let stem = dest.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = dest.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            dest = operations::unique_destination(&dir, &format!("{stem}-1{ext}"));
        }
        planned.insert(dest.clone());
        items.push(IngestItem { source: f.path, destination: dest.to_string_lossy().into(), size: f.size, already_present });
    }
    let total_bytes = items.iter().filter(|i| !i.already_present).map(|i| i.size).sum();
    let already = items.iter().filter(|i| i.already_present).count();
    Ok(IngestPlan {
        source_dir: source_dir.to_string_lossy().into(),
        library_dir: library_dir.to_string_lossy().into(),
        layout,
        rename_template: rename_template.map(str::to_string),
        to_copy: items.len() - already,
        already_present: already,
        items,
        total_bytes,
        free_bytes: free_space(library_dir),
    })
}

pub fn copy_item(item: &IngestItem, cancel: &CancellationToken) -> Result<CopyReport> {
    operations::safe_copy(Path::new(&item.source), Path::new(&item.destination), cancel)
}

/// Free space on the volume holding `dir` (or its nearest existing ancestor).
pub fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let mut p = dir;
        while !p.exists() {
            p = p.parent()?;
        }
        let c = CString::new(p.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 {
            return Some(st.f_bavail as u64 * st.f_frsize as u64);
        }
        None
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        None
    }
}
