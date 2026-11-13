//! Video segments (clips): drafts marked on a source video, and exported clips with provenance.

use crate::core::errors::{CuratorError, Result};
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{self, params, Connection, OptionalExtension, Row};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: i64,
    pub source_media_id: i64,
    pub output_media_id: Option<i64>,
    pub name: Option<String>,
    pub in_point: f64,
    pub out_point: f64,
    pub actual_in_point: Option<f64>,
    pub actual_out_point: Option<f64>,
    pub calculated_capture_datetime: Option<String>,
    pub calculated_capture_timezone: Option<String>,
    pub exported_path: Option<String>,
    pub processing_method: Option<String>,
    pub preset: Option<String>,
    pub exported_at: Option<i64>,
    pub verification: Option<serde_json::Value>,
    pub created_at: i64,
}

const COLS: &str = "id, source_media_id, output_media_id, name, in_point, out_point, actual_in_point, actual_out_point,
    calculated_capture_datetime, calculated_capture_timezone, exported_path, processing_method, preset, exported_at, verification_json, created_at";

fn row(r: &Row) -> rusqlite::Result<Segment> {
    Ok(Segment {
        id: r.get(0)?,
        source_media_id: r.get(1)?,
        output_media_id: r.get(2)?,
        name: r.get(3)?,
        in_point: r.get(4)?,
        out_point: r.get(5)?,
        actual_in_point: r.get(6)?,
        actual_out_point: r.get(7)?,
        calculated_capture_datetime: r.get(8)?,
        calculated_capture_timezone: r.get(9)?,
        exported_path: r.get(10)?,
        processing_method: r.get(11)?,
        preset: r.get(12)?,
        exported_at: r.get(13)?,
        verification: r.get::<_, Option<String>>(14)?.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: r.get(15)?,
    })
}

pub fn for_source(conn: &Connection, source_media_id: i64) -> Result<Vec<Segment>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM video_segments WHERE source_media_id = ?1 ORDER BY in_point, id"))?;
    let v = stmt.query_map([source_media_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get(conn: &Connection, id: i64) -> Result<Segment> {
    conn.query_row(&format!("SELECT {COLS} FROM video_segments WHERE id = ?1"), [id], row)
        .optional()?
        .ok_or_else(|| CuratorError::NotFound(format!("clip {id}")))
}

/// The segment a clip file was exported from (if `media_id` is an exported clip).
pub fn origin_of(conn: &Connection, media_id: i64) -> Result<Option<Segment>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM video_segments WHERE output_media_id = ?1 ORDER BY id DESC LIMIT 1"), [media_id], row).optional()?)
}

fn validate(conn: &Connection, source_media_id: i64, in_point: f64, out_point: f64) -> Result<()> {
    let (media_type, duration): (String, Option<f64>) =
        conn.query_row("SELECT media_type, duration FROM media WHERE id = ?1", [source_media_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if media_type != "video" {
        return Err(CuratorError::invalid("clips can only be made from videos"));
    }
    if !(in_point.is_finite() && out_point.is_finite()) || in_point < 0.0 {
        return Err(CuratorError::invalid("the in-point must be at or after the start of the video"));
    }
    if out_point - in_point < 0.1 {
        return Err(CuratorError::invalid("a clip must be at least 0.1 seconds long (out-point after in-point)"));
    }
    if let Some(d) = duration {
        if out_point > d + 0.05 {
            return Err(CuratorError::invalid(format!("the out-point is past the end of the video ({d:.2} s)")));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, source_media_id: i64, in_point: f64, out_point: f64, name: Option<&str>) -> Result<i64> {
    validate(conn, source_media_id, in_point, out_point)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO video_segments (source_media_id, name, in_point, out_point, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![source_media_id, name.map(str::trim).filter(|n| !n.is_empty()), in_point, out_point, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Edit a clip that has not been exported yet. Exported clips are a permanent record.
pub fn update(conn: &Connection, id: i64, in_point: f64, out_point: f64, name: Option<&str>) -> Result<()> {
    let s = get(conn, id)?;
    if s.exported_path.is_some() {
        // Renaming the label is harmless; the range of an exported clip is history.
        if (s.in_point - in_point).abs() > 1e-6 || (s.out_point - out_point).abs() > 1e-6 {
            return Err(CuratorError::invalid("this clip was already exported; add a new clip to change the range"));
        }
    } else {
        validate(conn, s.source_media_id, in_point, out_point)?;
    }
    conn.execute(
        "UPDATE video_segments SET in_point = ?2, out_point = ?3, name = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, in_point, out_point, name.map(str::trim).filter(|n| !n.is_empty()), now_ms()],
    )?;
    Ok(())
}

/// Delete a draft clip. Exported clips keep their provenance record.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let s = get(conn, id)?;
    if s.exported_path.is_some() {
        return Err(CuratorError::invalid("exported clips keep their record so the link to the original is never lost"));
    }
    conn.execute("DELETE FROM video_segments WHERE id = ?1", [id])?;
    Ok(())
}

/// Record a finished export. If the segment was already exported, a new row is created so
/// every exported version keeps its own provenance.
#[allow(clippy::too_many_arguments)]
pub fn record_export(
    conn: &Connection,
    segment_id: i64,
    exported_path: &str,
    method: &str,
    preset: &str,
    actual_in: f64,
    actual_out: f64,
    capture_local: Option<&str>,
    capture_offset: Option<&str>,
    verification: &serde_json::Value,
) -> Result<i64> {
    let s = get(conn, segment_id)?;
    let target = if s.exported_path.is_some() {
        conn.execute(
            "INSERT INTO video_segments (source_media_id, name, in_point, out_point, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![s.source_media_id, s.name, s.in_point, s.out_point, now_ms()],
        )?;
        conn.last_insert_rowid()
    } else {
        segment_id
    };
    conn.execute(
        "UPDATE video_segments SET exported_path = ?2, processing_method = ?3, preset = ?4, actual_in_point = ?5, actual_out_point = ?6,
            calculated_capture_datetime = ?7, calculated_capture_timezone = ?8, verification_json = ?9, exported_at = ?10, updated_at = ?10
         WHERE id = ?1",
        params![target, exported_path, method, preset, actual_in, actual_out, capture_local, capture_offset, verification.to_string(), now_ms()],
    )?;
    Ok(target)
}

pub fn set_output_media(conn: &Connection, segment_id: i64, media_id: i64) -> Result<()> {
    conn.execute("UPDATE video_segments SET output_media_id = ?2 WHERE id = ?1", params![segment_id, media_id])?;
    Ok(())
}
