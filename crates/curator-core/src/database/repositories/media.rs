//! Media catalog repository: scan upserts, metadata application, querying and curation.

use crate::core::errors::{CuratorError, Result};
use crate::database::models::*;
use crate::database::now_ms;
use crate::media::metadata::ExtractedMetadata;
use crate::media::scanner::ScannedFile;
use crate::media::timefmt;
use r2d2_sqlite::rusqlite::{self, params, params_from_iter, Connection, OptionalExtension, Row, ToSql};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    New(i64),
    Changed(i64),
    Unchanged(i64),
}

/// Register a file found by the scanner. Never creates a second record for a path
/// already in the catalog; detects changed files by size + modification time.
pub fn upsert_scanned(conn: &Connection, source_id: i64, f: &ScannedFile, scan_time: i64) -> Result<UpsertOutcome> {
    let existing: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT id, file_size, filesystem_modification_time FROM media WHERE current_path = ?1",
            [&f.path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let provisional_sort = timefmt::ms_to_local_naive_string(f.birth_ms.unwrap_or(f.mtime_ms));
    match existing {
        Some((id, size, mtime)) if size == f.size && mtime == f.mtime_ms => {
            conn.execute(
                "UPDATE media SET last_scanned_at = ?2, is_missing = 0, source_id = COALESCE(source_id, ?3) WHERE id = ?1",
                params![id, scan_time, source_id],
            )?;
            Ok(UpsertOutcome::Unchanged(id))
        }
        Some((id, _, _)) => {
            conn.execute(
                "UPDATE media SET file_size = ?2, filesystem_modification_time = ?3, filesystem_creation_time = ?4,
                    last_scanned_at = ?5, is_missing = 0, metadata_status = 'pending', thumbnail_status = 'pending',
                    file_hash = NULL, updated_at = ?5,
                    sort_datetime = COALESCE(user_capture_datetime, capture_datetime, ?6)
                 WHERE id = ?1",
                params![id, f.size, f.mtime_ms, f.birth_ms, scan_time, provisional_sort],
            )?;
            Ok(UpsertOutcome::Changed(id))
        }
        None => {
            conn.execute(
                "INSERT INTO media (source_id, original_filename, current_filename, original_path, current_path, folder_path,
                    extension, media_type, mime_type, file_size, filesystem_creation_time, filesystem_modification_time,
                    sort_datetime, added_at, last_scanned_at, updated_at)
                 VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?12)",
                params![
                    source_id,
                    f.filename,
                    f.path,
                    f.folder,
                    f.extension,
                    f.media_type.as_str(),
                    f.mime_type,
                    f.size,
                    f.birth_ms,
                    f.mtime_ms,
                    provisional_sort,
                    scan_time
                ],
            )?;
            let id = conn.last_insert_rowid();
            refresh_fts(conn, id)?;
            Ok(UpsertOutcome::New(id))
        }
    }
}

/// Flag records under a source that were not seen during the scan that started at `scan_time`.
/// Records are never deleted automatically.
pub fn mark_missing_after_scan(conn: &Connection, source_id: i64, scan_time: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE media SET is_missing = 1 WHERE source_id = ?1 AND last_scanned_at < ?2 AND is_missing = 0",
        params![source_id, scan_time],
    )?)
}

pub struct PendingItem {
    pub id: i64,
    pub path: String,
    pub media_type: MediaType,
    pub extension: String,
}

pub fn pending_metadata(conn: &Connection, source_id: Option<i64>) -> Result<Vec<PendingItem>> {
    pending_where(conn, "metadata_status = 'pending' AND is_missing = 0", source_id)
}

pub fn pending_thumbnails(conn: &Connection, source_id: Option<i64>) -> Result<Vec<PendingItem>> {
    pending_where(conn, "thumbnail_status = 'pending' AND is_missing = 0", source_id)
}

