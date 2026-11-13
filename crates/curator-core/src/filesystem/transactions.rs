//! Journaled, reversible filesystem operations on catalog media.
//!
//! Every operation is written to `file_operations` (status `pending`) *before* it touches the
//! disk, then completed with verification results. Renames/moves never overwrite: they use a
//! hard link + unlink (atomic no-clobber, same inode so all timestamps are kept), falling back
//! to an existence check + rename on volumes without hard links. Moves never cross drives.

use super::renaming::{PlanItem, PlanStatus};
use super::timestamps::{self, FileTimestamps};
use crate::core::errors::{CuratorError, Result};
use crate::database::now_ms;
use crate::database::repositories::{media as media_repo, operations as journal, sources};
use crate::database::Database;
use r2d2_sqlite::rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
#[allow(unused_imports)]
use std::ptr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExecReport {
    pub batch_id: String,
    pub done: usize,
    pub skipped: usize,
    pub failed: Vec<(String, String)>,
    pub timestamp_warnings: Vec<String>,
}

/// Move/rename `src` to `dst` on the same volume without ever replacing an existing file.
/// Returns the timestamps before and after (which must match).
pub fn move_no_clobber(src: &Path, dst: &Path) -> Result<(FileTimestamps, FileTimestamps)> {
    let before = timestamps::read(src)?;
    let size = std::fs::metadata(src)?.len();
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let same_file = super::renaming::occupied_by_other(dst, src) == false && dst.exists();
    if same_file {
        // Case-only rename on a case-insensitive volume: same inode, rename is safe.
        std::fs::rename(src, dst)?;
    } else if dst.exists() {
        return Err(CuratorError::Safety(format!("“{}” already exists; nothing was changed", dst.display())));
    } else {
        match std::fs::hard_link(src, dst) {
            Ok(()) => {
                if let Err(e) = std::fs::remove_file(src) {
                    let _ = std::fs::remove_file(dst); // restore the single original name
                    return Err(e.into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(CuratorError::Safety(format!("“{}” appeared during the operation; nothing was changed", dst.display())));
            }
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                return Err(CuratorError::Safety("source and destination are on different drives".into()));
            }
            Err(_) => {
                // No hard links on this volume (exFAT, some network shares).
                if dst.exists() {
                    return Err(CuratorError::Safety(format!("“{}” already exists; nothing was changed", dst.display())));
                }
                std::fs::rename(src, dst)?;
            }
        }
    }
    let after = timestamps::read(dst)?;
    if std::fs::metadata(dst)?.len() != size {
        return Err(CuratorError::Safety("file size changed during the operation".into()));
    }
    Ok((before, after))
}

fn update_media_path(conn: &Connection, media_id: i64, new_path: &Path) -> Result<()> {
    let name = new_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let folder = new_path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    // Keep the record attached to the source folder that now contains it.
    let source_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM sources WHERE ?1 = path OR ?1 LIKE path || '/%' ORDER BY length(path) DESC LIMIT 1",
            [&folder],
            |r| r.get(0),
        )
        .optional()?;
    conn.execute(
        "UPDATE media SET current_path = ?2, current_filename = ?3, folder_path = ?4, source_id = COALESCE(?5, source_id),
            is_missing = 0, updated_at = ?6 WHERE id = ?1",
        params![media_id, new_path.to_string_lossy(), name, folder, source_id, now_ms()],
    )?;
    media_repo::refresh_fts(conn, media_id)?;
    Ok(())
}

