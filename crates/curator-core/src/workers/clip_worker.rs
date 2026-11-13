//! Background clip export: FFmpeg runs, verification, cataloguing of the new clips.

use super::job_queue::JobContext;
use crate::core::errors::{CuratorError, Result};
use crate::filesystem::transactions;
use crate::video::exporting::{self, ExportPlan, ExportReport};
use std::path::Path;
use std::sync::Arc;

pub async fn export(ctx: Arc<JobContext>, plan: ExportPlan, batch_id: String) -> Result<ExportReport> {
    let c2 = ctx.clone();
    let p2 = plan.clone();
    let report = tokio::task::spawn_blocking(move || {
        exporting::execute(&c2.app.db, &c2.app.tools, &p2, &batch_id, &c2.token, |f, label| {
            c2.progress_msg("Exporting clips", (f * 1000.0) as u64, 1000, (!label.is_empty()).then(|| label.to_string()));
        })
    })
    .await
    .map_err(|e| CuratorError::invalid(e.to_string()))??;
    if !report.exported.is_empty() {
        // Catalog the new clips, then link them to their segments with the calculated capture time.
        let source_id = {
            let c = ctx.app.db.conn()?;
            transactions::ensure_source(&c, Path::new(&plan.request.destination))?
        };
        super::index_worker::run(ctx.clone(), source_id).await?;
        let c = ctx.app.db.conn()?;
        exporting::link_exported(&c, &report.exported)?;
        ctx.library_changed("clips", true);
    }
    Ok(report)
}
