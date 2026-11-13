//! Executes an approved ingest plan: verified copies into the library, then indexing.

use super::job_queue::JobContext;
use crate::core::errors::{CuratorError, Result};
use crate::database::repositories::{operations, sources};
use crate::media::importer::{self, IngestPlan};
use r2d2_sqlite::rusqlite::params;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestSummary {
    pub copied: u64,
    pub skipped_identical: u64,
    pub failed: u64,
    pub bytes_copied: u64,
    pub timestamp_warnings: u64,
    pub index: Option<super::index_worker::IndexSummary>,
}

pub async fn run(ctx: Arc<JobContext>, plan: IngestPlan) -> Result<IngestSummary> {
    let mut summary = IngestSummary::default();
    let batch_id = format!("ingest-{}", ctx.id);
    let total_bytes: i64 = plan.items.iter().filter(|i| !i.already_present).map(|i| i.size).sum::<i64>().max(1);
    let mut done_bytes = 0i64;
    let mut copied_pairs: Vec<(String, String)> = vec![];
    let n = plan.items.len();
    for (idx, item) in plan.items.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        if item.already_present {
            summary.skipped_identical += 1;
            copied_pairs.push((item.destination.clone(), item.source.clone()));
            continue;
        }
        let previous = json!({
            "sourceTimestamps": crate::filesystem::timestamps::read(Path::new(&item.source)).ok(),
            "size": item.size,
        });
        let op_id = {
            let c = ctx.app.db.conn()?;
            operations::begin(&c, &batch_id, "copy", None, &item.source, Some(&item.destination), &previous)?
        };
        let item2 = item.clone();
        let token = ctx.token.clone();
        let res = tokio::task::spawn_blocking(move || importer::copy_item(&item2, &token)).await.map_err(|e| CuratorError::invalid(e.to_string()))?;
        let c = ctx.app.db.conn()?;
        match res {
            Ok(report) => {
                if !report.timestamps.fully_preserved() {
                    summary.timestamp_warnings += 1;
                }
                operations::complete(&c, op_id, &serde_json::to_value(&report)?)?;
                summary.copied += 1;
                summary.bytes_copied += report.bytes;
                copied_pairs.push((item.destination.clone(), item.source.clone()));
            }
            Err(CuratorError::Cancelled) => {
                operations::fail(&c, op_id, "cancelled")?;
                break;
            }
            Err(e) => {
                operations::fail(&c, op_id, &e.to_string())?;
                summary.failed += 1;
            }
        }
        done_bytes += item.size;
        ctx.progress_msg("Copying", (done_bytes as f64 / total_bytes as f64 * 1000.0) as u64, 1000, Some(format!("{} of {n} files", idx + 1)));
    }
    if ctx.cancelled() {
        return Err(CuratorError::Cancelled);
    }
    // Register the library folder as a source and index it.
    let source_id = {
        let c = ctx.app.db.conn()?;
        sources::get_or_create(&c, Path::new(&plan.library_dir), true)?
    };
    let index = super::index_worker::run(ctx.clone(), source_id).await?;
    // Provenance: remember where each library file originally came from.
    {
        let c = ctx.app.db.conn()?;
        for (dest, src) in &copied_pairs {
            c.execute(
                "UPDATE media SET original_path = ?2 WHERE current_path = ?1 AND original_path = current_path",
                params![dest, src],
            )?;
        }
    }
    summary.index = Some(index);
    Ok(summary)
}
