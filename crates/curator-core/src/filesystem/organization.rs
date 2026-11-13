//! Physical folder organization: copy or move selected media into a destination folder,
//! optionally into subfolders built from a template such as `{year}/{location}`.
//! Planning only reads; execution happens in `transactions`/the organize worker.

use super::renaming::{self, ConflictPolicy, NameContext, PlanItem, PlanStatus};
use crate::core::errors::{CuratorError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrganizeMode {
    /// Verified copies; originals stay where they are.
    Copy,
    /// Atomic rename on the same drive. Moving between drives is not offered (it would
    /// require deleting originals); use Copy instead.
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub mode: OrganizeMode,
    pub destination: String,
    pub subfolder_template: Option<String>,
    pub items: Vec<PlanItem>,
    pub ready: usize,
    pub skipped: usize,
    pub total_bytes: i64,
    pub free_bytes: Option<u64>,
}

/// Render a subfolder template ("{year}/{location}") into a relative path; empty segments vanish.
pub fn render_subfolders(template: &str, ctx: &NameContext) -> Result<PathBuf> {
    let mut p = PathBuf::new();
    for seg in template.split('/').map(str::trim).filter(|s| !s.is_empty()) {
        match renaming::render(seg, ctx) {
            Ok(s) => p.push(s),
            Err(CuratorError::Invalid(m)) if m.contains("empty name") => {}
            Err(e) => return Err(e),
        }
    }
    Ok(p)
}

#[cfg(unix)]
fn device_of(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    let mut cur = p;
    loop {
        if let Ok(m) = std::fs::metadata(cur) {
            return Some(m.dev());
        }
        cur = cur.parent()?;
    }
}
#[cfg(not(unix))]
fn device_of(p: &Path) -> Option<String> {
    p.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase())
}

pub fn same_volume(a: &Path, b: &Path) -> bool {
    match (device_of(a), device_of(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

pub fn plan(
    conn: &r2d2_sqlite::rusqlite::Connection,
    ids: &[i64],
    destination: &Path,
    subfolder_template: Option<&str>,
    mode: OrganizeMode,
    policy: ConflictPolicy,
    protected_dirs: &[PathBuf],
) -> Result<OrganizePlan> {
    if !destination.is_absolute() {
        return Err(CuratorError::invalid("choose a destination folder"));
    }
    for p in protected_dirs {
        if destination.starts_with(p) {
            return Err(CuratorError::Safety("the destination is inside StoryLoom's own data folders".into()));
        }
    }
    let template = subfolder_template.map(str::trim).filter(|t| !t.is_empty());
    if let Some(t) = template {
        for seg in t.split('/') {
            if !seg.trim().is_empty() {
                renaming::validate_template(seg)?;
            }
        }
    }
    let contexts = renaming::load_contexts(conn, ids)?;
    let mut claimed: HashSet<String> = HashSet::new();
    let mut items = vec![];
    let mut total_bytes = 0i64;
    for (i, (id, from, mut ctx)) in contexts.into_iter().enumerate() {
        ctx.sequence = 1 + i as u64;
        let dir = match template {
            Some(t) => destination.join(render_subfolders(t, &ctx)?),
            None => destination.to_path_buf(),
        };
        let name = from.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut to = dir.join(&name);
        let (mut status, mut message) = (PlanStatus::Ready, None);
        if !from.exists() {
            status = PlanStatus::Missing;
            message = Some("file is missing or its drive is offline".into());
        } else if to == from {
            status = PlanStatus::Unchanged;
            message = Some("already in this folder".into());
        } else if mode == OrganizeMode::Move && !same_volume(&from, destination) {
            status = PlanStatus::Conflict;
            message = Some("on a different drive — use Copy instead (originals are never deleted)".into());
        } else {
            let taken = |p: &Path, claimed: &HashSet<String>| claimed.contains(&p.to_string_lossy().to_lowercase()) || p.exists();
            if taken(&to, &claimed) {
                match policy {
                    ConflictPolicy::Skip => {
                        status = PlanStatus::Conflict;
                        message = Some(format!("“{name}” already exists there"));
                    }
                    ConflictPolicy::AddSuffix => {
                        let base = to.clone();
                        let mut n = 2;
                        while taken(&to, &claimed) {
                            let stem = base.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            let ext = base.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                            to = base.with_file_name(format!("{stem}-{n}{ext}"));
                            n += 1;
                        }
                        message = Some("name was taken; a number was added".into());
                    }
                }
            }
        }
        if status == PlanStatus::Ready {
            claimed.insert(to.to_string_lossy().to_lowercase());
            if mode == OrganizeMode::Copy {
                total_bytes += std::fs::metadata(&from).map(|m| m.len() as i64).unwrap_or(0);
            }
        }
        items.push(PlanItem { media_id: id, from: from.to_string_lossy().into(), to: to.to_string_lossy().into(), status, message });
    }
    let ready = items.iter().filter(|i| i.status == PlanStatus::Ready).count();
    Ok(OrganizePlan {
        mode,
        destination: destination.to_string_lossy().into(),
        subfolder_template: template.map(str::to_string),
        skipped: items.len() - ready,
        items,
        ready,
        total_bytes,
        free_bytes: crate::media::importer::free_space(destination),
    })
}
