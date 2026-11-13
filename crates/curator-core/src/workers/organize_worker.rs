//! Background execution of rename / move / copy batches and undo.

use super::job_queue::JobContext;
use crate::core::errors::{CuratorError, Result};
use crate::database::repositories::{media as media_repo, operations as journal};
use crate::filesystem::organization::{OrganizeMode, OrganizePlan};
use crate::filesystem::renaming::{PlanStatus, RenamePlan};
use crate::filesystem::transactions::{self, ExecReport};
use r2d2_sqlite::rusqlite::{params, OptionalExtension};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

pub async fn rename(ctx: Arc<JobContext>, plan: RenamePlan, batch_id: String) -> Result<ExecReport> {
    let c2 = ctx.clone();
    tokio::task::spawn_blocking(move || {
        transactions::execute_moves(&c2.app.db, &plan.items, "rename", &batch_id, |d, t| {
            c2.progress("Renaming", d as u64, t as u64);
            c2.library_changed("rename", false);
        })
    })
    .await
    .map_err(|e| CuratorError::invalid(e.to_string()))?
}

pub async fn organize(ctx: Arc<JobContext>, plan: OrganizePlan, batch_id: String) -> Result<ExecReport> {
    let root = Path::new(&plan.destination).to_path_buf();
    std::fs::create_dir_all(&root)?;
    let source_id = {
        let c = ctx.app.db.conn()?;
        transactions::ensure_source(&c, &root)?
    };
    match plan.mode {
        OrganizeMode::Move => {
            let c2 = ctx.clone();
            let b = batch_id.clone();
            tokio::task::spawn_blocking(move || {
                transactions::execute_moves(&c2.app.db, &plan.items, "move", &b, |d, t| {
                    c2.progress("Moving", d as u64, t as u64);
                    c2.library_changed("move", false);
                })
            })
            .await
            .map_err(|e| CuratorError::invalid(e.to_string()))?
        }
        OrganizeMode::Copy => {
            let mut report = ExecReport { batch_id: batch_id.clone(), ..Default::default() };
            let ready: Vec<_> = plan.items.iter().filter(|i| i.status == PlanStatus::Ready).cloned().collect();
            report.skipped = plan.items.len() - ready.len();
            let mut copied: Vec<(i64, String)> = vec![];
            for (n, item) in ready.iter().enumerate() {
                if ctx.cancelled() {
                    break;
                }
                let op = {
                    let c = ctx.app.db.conn()?;
                    let prev = json!({ "timestamps": crate::filesystem::timestamps::read(Path::new(&item.from)).ok() });
                    journal::begin(&c, &batch_id, "copy", Some(item.media_id), &item.from, Some(&item.to), &prev)?
                };
                let (from, to, token) = (item.from.clone(), item.to.clone(), ctx.token.clone());
                let res = tokio::task::spawn_blocking(move || crate::filesystem::operations::safe_copy(Path::new(&from), Path::new(&to), &token))
                    .await
                    .map_err(|e| CuratorError::invalid(e.to_string()))?;
                let c = ctx.app.db.conn()?;
                match res {
                    Ok(r) => {
                        if !r.timestamps.fully_preserved() {
                            report.timestamp_warnings.extend(r.timestamps.warnings.iter().map(|w| format!("{}: {w}", item.to)));
                        }
                        journal::complete(&c, op, &serde_json::to_value(&r)?)?;
                        copied.push((item.media_id, item.to.clone()));
                        report.done += 1;
                    }
                    Err(e) => {
                        journal::fail(&c, op, &e.to_string())?;
                        report.failed.push((item.from.clone(), e.to_string()));
                    }
                }
                ctx.progress("Copying", n as u64 + 1, ready.len() as u64);
            }
            // Catalog the copies (incremental scan of the destination), then carry over provenance
            // and the curation work (ratings, flags, notes, tags) from each original.
            super::index_worker::run(ctx.clone(), source_id).await?;
            let mut c = ctx.app.db.conn()?;
            let tx = c.transaction()?;
            for (orig_id, dest) in &copied {
                let new_id: Option<i64> = tx.query_row("SELECT id FROM media WHERE current_path = ?1", [dest], |r| r.get(0)).optional()?;
                if let Some(new_id) = new_id {
                    let orig_path: String = tx.query_row("SELECT current_path FROM media WHERE id = ?1", [orig_id], |r| r.get(0))?;
                    tx.execute("UPDATE media SET original_path = ?2 WHERE id = ?1", params![new_id, orig_path])?;
                    media_repo::copy_curation(&tx, *orig_id, new_id)?;
                }
            }
            tx.commit()?;
            Ok(report)
        }
    }
}

pub async fn undo(ctx: Arc<JobContext>, batch_id: String) -> Result<transactions::UndoReport> {
    let c2 = ctx.clone();
    ctx.progress("Undoing", 0, 0);
    tokio::task::spawn_blocking(move || transactions::undo_batch(&c2.app.db, &batch_id))
        .await
        .map_err(|e| CuratorError::invalid(e.to_string()))?
}