fn pending_where(conn: &Connection, cond: &str, source_id: Option<i64>) -> Result<Vec<PendingItem>> {
    let sql = format!(
        "SELECT id, current_path, media_type, extension FROM media WHERE {cond} {} ORDER BY sort_datetime DESC",
        if source_id.is_some() { "AND source_id = ?1" } else { "" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let map = |r: &Row| {
        Ok(PendingItem {
            id: r.get(0)?,
            path: r.get(1)?,
            media_type: MediaType::parse(&r.get::<_, String>(2)?).unwrap_or(MediaType::Photo),
            extension: r.get(3)?,
        })
    };
    let rows = match source_id {
        Some(s) => stmt.query_map([s], map)?.collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(rows)
}

/// Store extracted metadata. Writes an immutable snapshot the first time a file is read.
pub fn apply_metadata(conn: &Connection, id: i64, m: &ExtractedMetadata) -> Result<()> {
    let now = now_ms();
    let (fs_birth, fs_mtime): (Option<i64>, i64) = conn.query_row(
        "SELECT filesystem_creation_time, filesystem_modification_time FROM media WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let provisional_sort = timefmt::ms_to_local_naive_string(fs_birth.unwrap_or(fs_mtime));
    let (cap_dt, cap_tz, cap_src) = match &m.capture {
        Some(c) => (Some(c.local.clone()), c.offset.clone(), Some(c.source.as_str().to_string())),
        None => (None, None, None),
    };
    conn.execute(
        "UPDATE media SET
            capture_datetime = CASE WHEN capture_time_source = 'derived_clip' THEN capture_datetime ELSE ?2 END,
            capture_timezone = CASE WHEN capture_time_source = 'derived_clip' THEN capture_timezone ELSE ?3 END,
            capture_time_source = CASE WHEN capture_time_source = 'derived_clip' THEN capture_time_source ELSE ?4 END,
            width = ?5, height = ?6, orientation = ?7, duration = ?8, codec = ?9, audio_codec = ?10,
            frame_rate = ?11, bitrate = ?12, sample_rate = ?13, camera_make = ?14, camera_model = ?15,
            lens = ?16, exposure = ?17, gps_latitude = ?18, gps_longitude = ?19, gps_altitude = ?20,
            title = ?21, artist = ?22, album = ?23, genre = ?24, embedded_caption = ?25, embedded_keywords = ?26,
            mime_type = COALESCE(?27, mime_type),
            metadata_status = 'done', metadata_error = NULL, updated_at = ?28,
            sort_datetime = COALESCE(user_capture_datetime, CASE WHEN capture_time_source = 'derived_clip' THEN capture_datetime ELSE ?2 END, ?29)
         WHERE id = ?1",
        params![
            id, cap_dt, cap_tz, cap_src, m.width, m.height, m.orientation, m.duration, m.codec, m.audio_codec,
            m.frame_rate, m.bitrate, m.sample_rate, m.camera_make, m.camera_model, m.lens, m.exposure,
            m.gps_latitude, m.gps_longitude, m.gps_altitude, m.title, m.artist, m.album, m.genre,
            m.caption, m.keywords, m.mime_type, now, provisional_sort
        ],
    )?;
    let has_snapshot: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM metadata_snapshots WHERE media_id = ?1)", [id], |r| r.get(0))?;
    if !has_snapshot {
        conn.execute(
            "INSERT INTO metadata_snapshots (media_id, original_metadata_json, extractor, extracted_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, serde_json::to_string(&m.raw)?, m.extractor, now],
        )?;
    }
    refresh_fts(conn, id)?;
    Ok(())
}

pub fn mark_metadata_error(conn: &Connection, id: i64, err: &str) -> Result<()> {
    conn.execute(
        "UPDATE media SET metadata_status = 'error', metadata_error = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, err, now_ms()],
    )?;
    Ok(())
}

pub fn set_thumbnail_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    conn.execute("UPDATE media SET thumbnail_status = ?2 WHERE id = ?1", params![id, status])?;
    Ok(())
}

pub fn set_hash(conn: &Connection, id: i64, hash: &str) -> Result<()> {
    conn.execute("UPDATE media SET file_hash = ?2 WHERE id = ?1", params![id, hash])?;
    Ok(())
}

/// Minimal info needed to work with the physical file.
#[derive(Debug, Clone)]
pub struct MediaLocation {
    pub id: i64,
    pub path: String,
    pub media_type: MediaType,
    pub extension: String,
    pub orientation: Option<i64>,
    pub duration: Option<f64>,
    pub updated_at: i64,
}

pub fn location(conn: &Connection, id: i64) -> Result<MediaLocation> {
    conn.query_row(
        "SELECT id, current_path, media_type, extension, orientation, duration, updated_at FROM media WHERE id = ?1",
        [id],
        |r| {
            Ok(MediaLocation {
                id: r.get(0)?,
                path: r.get(1)?,
                media_type: MediaType::parse(&r.get::<_, String>(2)?).unwrap_or(MediaType::Photo),
                extension: r.get(3)?,
                orientation: r.get(4)?,
                duration: r.get(5)?,
                updated_at: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CuratorError::NotFound(format!("media {id}")))
}

/// Rebuild the full-text row for a media item.
pub fn refresh_fts(conn: &Connection, id: i64) -> Result<()> {
    let text: Option<String> = conn
        .query_row(
            "SELECT concat_ws(' ', m.current_filename, m.original_filename, m.folder_path, m.description, m.notes, m.location,
                    m.camera_make, m.camera_model, m.lens, m.title, m.artist, m.album, m.genre, m.embedded_caption,
                    m.embedded_keywords, m.codec,
                    (SELECT group_concat(t.name, ' ') FROM media_tags mt JOIN tags t ON t.id = mt.tag_id WHERE mt.media_id = m.id))
             FROM media m WHERE m.id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    conn.execute("DELETE FROM media_fts WHERE rowid = ?1", [id])?;
    if let Some(t) = text {
        // Split path separators/underscores so "GOPR0042" and folder names are searchable words.
        let t = t.replace(['/', '\\', '_', '-', '.'], " ");
        conn.execute("INSERT INTO media_fts (rowid, content) VALUES (?1, ?2)", params![id, t])?;
    }
    Ok(())
}

// ----------------------------------------------------------------------------------------------
// Querying
// ----------------------------------------------------------------------------------------------

struct WhereBuilder {
    clauses: Vec<String>,
    params: Vec<Box<dyn ToSql>>,
}

impl WhereBuilder {
    fn new() -> Self {
        Self { clauses: vec![], params: vec![] }
    }
    fn push(&mut self, clause: impl Into<String>, p: Vec<Box<dyn ToSql>>) {
        let mut clause = clause.into();
        // Replace each "?" placeholder with a numbered one.
        let mut out = String::with_capacity(clause.len() + 8);
        for ch in clause.drain(..) {
            if ch == '?' {
                self.params.push(Box::new(rusqlite::types::Null)); // placeholder, replaced below
                out.push_str(&format!("?{}", self.params.len()));
            } else {
                out.push(ch);
            }
        }
        let start = self.params.len() - p.len();
        for (i, v) in p.into_iter().enumerate() {
            self.params[start + i] = v;
        }
        self.clauses.push(out);
    }
    fn sql(&self) -> String {
        if self.clauses.is_empty() {
            "1=1".into()
        } else {
            self.clauses.join(" AND ")
        }
    }
}

/// Convert free text into a safe FTS5 prefix query: each word must match.
pub fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn apply_filter(conn: &Connection, w: &mut WhereBuilder, f: &MediaFilter, depth: u8) -> Result<()> {
    if let Some(q) = f.text.as_deref().and_then(fts_query) {
        w.push("m.id IN (SELECT rowid FROM media_fts WHERE media_fts MATCH ?)", vec![Box::new(q)]);
    }
    if !f.media_types.is_empty() {
        let ph = vec!["?"; f.media_types.len()].join(",");
        w.push(
            format!("m.media_type IN ({ph})"),
            f.media_types.iter().map(|t| Box::new(t.as_str().to_string()) as Box<dyn ToSql>).collect(),
        );
    }
    if f.unrated_only {
        w.push("m.rating = 0", vec![]);
    }
    if let Some(r) = f.min_rating {
        w.push("m.rating >= ?", vec![Box::new(r)]);
    }
    if let Some(r) = f.max_rating {
        w.push("m.rating <= ?", vec![Box::new(r)]);
    }
    if let Some(s) = &f.selection_status {
        w.push("m.selection_status = ?", vec![Box::new(s.clone())]);
    }
    if let Some(s) = &f.review_status {
        w.push("m.review_status = ?", vec![Box::new(s.clone())]);
    }
    if f.favorite_only {
        w.push("m.favorite = 1", vec![]);
    }
    if let Some(c) = &f.color_label {
        w.push("m.color_label = ?", vec![Box::new(c.clone())]);
    }
    for tag in &f.tag_ids {
        w.push(
            "m.id IN (SELECT media_id FROM media_tags WHERE tag_id IN (
                WITH RECURSIVE d(id) AS (SELECT ? UNION ALL SELECT t.id FROM tags t JOIN d ON t.parent_id = d.id) SELECT id FROM d))",
            vec![Box::new(*tag)],
        );
    }
    if let Some(cid) = f.collection_id {
        let row: Option<(String, Option<String>)> = conn
            .query_row("SELECT collection_type, rules_json FROM collections WHERE id = ?1", [cid], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        match row {
            Some((t, Some(rules))) if t == "smart" => {
                if depth > 3 {
                    return Err(CuratorError::invalid("smart collections are nested too deeply"));
                }
                let rules: MediaFilter = serde_json::from_str(&rules)?;
                apply_filter(conn, w, &rules, depth + 1)?;
            }
            Some(_) => w.push("m.id IN (SELECT media_id FROM collection_media WHERE collection_id = ?)", vec![Box::new(cid)]),
            None => w.push("0", vec![]),
        }
    }
    if let Some(s) = f.source_id {
        w.push("m.source_id = ?", vec![Box::new(s)]);
    }
    if let Some(folder) = &f.folder {
        let folder = folder.trim_end_matches('/').to_string();
        w.push("(m.folder_path = ? OR m.folder_path LIKE ? ESCAPE '\\')", vec![
            Box::new(folder.clone()),
            Box::new(format!("{}/%", like_escape(&folder))),
        ]);
    }
    if let Some(d) = &f.date_from {
        w.push("m.sort_datetime >= ?", vec![Box::new(d.clone())]);
    }
    if let Some(d) = &f.date_to {
        // inclusive end date: everything before the following day
        w.push("m.sort_datetime < ?", vec![Box::new(format!("{d}T99"))]);
    }
    if let Some(c) = &f.camera {
        w.push("(m.camera_model LIKE ? OR m.camera_make LIKE ?)", vec![
            Box::new(format!("%{c}%")),
            Box::new(format!("%{c}%")),
        ]);
    }
    if let Some(d) = f.min_duration {
        w.push("m.duration >= ?", vec![Box::new(d)]);
    }
    if let Some(d) = f.max_duration {
        w.push("m.duration <= ?", vec![Box::new(d)]);
    }
    if let Some(wd) = f.min_width {
        w.push("MAX(COALESCE(m.width,0), COALESCE(m.height,0)) >= ?", vec![Box::new(wd)]);
    }
    if let Some(t) = f.added_since_ms {
        w.push("m.added_at >= ?", vec![Box::new(t)]);
    }
    if f.missing_only {
        w.push("m.is_missing = 1", vec![]);
    }
    if f.no_capture_date {
        w.push("m.metadata_status = 'done' AND m.capture_datetime IS NULL AND m.user_capture_datetime IS NULL", vec![]);
    }
    match f.has_location {
        Some(true) => w.push("m.gps_latitude IS NOT NULL", vec![]),
        Some(false) => w.push("m.gps_latitude IS NULL", vec![]),
        None => {}
    }
    Ok(())
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn order_by(sort: SortField, desc: bool) -> String {
    let dir = if desc { "DESC" } else { "ASC" };
    match sort {
        SortField::CaptureDate => format!("m.sort_datetime IS NULL, m.sort_datetime {dir}, m.id {dir}"),
        SortField::Filename => format!("m.current_filename COLLATE NOCASE {dir}, m.id {dir}"),
        SortField::Rating => format!("m.rating {dir}, m.sort_datetime {dir}, m.id {dir}"),
        SortField::FileSize => format!("m.file_size {dir}, m.id {dir}"),
        SortField::AddedAt => format!("m.added_at {dir}, m.id {dir}"),
        SortField::Duration => format!("m.duration IS NULL, m.duration {dir}, m.id {dir}"),
    }
}

const SUMMARY_COLS: &str = "m.id, m.media_type, m.current_filename, m.sort_datetime, m.width, m.height, m.duration, m.rating,
    m.selection_status, m.review_status, m.favorite, m.color_label, m.thumbnail_status, m.is_missing, m.file_size, m.orientation,
    (m.capture_datetime IS NOT NULL OR m.user_capture_datetime IS NOT NULL)";

fn summary_from_row(r: &Row) -> rusqlite::Result<MediaSummary> {
    let (mut w, mut h): (Option<i64>, Option<i64>) = (r.get(4)?, r.get(5)?);
    // Report display dimensions (rotated for EXIF orientations 5-8).
    if matches!(r.get::<_, Option<i64>>(15)?, Some(5..=8)) {
        std::mem::swap(&mut w, &mut h);
    }
    Ok(MediaSummary {
        id: r.get(0)?,
        media_type: MediaType::parse(&r.get::<_, String>(1)?).unwrap_or(MediaType::Photo),
        filename: r.get(2)?,
        sort_datetime: r.get(3)?,
        capture_known: r.get(16)?,
        width: w,
        height: h,
        duration: r.get(6)?,
        rating: r.get(7)?,
        selection_status: r.get(8)?,
        review_status: r.get(9)?,
        favorite: r.get(10)?,
        color_label: r.get(11)?,
        thumbnail_status: r.get(12)?,
        is_missing: r.get(13)?,
        file_size: r.get(14)?,
    })
}

pub fn query(conn: &Connection, q: &MediaQuery) -> Result<MediaPage> {
    let mut w = WhereBuilder::new();
    apply_filter(conn, &mut w, &q.filter, 0)?;
    let where_sql = w.sql();
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM media m WHERE {where_sql}"),
        params_from_iter(w.params.iter()),
        |r| r.get(0),
    )?;
    let limit = if q.limit <= 0 { 200 } else { q.limit.min(5000) };
    let sql = format!(
        "SELECT {SUMMARY_COLS} FROM media m WHERE {where_sql} ORDER BY {} LIMIT {limit} OFFSET {}",
        order_by(q.sort, q.descending),
        q.offset.max(0)
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt.query_map(params_from_iter(w.params.iter()), summary_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(MediaPage { total, offset: q.offset.max(0), items })
}

/// All ids matching a filter in display order (used for select-all and batch actions).
pub fn query_ids(conn: &Connection, filter: &MediaFilter, sort: SortField, descending: bool) -> Result<Vec<i64>> {
    let mut w = WhereBuilder::new();
    apply_filter(conn, &mut w, filter, 0)?;
    let sql = format!("SELECT m.id FROM media m WHERE {} ORDER BY {}", w.sql(), order_by(sort, descending));
    let mut stmt = conn.prepare(&sql)?;
    let ids = stmt.query_map(params_from_iter(w.params.iter()), |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

pub fn count(conn: &Connection, filter: &MediaFilter) -> Result<i64> {
    let mut w = WhereBuilder::new();
    apply_filter(conn, &mut w, filter, 0)?;
    Ok(conn.query_row(&format!("SELECT COUNT(*) FROM media m WHERE {}", w.sql()), params_from_iter(w.params.iter()), |r| r.get(0))?)
}

pub fn detail(conn: &Connection, id: i64) -> Result<MediaDetail> {
    let mut d = conn
        .query_row(
            "SELECT id, source_id, original_filename, current_filename, original_path, current_path, folder_path, extension,
                media_type, mime_type, file_size, file_hash, capture_datetime, capture_timezone, capture_time_source,
                user_capture_datetime, user_capture_timezone, sort_datetime, filesystem_creation_time,
                filesystem_modification_time, width, height, orientation, duration, codec, audio_codec, frame_rate,
                bitrate, sample_rate, camera_make, camera_model, lens, exposure, gps_latitude, gps_longitude,
                gps_altitude, title, artist, album, genre, embedded_caption, embedded_keywords, rating,
                selection_status, review_status, favorite, color_label, description, location, notes,
                metadata_status, metadata_error, thumbnail_status, is_missing, added_at, last_scanned_at
             FROM media WHERE id = ?1",
            [id],
            |r| {
                Ok(MediaDetail {
                    id: r.get(0)?,
                    source_id: r.get(1)?,
                    original_filename: r.get(2)?,
                    current_filename: r.get(3)?,
                    original_path: r.get(4)?,
                    current_path: r.get(5)?,
                    folder_path: r.get(6)?,
                    extension: r.get(7)?,
                    media_type: r.get(8)?,
                    mime_type: r.get(9)?,
                    file_size: r.get(10)?,
                    file_hash: r.get(11)?,
                    capture_datetime: r.get(12)?,
                    capture_timezone: r.get(13)?,
                    capture_time_source: r.get(14)?,
                    user_capture_datetime: r.get(15)?,
                    user_capture_timezone: r.get(16)?,
                    sort_datetime: r.get(17)?,
                    filesystem_creation_time: r.get(18)?,
                    filesystem_modification_time: r.get(19)?,
                    width: r.get(20)?,
                    height: r.get(21)?,
                    orientation: r.get(22)?,
                    duration: r.get(23)?,
                    codec: r.get(24)?,
                    audio_codec: r.get(25)?,
                    frame_rate: r.get(26)?,
                    bitrate: r.get(27)?,
                    sample_rate: r.get(28)?,
                    camera_make: r.get(29)?,
                    camera_model: r.get(30)?,
                    lens: r.get(31)?,
                    exposure: r.get(32)?,
                    gps_latitude: r.get(33)?,
                    gps_longitude: r.get(34)?,
                    gps_altitude: r.get(35)?,
                    title: r.get(36)?,
                    artist: r.get(37)?,
                    album: r.get(38)?,
                    genre: r.get(39)?,
                    embedded_caption: r.get(40)?,
                    embedded_keywords: r.get(41)?,
                    rating: r.get(42)?,
                    selection_status: r.get(43)?,
                    review_status: r.get(44)?,
                    favorite: r.get(45)?,
                    color_label: r.get(46)?,
                    description: r.get(47)?,
                    location: r.get(48)?,
                    notes: r.get(49)?,
                    metadata_status: r.get(50)?,
                    metadata_error: r.get(51)?,
                    thumbnail_status: r.get(52)?,
                    is_missing: r.get(53)?,
                    added_at: r.get(54)?,
                    last_scanned_at: r.get(55)?,
                    ..Default::default()
                })
            },
        )
        .optional()?
        .ok_or_else(|| CuratorError::NotFound(format!("media {id}")))?;
    d.tags = super::tags::tags_for_media(conn, id)?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name FROM collection_media cm JOIN collections c ON c.id = cm.collection_id WHERE cm.media_id = ?1 ORDER BY c.name",
    )?;
    d.collections = stmt.query_map([id], |r| Ok(CollectionRef { id: r.get(0)?, name: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;
    d.derived_segments = conn.query_row("SELECT COUNT(*) FROM video_segments WHERE source_media_id = ?1 AND exported_path IS NOT NULL", [id], |r| r.get(0))?;
    d.clip_origin = conn
        .query_row(
            "SELECT vs.id, vs.source_media_id, m.current_filename, vs.in_point, vs.out_point, vs.actual_in_point, vs.processing_method, vs.preset
             FROM video_segments vs JOIN media m ON m.id = vs.source_media_id WHERE vs.output_media_id = ?1 ORDER BY vs.id DESC LIMIT 1",
            [id],
            |r| {
                Ok(ClipOrigin {
                    segment_id: r.get(0)?,
                    source_media_id: r.get(1)?,
                    source_filename: r.get(2)?,
                    in_point: r.get(3)?,
                    out_point: r.get(4)?,
                    actual_in_point: r.get(5)?,
                    processing_method: r.get(6)?,
                    preset: r.get(7)?,
                })
            },
        )
        .optional()?;
    Ok(d)
}

/// The original metadata snapshot (everything the extractor returned), for the Advanced panel.
pub fn metadata_snapshot(conn: &Connection, id: i64) -> Result<Option<serde_json::Value>> {
    let s: Option<String> = conn
        .query_row("SELECT original_metadata_json FROM metadata_snapshots WHERE media_id = ?1 ORDER BY id LIMIT 1", [id], |r| r.get(0))
        .optional()?;
    Ok(match s {
        Some(s) => Some(serde_json::from_str(&s)?),
        None => None,
    })
}

const SELECTIONS: &[&str] = &["none", "selected", "rejected"];
const REVIEWS: &[&str] = &["none", "needs_review", "reviewed"];
const COLORS: &[&str] = &["red", "orange", "yellow", "green", "blue", "purple"];

/// Apply curation changes to many items in one transaction.
pub fn update_curation(conn: &mut Connection, ids: &[i64], u: &CurationUpdate) -> Result<usize> {
    if let Some(r) = u.rating {
        if !(0..=5).contains(&r) {
            return Err(CuratorError::invalid("rating must be between 0 and 5"));
        }
    }
    if let Some(s) = &u.selection_status {
        if !SELECTIONS.contains(&s.as_str()) {
            return Err(CuratorError::invalid(format!("unknown selection status {s}")));
        }
    }
    if let Some(s) = &u.review_status {
        if !REVIEWS.contains(&s.as_str()) {
            return Err(CuratorError::invalid(format!("unknown review status {s}")));
        }
    }
    if let Some(c) = &u.color_label {
        if !c.is_empty() && !COLORS.contains(&c.as_str()) {
            return Err(CuratorError::invalid(format!("unknown color label {c}")));
        }
    }
    let text_changed = u.description.is_some() || u.location.is_some() || u.notes.is_some();
    let now = now_ms();
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut stmt = tx.prepare(
            "UPDATE media SET
                rating = COALESCE(?2, rating),
                selection_status = COALESCE(?3, selection_status),
                review_status = COALESCE(?4, review_status),
                favorite = COALESCE(?5, favorite),
                color_label = CASE WHEN ?6 IS NULL THEN color_label WHEN ?6 = '' THEN NULL ELSE ?6 END,
                description = CASE WHEN ?7 IS NULL THEN description WHEN ?7 = '' THEN NULL ELSE ?7 END,
                location = CASE WHEN ?8 IS NULL THEN location WHEN ?8 = '' THEN NULL ELSE ?8 END,
                notes = CASE WHEN ?9 IS NULL THEN notes WHEN ?9 = '' THEN NULL ELSE ?9 END,
                updated_at = ?10
             WHERE id = ?1",
        )?;
        for id in ids {
            n += stmt.execute(params![
                id,
                u.rating,
                u.selection_status,
                u.review_status,
                u.favorite,
                u.color_label,
                u.description,
                u.location,
                u.notes,
                now
            ])?;
            if text_changed {
                refresh_fts(&tx, *id)?;
            }
        }
    }
    tx.commit()?;
    Ok(n)
}

/// Set or clear a user correction of the capture time. The extracted value is kept untouched.
pub fn set_capture_correction(conn: &Connection, id: i64, local: Option<&str>, offset: Option<&str>) -> Result<()> {
    if let Some(l) = local {
        timefmt::parse_local(l).ok_or_else(|| CuratorError::invalid(format!("'{l}' is not a valid date/time (YYYY-MM-DDTHH:MM:SS)")))?;
    }
    if let Some(o) = offset {
        timefmt::parse_offset(o).ok_or_else(|| CuratorError::invalid(format!("'{o}' is not a valid UTC offset (+HH:MM)")))?;
    }
    let local = local.map(|l| timefmt::normalize_local(l).unwrap_or_else(|| l.to_string()));
    let (birth, mtime): (Option<i64>, i64) = conn.query_row(
        "SELECT filesystem_creation_time, filesystem_modification_time FROM media WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let provisional = timefmt::ms_to_local_naive_string(birth.unwrap_or(mtime));
    conn.execute(
        "UPDATE media SET user_capture_datetime = ?2, user_capture_timezone = ?3, updated_at = ?4,
            sort_datetime = COALESCE(?2, capture_datetime, ?5) WHERE id = ?1",
        params![id, local, if local.is_some() { offset } else { None }, now_ms(), provisional],
    )?;
    Ok(())
}

/// Distinct camera models (for filter suggestions).
pub fn cameras(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT CASE WHEN camera_make IS NULL OR camera_model LIKE substr(camera_make, 1, instr(camera_make || ' ', ' ') - 1) || '%' THEN camera_model ELSE camera_make || ' ' || camera_model END AS c FROM media WHERE camera_model IS NOT NULL
         GROUP BY c ORDER BY COUNT(*) DESC LIMIT 100",
    )?;
    let v = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(v)
}

/// Distinct folders containing media, with counts (for the folder tree).
pub fn folders(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT folder_path, COUNT(*) FROM media GROUP BY folder_path ORDER BY folder_path")?;
    let v = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Candidates for hashing: same file size as at least one other file and not yet hashed.
pub fn hash_candidates(conn: &Connection) -> Result<Vec<(i64, String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT id, current_path, file_size FROM media
         WHERE file_hash IS NULL AND is_missing = 0 AND file_size > 0
           AND file_size IN (SELECT file_size FROM media WHERE is_missing = 0 GROUP BY file_size HAVING COUNT(*) > 1)
         ORDER BY file_size",
    )?;
    let v = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub file_hash: String,
    pub file_size: i64,
    pub items: Vec<DuplicateItem>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateItem {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub media_type: String,
    pub rating: i64,
    pub selection_status: String,
    pub sort_datetime: Option<String>,
}

/// Groups of byte-identical files (same BLAKE3 hash). Nothing is ever deleted automatically.
pub fn duplicate_groups(conn: &Connection) -> Result<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT file_hash, file_size, id, current_path, current_filename, media_type, rating, selection_status, sort_datetime
         FROM media WHERE file_hash IN (SELECT file_hash FROM media WHERE file_hash IS NOT NULL GROUP BY file_hash HAVING COUNT(*) > 1)
         ORDER BY file_size DESC, file_hash, id",
    )?;
    let mut groups: Vec<DuplicateGroup> = vec![];
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            DuplicateItem {
                id: r.get(2)?,
                path: r.get(3)?,
                filename: r.get(4)?,
                media_type: r.get(5)?,
                rating: r.get(6)?,
                selection_status: r.get(7)?,
                sort_datetime: r.get(8)?,
            },
        ))
    })?;
    for row in rows {
        let (hash, size, item) = row?;
        match groups.last_mut() {
            Some(g) if g.file_hash == hash => g.items.push(item),
            _ => groups.push(DuplicateGroup { file_hash: hash, file_size: size, items: vec![item] }),
        }
    }
    Ok(groups)
}

/// Copy curation (ratings, flags, labels, text, tags, user date correction) from one record to another.
pub fn copy_curation(conn: &Connection, from: i64, to: i64) -> Result<()> {
    conn.execute(
        "UPDATE media SET (rating, selection_status, review_status, favorite, color_label, description, location, notes,
                user_capture_datetime, user_capture_timezone) =
            (SELECT rating, selection_status, review_status, favorite, color_label, description, location, notes,
                user_capture_datetime, user_capture_timezone FROM media WHERE id = ?1)
         WHERE id = ?2",
        params![from, to],
    )?;
    conn.execute("INSERT OR IGNORE INTO media_tags (media_id, tag_id) SELECT ?2, tag_id FROM media_tags WHERE media_id = ?1", params![from, to])?;
    conn.execute("UPDATE media SET sort_datetime = COALESCE(user_capture_datetime, capture_datetime, sort_datetime) WHERE id = ?1", [to])?;
    refresh_fts(conn, to)?;
    Ok(())
}

/// Tags present on any of `ids`, with how many of them carry each (batch tag editing).
pub fn tag_counts(conn: &Connection, ids: &[i64]) -> Result<Vec<(i64, i64)>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let ph = vec!["?"; ids.len()].join(",");
    let mut stmt = conn.prepare(&format!("SELECT tag_id, COUNT(*) FROM media_tags WHERE media_id IN ({ph}) GROUP BY tag_id"))?;
    let v = stmt.query_map(params_from_iter(ids.iter()), |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
