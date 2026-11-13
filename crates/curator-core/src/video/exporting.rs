//! Clip export: planning (names, keyframes, capture times) and verified execution.
//!
//! Capture time of a clip = source capture time + the clip's *actual* in-point (for lossless
//! exports that is the keyframe the clip really starts on). It is written into the clip as
//! QuickTime metadata and stored in the catalog; filesystem dates follow [`FileDatePolicy`].

use super::timeline::clip_capture_time;
use super::trimming::{self, ExportPreset, SourceInfo};
use crate::core::errors::{CuratorError, Result};
use crate::database::models::MediaType;
use crate::database::repositories::{operations as journal, segments};
use crate::database::Database;
use crate::filesystem::renaming::{self, ConflictPolicy, NameContext, PlanStatus};
use crate::filesystem::timestamps::{self, FileTimestamps};
use crate::filesystem::transactions;
use crate::media::{metadata, timefmt};
use crate::tools::Tools;
use chrono::{Local, NaiveDateTime, TimeZone};
use r2d2_sqlite::rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FileDatePolicy {
    /// Give the clip file the source file's creation and modification dates (default).
    #[default]
    SourceFile,
    /// Set the clip file's dates to the clip's capture time.
    ClipCapture,
    /// Leave the dates of the newly written file (the export time).
    ExportTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub segment_ids: Vec<i64>,
    pub destination: String,
    pub preset: ExportPreset,
    pub name_template: String,
    #[serde(default)]
    pub file_dates: FileDatePolicy,
    #[serde(default)]
    pub conflict: ConflictPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportItem {
    pub segment_id: i64,
    pub source_media_id: i64,
    pub source_path: String,
    pub source_filename: String,
    pub clip_name: Option<String>,
    pub in_point: f64,
    pub out_point: f64,
    pub actual_in: f64,
    pub actual_out: f64,
    pub capture_local: Option<String>,
    pub capture_offset: Option<String>,
    pub to: String,
    pub status: PlanStatus,
    pub notes: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlan {
    pub request: ExportRequest,
    pub items: Vec<ExportItem>,
    pub ready: usize,
    pub total_seconds: f64,
    pub method: String,
}

struct SourceRow {
    path: String,
    original_filename: String,
    current_filename: String,
    capture: Option<String>,
    offset: Option<String>,
    location: Option<String>,
    description: Option<String>,
    camera: Option<String>,
}

fn load_source(conn: &Connection, id: i64) -> Result<SourceRow> {
    conn.query_row(
        "SELECT current_path, original_filename, current_filename,
            COALESCE(user_capture_datetime, capture_datetime),
            CASE WHEN user_capture_datetime IS NOT NULL THEN user_capture_timezone ELSE capture_timezone END,
            location, description, camera_model
         FROM media WHERE id = ?1",
        [id],
        |r| {
            Ok(SourceRow {
                path: r.get(0)?,
                original_filename: r.get(1)?,
                current_filename: r.get(2)?,
                capture: r.get(3)?,
                offset: r.get(4)?,
                location: r.get(5)?,
                description: r.get(6)?,
                camera: r.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CuratorError::NotFound(format!("media {id}")))
}

/// Preview an export. Reads only (FFprobe is used to find keyframes).
pub fn plan(conn: &Connection, tools: &Tools, req: &ExportRequest, protected_dirs: &[PathBuf]) -> Result<ExportPlan> {
    let ffprobe = tools.require_ffprobe()?;
    tools.require_ffmpeg()?;
    let dest = PathBuf::from(&req.destination);
    if !dest.is_absolute() {
        return Err(CuratorError::invalid("choose a folder for the clips"));
    }
    if protected_dirs.iter().any(|p| dest.starts_with(p)) {
        return Err(CuratorError::Safety("the destination is inside Media Curator's own data folders".into()));
    }
    renaming::validate_template(&req.name_template)?;
    let mut segs = req.segment_ids.iter().map(|id| segments::get(conn, *id)).collect::<Result<Vec<_>>>()?;
    let mut sources: HashMap<i64, (SourceRow, Option<SourceInfo>)> = HashMap::new();
    for s in &segs {
        if !sources.contains_key(&s.source_media_id) {
            let row = load_source(conn, s.source_media_id)?;
            let info = if Path::new(&row.path).exists() { Some(trimming::source_info(ffprobe, Path::new(&row.path))?) } else { None };
            sources.insert(s.source_media_id, (row, info));
        }
    }
    // Chronological: by source capture time, then in-point.
    segs.sort_by(|a, b| {
        let ka = sources[&a.source_media_id].0.capture.clone();
        let kb = sources[&b.source_media_id].0.capture.clone();
        ka.cmp(&kb).then(a.source_media_id.cmp(&b.source_media_id)).then(a.in_point.total_cmp(&b.in_point))
    });
    let mut claimed: HashSet<String> = HashSet::new();
    let mut items = vec![];
    for (i, s) in segs.iter().enumerate() {
        let (row, info) = &sources[&s.source_media_id];
        let mut item = ExportItem {
            segment_id: s.id,
            source_media_id: s.source_media_id,
            source_path: row.path.clone(),
            source_filename: row.current_filename.clone(),
            clip_name: s.name.clone(),
            in_point: s.in_point,
            out_point: s.out_point,
            actual_in: s.in_point,
            actual_out: s.out_point,
            capture_local: None,
            capture_offset: None,
            to: String::new(),
            status: PlanStatus::Ready,
            notes: vec![],
            message: None,
        };
        let Some(info) = info else {
            item.status = PlanStatus::Missing;
            item.message = Some("the original video is missing or its drive is offline".into());
            items.push(item);
            continue;
        };
        item.actual_out = s.out_point.min(info.duration.max(s.out_point));
        if req.preset == ExportPreset::Lossless {
            item.actual_in = trimming::keyframe_at_or_before(ffprobe, Path::new(&row.path), s.in_point, info.start_time)?;
        }
        item.notes = trimming::preset_notes(req.preset, info, s.in_point, item.actual_in);
        match row.capture.as_deref().and_then(|c| clip_capture_time(c, row.offset.as_deref(), item.actual_in)) {
            Some(c) => {
                item.capture_local = Some(c.local);
                item.capture_offset = c.offset;
            }
            None => item.notes.push("The original has no capture date, so the clip has none either".into()),
        }
        let stem = |f: &str| Path::new(f).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        let ctx = NameContext {
            capture_local: item.capture_local.clone(),
            original_stem: stem(&row.original_filename),
            current_stem: stem(&row.current_filename),
            camera: row.camera.clone(),
            location: row.location.clone(),
            description: s.name.clone().or_else(|| row.description.clone()),
            rating: 0,
            media_type: "video".into(),
            sequence: i as u64 + 1,
        };
        let name = renaming::render(&req.name_template, &ctx)?;
        let ext = trimming::output_ext(req.preset, info);
        let mut to = dest.join(format!("{name}.{ext}"));
        let taken = |p: &Path, claimed: &HashSet<String>| claimed.contains(&p.to_string_lossy().to_lowercase()) || p.exists();
        if taken(&to, &claimed) {
            match req.conflict {
                ConflictPolicy::Skip => {
                    item.status = PlanStatus::Conflict;
                    item.message = Some(format!("“{name}.{ext}” already exists"));
                }
                ConflictPolicy::AddSuffix => {
                    let mut n = 2;
                    while taken(&to, &claimed) {
                        to = dest.join(format!("{name}-{n}.{ext}"));
                        n += 1;
                    }
                }
            }
        }
        if item.status == PlanStatus::Ready {
            claimed.insert(to.to_string_lossy().to_lowercase());
        }
        item.to = to.to_string_lossy().into();
        items.push(item);
    }
    let ready = items.iter().filter(|i| i.status == PlanStatus::Ready).count();
    let total_seconds = items.iter().filter(|i| i.status == PlanStatus::Ready).map(|i| i.actual_out - i.actual_in).sum();
    Ok(ExportPlan { request: req.clone(), items, ready, total_seconds, method: req.preset.method().into() })
}

/// The metadata timestamps to embed for a clip capture time.
pub fn clip_metadata(local: Option<&str>, offset: Option<&str>, comment: String) -> trimming::ClipMetadata {
    let parsed = local.and_then(timefmt::parse_local);
    let (creation_time, apple) = match (parsed, offset.and_then(timefmt::parse_offset)) {
        (Some(dt), Some(off)) => {
            let utc = dt - chrono::Duration::seconds(off.local_minus_utc() as i64);
            let z = off.local_minus_utc().abs();
            let compact = format!("{}{:02}{:02}", if off.local_minus_utc() < 0 { '-' } else { '+' }, z / 3600, (z % 3600) / 60);
            (Some(utc.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()), Some(format!("{}{compact}", dt.format("%Y-%m-%dT%H:%M:%S"))))
        }
        // No time zone recorded: keep the camera's convention (local wall-clock in the UTC field).
        (Some(dt), None) => (Some(dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()), Some(String::new())),
        // No capture time: clear the fields so the source's own date is not copied over.
        (None, _) => (Some(String::new()), Some(String::new())),
    };
    trimming::ClipMetadata { creation_time, apple_creationdate: apple, comment }
}

fn capture_instant_ms(local: &str, offset: Option<&str>) -> Option<i64> {
    let dt: NaiveDateTime = timefmt::parse_local(local)?;
    match offset.and_then(timefmt::parse_offset) {
        Some(off) => Some((dt - chrono::Duration::seconds(off.local_minus_utc() as i64)).and_utc().timestamp_millis()),
        None => Local.from_local_datetime(&dt).earliest().map(|d| d.timestamp_millis()),
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub batch_id: String,
    pub done: usize,
    pub skipped: usize,
    pub failed: Vec<(String, String)>,
    pub timestamp_warnings: Vec<String>,
    /// (new segment id, exported path)
    pub exported: Vec<(i64, String)>,
}

/// Export the ready items of a plan. `progress(done_fraction_of_total)`.
pub fn execute(db: &Database, tools: &Tools, plan: &ExportPlan, batch_id: &str, cancel: &CancellationToken, mut progress: impl FnMut(f64, &str)) -> Result<ExportReport> {
    let ffmpeg = tools.require_ffmpeg()?.to_path_buf();
    let ffprobe = tools.require_ffprobe()?.to_path_buf();
    let mut report = ExportReport { batch_id: batch_id.into(), ..Default::default() };
    let ready: Vec<&ExportItem> = plan.items.iter().filter(|i| i.status == PlanStatus::Ready).collect();
    report.skipped = plan.items.len() - ready.len();
    let total: f64 = ready.iter().map(|i| i.actual_out - i.actual_in).sum::<f64>().max(0.001);
    let mut done_seconds = 0.0;
    std::fs::create_dir_all(&plan.request.destination)?;
    for item in ready {
        if cancel.is_cancelled() {
            return Err(CuratorError::Cancelled);
        }
        let src = Path::new(&item.source_path);
        let dest = PathBuf::from(&item.to);
        let conn = db.conn()?;
        let source_ts = timestamps::read(src)?;
        let op = journal::begin(&conn, batch_id, "export-clip", Some(item.source_media_id), &item.source_path, Some(&item.to),
            &json!({ "sourceTimestamps": source_ts, "inPoint": item.in_point, "outPoint": item.out_point, "preset": plan.request.preset }))?;
        let label = dest.file_name().unwrap_or_default().to_string_lossy().to_string();
        let result = (|| -> Result<(i64, serde_json::Value, Vec<String>)> {
            let info = trimming::source_info(&ffprobe, src)?;
            let tmp = dest.with_file_name(format!(".{}.mcpartial-{}", label, crate::database::now_ms()));
            let tmp = tmp.with_extension(dest.extension().unwrap_or_default());
            let duration = item.actual_out - item.actual_in;
            let comment = format!("Clip of {} ({} – {}), made with Media Curator", item.source_filename, fmt_tc(item.in_point), fmt_tc(item.out_point));
            let meta = clip_metadata(item.capture_local.as_deref(), item.capture_offset.as_deref(), comment);
            let cmd = trimming::build_command(&ffmpeg, src, &tmp, plan.request.preset, &info, item.actual_in, duration, &meta);
            let run = trimming::run(cmd, duration, cancel, |f| progress((done_seconds + f * duration) / total, &label));
            if let Err(e) = run {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            // --- verify the output before it gets its final name
            let out_info = trimming::source_info(&ffprobe, &tmp).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })?;
            let tol = if plan.request.preset == ExportPreset::Lossless { 0.5 + 2.0 / info.fps } else { 0.15 + 2.0 / info.fps };
            let duration_ok = (out_info.duration - duration).abs() <= tol;
            if !duration_ok {
                let _ = std::fs::remove_file(&tmp);
                return Err(CuratorError::Safety(format!("verification failed: the clip is {:.2} s long, expected {:.2} s", out_info.duration, duration)));
            }
            let mut warnings = vec![];
            let wanted: Option<FileTimestamps> = match plan.request.file_dates {
                FileDatePolicy::SourceFile => Some(source_ts),
                FileDatePolicy::ClipCapture => item.capture_local.as_deref().and_then(|l| capture_instant_ms(l, item.capture_offset.as_deref())).map(|ms| FileTimestamps { created_ms: Some(ms), modified_ms: ms }),
                FileDatePolicy::ExportTime => None,
            };
            let ts_report = match &wanted {
                Some(w) => {
                    let r = timestamps::apply(&tmp, w)?;
                    warnings.extend(r.warnings.iter().map(|x| format!("{label}: {x}")));
                    Some(r)
                }
                None => None,
            };
            transactions::move_no_clobber(&tmp, &dest).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })?;
            let final_ts = timestamps::read(&dest)?;
            let fs_ok = wanted.map(|w| timestamps::verify(&w, &dest).unwrap_or(false));
            // Read the embedded capture time back with the same extractor used for indexing.
            let embedded = metadata::extract(&dest, MediaType::Video, tools).ok().and_then(|m| m.capture);
            let embedded_ok = match (&item.capture_local, &embedded) {
                (Some(want), Some(got)) => Some(want.get(..19) == got.local.get(..19)),
                (None, None) => Some(true),
                _ => Some(false),
            };
            if embedded_ok == Some(false) && item.capture_offset.is_some() {
                warnings.push(format!("{label}: the capture time read back from the file does not match"));
            }
            let verification = json!({
                "expectedDuration": duration, "actualDuration": out_info.duration, "durationVerified": duration_ok,
                "fileDatesPolicy": plan.request.file_dates, "fileDatesRequested": wanted, "fileDatesActual": final_ts,
                "fileDatesVerified": fs_ok, "timestampReport": ts_report,
                "captureCalculated": item.capture_local, "captureOffset": item.capture_offset,
                "captureEmbeddedReadBack": embedded.as_ref().map(|c| c.local.clone()), "captureEmbeddedVerified": embedded_ok,
                "sourceUnchanged": timestamps::read(src).map(|t| t == source_ts).unwrap_or(false),
            });
            let seg_id = segments::record_export(
                &conn, item.segment_id, &item.to, plan.request.preset.method(), plan.request.preset.as_str(),
                item.actual_in, item.actual_out, item.capture_local.as_deref(), item.capture_offset.as_deref(), &verification,
            )?;
            Ok((seg_id, verification, warnings))
        })();
        match result {
            Ok((seg_id, verification, warnings)) => {
                journal::complete(&conn, op, &verification)?;
                report.timestamp_warnings.extend(warnings);
                report.exported.push((seg_id, item.to.clone()));
                report.done += 1;
            }
            Err(CuratorError::Cancelled) => {
                journal::fail(&conn, op, "cancelled")?;
                return Err(CuratorError::Cancelled);
            }
            Err(e) => {
                journal::fail(&conn, op, &e.to_string())?;
                report.failed.push((item.to.clone(), e.to_string()));
            }
        }
        done_seconds += item.actual_out - item.actual_in;
        progress(done_seconds / total, "");
    }
    Ok(report)
}

/// After the clips were indexed: link catalog records, set the calculated capture time
/// (authoritative, marked `derived_clip`) and carry over tags / location / description.
pub fn link_exported(conn: &Connection, exported: &[(i64, String)]) -> Result<usize> {
    let mut n = 0;
    for (seg_id, path) in exported {
        let Some(media_id) = conn.query_row("SELECT id FROM media WHERE current_path = ?1", [path], |r| r.get::<_, i64>(0)).optional()? else { continue };
        let s = segments::get(conn, *seg_id)?;
        segments::set_output_media(conn, *seg_id, media_id)?;
        conn.execute(
            "UPDATE media SET capture_datetime = ?2, capture_timezone = ?3,
                capture_time_source = CASE WHEN ?2 IS NULL THEN NULL ELSE 'derived_clip' END,
                sort_datetime = COALESCE(user_capture_datetime, ?2, sort_datetime),
                location = COALESCE(location, (SELECT location FROM media WHERE id = ?4)),
                description = COALESCE(description, ?5, (SELECT description FROM media WHERE id = ?4))
             WHERE id = ?1",
            r2d2_sqlite::rusqlite::params![media_id, s.calculated_capture_datetime, s.calculated_capture_timezone, s.source_media_id, s.name],
        )?;
        conn.execute("INSERT OR IGNORE INTO media_tags (media_id, tag_id) SELECT ?2, tag_id FROM media_tags WHERE media_id = ?1", [s.source_media_id, media_id])?;
        crate::database::repositories::media::refresh_fts(conn, media_id)?;
        n += 1;
    }
    Ok(n)
}

pub fn fmt_tc(t: f64) -> String {
    let ms = (t * 1000.0).round() as i64;
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_with_offset_is_exact_utc_and_local() {
        let m = clip_metadata(Some("2026-07-14T15:37:15"), Some("-04:00"), String::new());
        assert_eq!(m.creation_time.as_deref(), Some("2026-07-14T19:37:15.000000Z"));
        assert_eq!(m.apple_creationdate.as_deref(), Some("2026-07-14T15:37:15-0400"));
    }

    #[test]
    fn metadata_without_offset_keeps_wall_clock() {
        let m = clip_metadata(Some("2026-07-14T15:37:15.500"), None, String::new());
        assert_eq!(m.creation_time.as_deref(), Some("2026-07-14T15:37:15.500000Z"));
        assert_eq!(m.apple_creationdate.as_deref(), Some(""));
    }

    #[test]
    fn metadata_without_capture_clears_fields() {
        let m = clip_metadata(None, None, String::new());
        assert_eq!(m.creation_time.as_deref(), Some(""));
    }

    #[test]
    fn timecodes() {
        assert_eq!(fmt_tc(270.5), "00:04:30.500");
        assert_eq!(fmt_tc(3725.042), "01:02:05.042");
    }
}