/// Apply a rename or move plan. `kind` is "rename" or "move".
///
/// Items are ordered so a file is renamed away before another takes its name (A→B, B→C).
/// Cycles (A↔B) are broken by parking one file under a hidden temporary name, letting the
/// others proceed, then finishing it. The journal always records the true original path.
pub fn execute_moves(db: &Database, items: &[PlanItem], kind: &str, batch_id: &str, mut progress: impl FnMut(usize, usize)) -> Result<ExecReport> {
    struct Work {
        media_id: i64,
        original: PathBuf,
        current: PathBuf,
        to: PathBuf,
        op: Option<i64>,
        before: Option<FileTimestamps>,
    }
    let mut report = ExecReport { batch_id: batch_id.into(), ..Default::default() };
    let mut pending: Vec<Work> = items
        .iter()
        .filter(|i| i.status == PlanStatus::Ready)
        .map(|i| Work { media_id: i.media_id, original: PathBuf::from(&i.from), current: PathBuf::from(&i.from), to: PathBuf::from(&i.to), op: None, before: None })
        .collect();
    report.skipped = items.len() - pending.len();
    let total = pending.len();
    let conn = db.conn()?;

    let fail = |w: &Work, e: &CuratorError, report: &mut ExecReport, conn: &Connection| -> Result<()> {
        if let Some(op) = w.op {
            journal::fail(conn, op, &e.to_string())?;
        }
        // A parked file goes back to its original name (on disk and in the catalog).
        if w.current != w.original && move_no_clobber(&w.current, &w.original).is_ok() {
            update_media_path(conn, w.media_id, &w.original)?;
        }
        report.failed.push((w.original.to_string_lossy().into(), e.to_string()));
        Ok(())
    };

    while !pending.is_empty() {
        // A target is blocked if it is (by path or by file identity) another pending file.
        let occupied: HashSet<PathBuf> = pending.iter().map(|w| w.current.clone()).collect();
        let blocked = |w: &Work| {
            if w.to.to_string_lossy().eq_ignore_ascii_case(&w.current.to_string_lossy()) {
                return false; // case-only rename of itself
            }
            occupied.contains(&w.to) || pending.iter().any(|o| !std::ptr::eq(o, w) && w.to.exists() && !super::renaming::occupied_by_other(&w.to, &o.current))
        };
        let free = pending.iter().position(|w| !blocked(w));
        let idx = free.unwrap_or(0);
        let mut w = pending.remove(idx);
        if w.op.is_none() {
            let previous = json!({ "timestamps": timestamps::read(&w.original).ok(), "size": std::fs::metadata(&w.original).map(|m| m.len()).ok() });
            w.op = Some(journal::begin(&conn, batch_id, kind, Some(w.media_id), &w.original.to_string_lossy(), Some(&w.to.to_string_lossy()), &previous)?);
        }
        if free.is_none() {
            // Every remaining target is held by another pending file: park this one.
            let tmp = w.current.with_file_name(format!(".mc-rename-{}-{}", now_ms(), w.current.file_name().unwrap_or_default().to_string_lossy()));
            match move_no_clobber(&w.current, &tmp) {
                Ok((b, _)) => {
                    w.before.get_or_insert(b);
                    // The catalog follows the file so the freed name can be claimed by another record.
                    update_media_path(&conn, w.media_id, &tmp)?;
                    w.current = tmp;
                    pending.push(w);
                }
                Err(e) => fail(&w, &e, &mut report, &conn)?,
            }
            continue;
        }
        match move_no_clobber(&w.current, &w.to) {
            Ok((b, after)) => {
                let before = w.before.unwrap_or(b);
                let verified = before == after;
                if !verified {
                    report.timestamp_warnings.push(format!("{}: file dates changed unexpectedly", w.to.display()));
                }
                update_media_path(&conn, w.media_id, &w.to)?;
                journal::complete(&conn, w.op.unwrap(), &json!({ "timestampsBefore": before, "timestampsAfter": after, "timestampsVerified": verified }))?;
                report.done += 1;
            }
            Err(e) => fail(&w, &e, &mut report, &conn)?,
        }
        progress(report.done + report.failed.len(), total);
    }
    Ok(report)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub batch_id: String,
    pub operation_type: String,
    pub created_at: i64,
    pub completed: i64,
    pub failed: i64,
    pub undone: i64,
    pub undoable: bool,
}

