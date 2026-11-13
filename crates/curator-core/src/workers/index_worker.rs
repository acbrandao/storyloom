//! The indexing pipeline for one source folder:
//!   1. Discover files (read-only walk) and register/refresh catalog records in batches.
//!   2. Read metadata with bounded parallelism (single writer, batched transactions).
//!   3. Generate thumbnails with bounded parallelism (visible items are also generated on demand).
//!   4. Hash same-size files in the background to find exact duplicates.
//! Every phase is cancellable; per-file failures are recorded and never stop the job.

use super::job_queue::JobContext;
use crate::core::errors::{CuratorError, Result};
use crate::database::connection::in_transaction;
use crate::database::models::MediaType;
use crate::database::now_ms;
use crate::database::repositories::{jobs, media as media_repo, sources};
use crate::media::metadata::{self, ExtractedMetadata};
use crate::media::scanner::{self, ScannedFile};
use crate::media::thumbnails::{self, ThumbOutcome};
use crate::media::{fingerprints, media_types};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSummary {
    pub found: u64,
    pub new: u64,
    pub changed: u64,
    pub unchanged: u64,
    pub missing: u64,
    pub unsupported: u64,
    pub inaccessible: u64,
    pub empty: u64,
    pub metadata_read: u64,
    pub metadata_errors: u64,
    pub thumbnails: u64,
    pub thumbnail_errors: u64,
    pub hashed: u64,
}

pub async fn run(ctx: Arc<JobContext>, source_id: i64) -> Result<IndexSummary> {
    let root = {
        let c = ctx.app.db.conn()?;
        PathBuf::from(sources::get(&c, source_id)?.path)
    };
    if !root.is_dir() {
        return Err(CuratorError::NotFound(format!(
            "The folder {} is not available. If it is on an external drive, connect the drive and try again.",
            root.display()
        )));
    }

    // ---- Phase 1: discovery -------------------------------------------------------------
    let scan_time = now_ms();
    let ctx2 = ctx.clone();
    let root2 = root.clone();
    let mut summary = tokio::task::spawn_blocking(move || discover(&ctx2, source_id, &root2, scan_time))
        .await
        .map_err(|e| CuratorError::invalid(e.to_string()))??;
    if ctx.cancelled() {
        return Err(CuratorError::Cancelled);
    }
    {
        let c = ctx.app.db.conn()?;
        summary.missing = media_repo::mark_missing_after_scan(&c, source_id, scan_time)? as u64;
        sources::set_scanned(&c, source_id, scan_time)?;
    }
    ctx.library_changed("scan", true);

    // ---- Phase 2: metadata ----------------------------------------------------------------
    let (ok, err) = read_metadata(&ctx, Some(source_id)).await?;
    summary.metadata_read = ok;
    summary.metadata_errors = err;
    ctx.library_changed("metadata", true);

    // ---- Phase 3: thumbnails ----------------------------------------------------------------
    let (ok, err) = make_thumbnails(&ctx, Some(source_id)).await?;
    summary.thumbnails = ok;
    summary.thumbnail_errors = err;
    ctx.library_changed("thumbnails", true);

    // ---- Phase 4: duplicate hashing ---------------------------------------------------------
    summary.hashed = hash_duplicates(&ctx).await?;
    Ok(summary)
}

fn discover(ctx: &JobContext, source_id: i64, root: &Path, scan_time: i64) -> Result<IndexSummary> {
    let mut summary = IndexSummary::default();
    let mut batch: Vec<ScannedFile> = Vec::with_capacity(500);
    let mut issues: Vec<(String, &'static str, Option<String>)> = vec![];
    let mut conn = ctx.app.db.conn()?;
    let flush = |batch: &mut Vec<ScannedFile>, summary: &mut IndexSummary, conn: &mut r2d2_sqlite::rusqlite::Connection| -> Result<()> {
        in_transaction(conn, |tx| {
            for f in batch.iter() {
                match media_repo::upsert_scanned(tx, source_id, f, scan_time)? {
                    media_repo::UpsertOutcome::New(_) => summary.new += 1,
                    media_repo::UpsertOutcome::Changed(id) => {
                        summary.changed += 1;
                        let _ = std::fs::remove_file(ctx.app.cfg.thumbnail_path(id));
                        let _ = std::fs::remove_file(ctx.app.cfg.preview_path(id));
                    }
                    media_repo::UpsertOutcome::Unchanged(_) => summary.unchanged += 1,
                }
            }
            Ok(())
        })?;
        batch.clear();
        Ok(())
    };
    let mut err: Option<CuratorError> = None;
    let counts = scanner::scan_directory(
        root,
        &ctx.token,
        |f| {
            if err.is_some() {
                return;
            }
            batch.push(f);
            if batch.len() >= 500 {
                if let Err(e) = flush(&mut batch, &mut summary, &mut conn) {
                    err = Some(e);
                }
                let found = summary.new + summary.changed + summary.unchanged;
                ctx.progress_msg("Finding media", found, 0, Some(format!("{found} files found")));
                ctx.library_changed("scan", false);
            }
        },
        |path, kind, detail| issues.push((path.to_string(), kind.as_str(), detail)),
    );
    if let Some(e) = err {
        return Err(e);
    }
    flush(&mut batch, &mut summary, &mut conn)?;
    for (p, k, d) in issues.iter().take(5000) {
        jobs::add_issue(&conn, ctx.id, p, k, d.as_deref())?;
    }
    summary.found = counts.media_found;
    summary.unsupported = counts.unsupported;
    summary.inaccessible = counts.inaccessible;
    summary.empty = counts.empty;
    ctx.progress_msg("Finding media", summary.found, summary.found, Some(format!("{} files found", summary.found)));
    Ok(summary)
}

type MetaResult = Vec<(i64, std::result::Result<ExtractedMetadata, String>)>;

/// Read metadata for all pending items (optionally only one source). Returns (ok, errors).
pub async fn read_metadata(ctx: &Arc<JobContext>, source_id: Option<i64>) -> Result<(u64, u64)> {
    let pending = {
        let c = ctx.app.db.conn()?;
        media_repo::pending_metadata(&c, source_id)?
    };
    let total = pending.len() as u64;
    if total == 0 {
        return Ok((0, 0));
    }
    let tools = ctx.app.tools.clone();
    // Work units: ExifTool reads photos in batches; everything else is one file per unit.
    let mut units: Vec<Vec<(i64, PathBuf, MediaType)>> = vec![];
    let mut photo_batch = vec![];
    for p in pending {
        if p.media_type == MediaType::Photo && tools.exiftool.is_some() {
            photo_batch.push((p.id, PathBuf::from(p.path), p.media_type));
            if photo_batch.len() == 40 {
                units.push(std::mem::take(&mut photo_batch));
            }
        } else {
            units.push(vec![(p.id, PathBuf::from(p.path), p.media_type)]);
        }
    }
    if !photo_batch.is_empty() {
        units.push(photo_batch);
    }

    let mut set: JoinSet<MetaResult> = JoinSet::new();
    let mut units = units.into_iter();
    let mut buffer: MetaResult = vec![];
    let (mut done, mut ok, mut errors) = (0u64, 0u64, 0u64);
    let mut last_flush = Instant::now();
    ctx.progress("Reading details", 0, total);
    loop {
        while !ctx.cancelled() && set.len() < ctx.app.cfg.worker_concurrency {
            let Some(unit) = units.next() else { break };
            let permit = ctx.app.cpu_slots.clone().acquire_owned().await.map_err(|e| CuratorError::invalid(e.to_string()))?;
            let tools = tools.clone();
            set.spawn_blocking(move || {
                let _permit = permit;
                extract_unit(&unit, &tools)
            });
        }
        let Some(res) = set.join_next().await else { break };
        match res {
            Ok(r) => buffer.extend(r),
            Err(e) => tracing::error!(error = %e, "metadata task panicked"),
        }
        if buffer.len() >= 100 || last_flush.elapsed() > Duration::from_millis(500) || set.is_empty() {
            let n = buffer.len() as u64;
            let (o, e) = write_metadata(ctx, std::mem::take(&mut buffer))?;
            ok += o;
            errors += e;
            done += n;
            last_flush = Instant::now();
            ctx.progress("Reading details", done, total);
            ctx.library_changed("metadata", false);
        }
    }
    if !buffer.is_empty() {
        let (o, e) = write_metadata(ctx, buffer)?;
        ok += o;
        errors += e;
    }
    if ctx.cancelled() {
        return Err(CuratorError::Cancelled);
    }
    Ok((ok, errors))
}

fn extract_unit(unit: &[(i64, PathBuf, MediaType)], tools: &crate::tools::Tools) -> MetaResult {
    if unit.len() > 1 || (unit[0].2 == MediaType::Photo && tools.exiftool.is_some()) {
        let paths: Vec<&Path> = unit.iter().map(|(_, p, _)| p.as_path()).collect();
        if let Some(et) = &tools.exiftool {
            if let Ok(results) = metadata::extract_photos_exiftool(et, &paths) {
                return unit
                    .iter()
                    .zip(results)
                    .map(|((id, p, t), r)| {
                        // Fall back to the built-in reader if ExifTool failed for this file.
                        let r = r.or_else(|_| metadata::extract(p, *t, tools));
                        (*id, r.map_err(|e| e.to_string()))
                    })
                    .collect();
            }
        }
    }
    unit.iter().map(|(id, p, t)| (*id, metadata::extract(p, *t, tools).map_err(|e| e.to_string()))).collect()
}

fn write_metadata(ctx: &JobContext, results: MetaResult) -> Result<(u64, u64)> {
    let mut conn = ctx.app.db.conn()?;
    in_transaction(&mut conn, |tx| {
        let (mut ok, mut err) = (0, 0);
        for (id, r) in &results {
            match r {
                Ok(m) => {
                    media_repo::apply_metadata(tx, *id, m)?;
                    ok += 1;
                }
                Err(e) => {
                    media_repo::mark_metadata_error(tx, *id, e)?;
                    err += 1;
                }
            }
        }
        Ok((ok, err))
    })
}

/// Generate thumbnails for all pending items. Returns (created, errors).
pub async fn make_thumbnails(ctx: &Arc<JobContext>, source_id: Option<i64>) -> Result<(u64, u64)> {
    let pending = {
        let c = ctx.app.db.conn()?;
        media_repo::pending_thumbnails(&c, source_id)?
    };
    let total = pending.len() as u64;
    if total == 0 {
        return Ok((0, 0));
    }
    let mut set: JoinSet<(i64, &'static str)> = JoinSet::new();
    let mut items = pending.into_iter();
    let mut statuses: Vec<(i64, &'static str)> = vec![];
    let (mut done, mut ok, mut errors) = (0u64, 0u64, 0u64);
    ctx.progress("Creating thumbnails", 0, total);
    loop {
        while !ctx.cancelled() && set.len() < ctx.app.cfg.worker_concurrency {
            let Some(item) = items.next() else { break };
            let permit = ctx.app.cpu_slots.clone().acquire_owned().await.map_err(|e| CuratorError::invalid(e.to_string()))?;
            let app = ctx.app.clone();
            set.spawn_blocking(move || {
                let _permit = permit;
                // Possibly already generated on demand by the UI.
                if app.cfg.thumbnail_path(item.id).exists() {
                    return (item.id, "done");
                }
                let status = match app.db.conn().map_err(CuratorError::from).and_then(|c| media_repo::location(&c, item.id)) {
                    Ok(loc) => match thumbnails::generate_thumbnail(&loc, &app.tools, &app.cfg) {
                        Ok(ThumbOutcome::Created) => "done",
                        Ok(ThumbOutcome::Unsupported) => "unsupported",
                        Err(e) => {
                            tracing::warn!(id = item.id, error = %e, "thumbnail failed");
                            "error"
                        }
                    },
                    Err(_) => "error",
                };
                (item.id, status)
            });
        }
        let Some(res) = set.join_next().await else { break };
        if let Ok((id, status)) = res {
            match status {
                "done" | "unsupported" => ok += 1,
                _ => errors += 1,
            }
            statuses.push((id, status));
        }
        done += 1;
        if statuses.len() >= 50 || set.is_empty() {
            flush_thumb_statuses(ctx, &mut statuses)?;
            ctx.progress("Creating thumbnails", done, total);
            ctx.library_changed("thumbnails", false);
        }
    }
    flush_thumb_statuses(ctx, &mut statuses)?;
    if ctx.cancelled() {
        return Err(CuratorError::Cancelled);
    }
    Ok((ok, errors))
}

fn flush_thumb_statuses(ctx: &JobContext, statuses: &mut Vec<(i64, &'static str)>) -> Result<()> {
    if statuses.is_empty() {
        return Ok(());
    }
    let mut conn = ctx.app.db.conn()?;
    in_transaction(&mut conn, |tx| {
        for (id, s) in statuses.iter() {
            media_repo::set_thumbnail_status(tx, *id, s)?;
        }
        Ok(())
    })?;
    statuses.clear();
    Ok(())
}

/// Hash files that share a size with another file (only those can be exact duplicates).
pub async fn hash_duplicates(ctx: &Arc<JobContext>) -> Result<u64> {
    let candidates = {
        let c = ctx.app.db.conn()?;
        media_repo::hash_candidates(&c)?
    };
    let total = candidates.len() as u64;
    if total == 0 {
        return Ok(0);
    }
    let total_bytes: i64 = candidates.iter().map(|c| c.2).sum();
    let mut done_bytes = 0i64;
    let mut done = 0u64;
    let mut set: JoinSet<(i64, i64, Option<String>)> = JoinSet::new();
    let mut it = candidates.into_iter();
    loop {
        // Hashing is disk-bound: two at a time keeps the UI and other jobs responsive.
        while !ctx.cancelled() && set.len() < 2 {
            let Some((id, path, size)) = it.next() else { break };
            let token = ctx.token.clone();
            set.spawn_blocking(move || (id, size, fingerprints::hash_file(Path::new(&path), &token).ok()));
        }
        let Some(res) = set.join_next().await else { break };
        if let Ok((id, size, Some(hash))) = res {
            let c = ctx.app.db.conn()?;
            media_repo::set_hash(&c, id, &hash)?;
            done_bytes += size;
        }
        done += 1;
        let pct = if total_bytes > 0 { (done_bytes as f64 / total_bytes as f64 * 1000.0) as u64 } else { 0 };
        ctx.progress_msg("Checking for duplicates", pct, 1000, Some(format!("{done} of {total} files")));
    }
    if ctx.cancelled() {
        return Err(CuratorError::Cancelled);
    }
    Ok(done)
}

pub fn summary_json(s: &IndexSummary) -> serde_json::Value {
    json!(s)
}

/// Re-queue metadata and thumbnails for specific items (e.g. after installing ExifTool).
pub fn requeue(conn: &r2d2_sqlite::rusqlite::Connection, ids: &[i64]) -> Result<()> {
    for id in ids {
        conn.execute("UPDATE media SET metadata_status = 'pending', thumbnail_status = 'pending' WHERE id = ?1", [id])?;
    }
    Ok(())
}

pub fn is_webview_playable_video(ext: &str) -> bool {
    matches!(ext, "mp4" | "mov" | "m4v" | "webm" | "insv" | "lrv" | "3gp")
}

pub fn is_webview_image(ext: &str) -> bool {
    media_types::WEBVIEW_IMAGE_EXTENSIONS.contains(&ext)
}