pub fn batches(conn: &Connection, limit: i64) -> Result<Vec<BatchSummary>> {
    let mut stmt = conn.prepare(
        "SELECT batch_id, MIN(operation_type), MIN(created_at),
            SUM(operation_status = 'completed'), SUM(operation_status = 'failed'), SUM(operation_status = 'undone')
         FROM file_operations WHERE batch_id IS NOT NULL AND operation_type IN ('rename','move','copy')
         GROUP BY batch_id ORDER BY MIN(id) DESC LIMIT ?1",
    )?;
    let v = stmt
        .query_map([limit], |r| {
            let completed: i64 = r.get(3)?;
            let t: String = r.get(1)?;
            Ok(BatchSummary {
                batch_id: r.get(0)?,
                undoable: completed > 0,
                operation_type: t,
                created_at: r.get(2)?,
                completed,
                failed: r.get(4)?,
                undone: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(v)
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub undone: usize,
    pub skipped: Vec<(String, String)>,
}

/// Reverse a batch. Renames/moves are moved back (never overwriting). Copies are moved to the
/// Trash — only if they are still byte-identical to what was copied — and their catalog records removed.
pub fn undo_batch(db: &Database, batch_id: &str) -> Result<UndoReport> {
    let conn = db.conn()?;
    let ops: Vec<(i64, String, Option<i64>, String, Option<String>, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, operation_type, media_id, original_path, destination_path, result_metadata FROM file_operations
             WHERE batch_id = ?1 AND operation_status = 'completed' ORDER BY id DESC",
        )?;
        let v = stmt.query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<std::result::Result<Vec<_>, _>>()?;
        v
    };
    if ops.is_empty() {
        return Err(CuratorError::invalid("nothing in this batch can be undone"));
    }
    let undo_batch_id = format!("undo-{batch_id}");
    let mut report = UndoReport::default();
    for (op_id, kind, media_id, original, destination, result) in ops {
        let Some(dest) = destination else { continue };
        let (dest, original) = (PathBuf::from(dest), PathBuf::from(original));
        let outcome: Result<()> = (|| match kind.as_str() {
            "rename" | "move" => {
                if !dest.exists() {
                    return Err(CuratorError::invalid("the file is no longer where it was moved to"));
                }
                if original.exists() && super::renaming::occupied_by_other(&original, &dest) {
                    return Err(CuratorError::invalid("another file now has the original name"));
                }
                let jid = journal::begin(&conn, &undo_batch_id, &format!("undo-{kind}"), media_id, &dest.to_string_lossy(), Some(&original.to_string_lossy()), &json!({}))?;
                match move_no_clobber(&dest, &original) {
                    Ok((b, a)) => {
                        if let Some(m) = media_id {
                            update_media_path(&conn, m, &original)?;
                        }
                        journal::complete(&conn, jid, &json!({ "timestampsBefore": b, "timestampsAfter": a, "timestampsVerified": b == a }))?;
                        Ok(())
                    }
                    Err(e) => {
                        journal::fail(&conn, jid, &e.to_string())?;
                        Err(e)
                    }
                }
            }
            "copy" => {
                let recorded: Option<String> = result.as_deref().and_then(|r| serde_json::from_str::<serde_json::Value>(r).ok()).and_then(|v| v["destinationHash"].as_str().map(str::to_string));
                if !dest.exists() {
                    return Err(CuratorError::invalid("the copy no longer exists"));
                }
                let now = crate::media::fingerprints::hash_file(&dest, &tokio_util::sync::CancellationToken::new())?;
                if recorded.as_deref() != Some(now.as_str()) {
                    return Err(CuratorError::Safety("the copy was changed after it was made; leaving it in place".into()));
                }
                if !original.exists() {
                    return Err(CuratorError::Safety("the original is missing, so the copy is the only one; leaving it in place".into()));
                }
                let jid = journal::begin(&conn, &undo_batch_id, "trash-copy", None, &dest.to_string_lossy(), None, &json!({ "hash": now }))?;
                move_to_trash(&dest)?;
                if let Some(id) = conn.query_row("SELECT id FROM media WHERE current_path = ?1", [dest.to_string_lossy()], |r| r.get::<_, i64>(0)).optional()? {
                    conn.execute("DELETE FROM media_fts WHERE rowid = ?1", [id])?;
                    conn.execute("DELETE FROM media WHERE id = ?1", [id])?;
                }
                journal::complete(&conn, jid, &json!({ "movedToTrash": true }))?;
                Ok(())
            }
            _ => Err(CuratorError::invalid("this operation type cannot be undone")),
        })();
        match outcome {
            Ok(()) => {
                conn.execute("UPDATE file_operations SET operation_status = 'undone' WHERE id = ?1", [op_id])?;
                report.undone += 1;
            }
            Err(e) => report.skipped.push((dest.to_string_lossy().into(), e.to_string())),
        }
    }
    Ok(report)
}

/// Register the destination root of a move/copy as a source so later rescans account for it.
pub fn ensure_source(conn: &Connection, root: &Path) -> Result<i64> {
    // Reuse an existing source that already contains the root.
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM sources WHERE ?1 = path OR ?1 LIKE path || '/%' ORDER BY length(path) DESC LIMIT 1",
            [root.to_string_lossy()],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => Ok(id),
        None => sources::get_or_create(conn, root, false),
    }
}

/// Move a file to the system Trash (recoverable). `MEDIA_CURATOR_TRASH_DIR` redirects this to
/// a folder, which the automated tests use so they never touch the user's real Trash.
pub fn move_to_trash(path: &Path) -> Result<()> {
    if let Some(dir) = std::env::var_os("MEDIA_CURATOR_TRASH_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir)?;
        let target = crate::filesystem::operations::unique_destination(&dir, &path.file_name().unwrap_or_default().to_string_lossy());
        std::fs::rename(path, target)?;
        return Ok(());
    }
    trash::delete(path).map_err(|e| CuratorError::ToolFailed(format!("could not move to Trash: {e}")))
}
