//! Library statistics for the dashboard. Everything is computed from the catalog.

use crate::core::errors::Result;
use r2d2_sqlite::rusqlite::{self, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TypeBreakdown {
    pub media_type: String,
    pub count: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub total_media: i64,
    pub total_photos: i64,
    pub total_videos: i64,
    pub total_audio: i64,
    pub total_bytes: i64,
    pub total_selected: i64,
    pub total_rejected: i64,
    pub total_five_star: i64,
    pub total_favorites: i64,
    pub total_unrated: i64,
    pub total_needs_review: i64,
    pub total_video_seconds: f64,
    pub total_audio_seconds: f64,
    pub missing: i64,
    pub metadata_pending: i64,
    pub metadata_errors: i64,
    pub thumbnails_pending: i64,
    pub duplicate_groups: i64,
    pub duplicate_files: i64,
    pub duplicate_wasted_bytes: i64,
    pub without_capture_date: i64,
    pub by_type: Vec<TypeBreakdown>,
    /// Count per rating 0..=5.
    pub rating_distribution: Vec<i64>,
    pub by_year: Vec<YearCount>,
    pub top_cameras: Vec<CameraCount>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearCount {
    pub year: String,
    pub photos: i64,
    pub videos: i64,
    pub audio: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraCount {
    pub camera: String,
    pub count: i64,
}

pub fn library_stats(conn: &Connection) -> Result<LibraryStats> {
    let mut s = conn.query_row(
        "SELECT COUNT(*),
            COALESCE(SUM(media_type = 'photo'), 0), COALESCE(SUM(media_type = 'video'), 0), COALESCE(SUM(media_type = 'audio'), 0),
            COALESCE(SUM(file_size), 0),
            COALESCE(SUM(selection_status = 'selected'), 0), COALESCE(SUM(selection_status = 'rejected'), 0),
            COALESCE(SUM(rating = 5), 0), COALESCE(SUM(favorite = 1), 0), COALESCE(SUM(rating = 0), 0),
            COALESCE(SUM(review_status = 'needs_review'), 0),
            COALESCE(SUM(CASE WHEN media_type = 'video' THEN duration END), 0),
            COALESCE(SUM(CASE WHEN media_type = 'audio' THEN duration END), 0),
            COALESCE(SUM(is_missing), 0),
            COALESCE(SUM(metadata_status = 'pending'), 0), COALESCE(SUM(metadata_status = 'error'), 0),
            COALESCE(SUM(thumbnail_status = 'pending'), 0),
            COALESCE(SUM(metadata_status = 'done' AND capture_datetime IS NULL AND user_capture_datetime IS NULL), 0)
         FROM media",
        [],
        |r| {
            Ok(LibraryStats {
                total_media: r.get(0)?,
                total_photos: r.get(1)?,
                total_videos: r.get(2)?,
                total_audio: r.get(3)?,
                total_bytes: r.get(4)?,
                total_selected: r.get(5)?,
                total_rejected: r.get(6)?,
                total_five_star: r.get(7)?,
                total_favorites: r.get(8)?,
                total_unrated: r.get(9)?,
                total_needs_review: r.get(10)?,
                total_video_seconds: r.get(11)?,
                total_audio_seconds: r.get(12)?,
                missing: r.get(13)?,
                metadata_pending: r.get(14)?,
                metadata_errors: r.get(15)?,
                thumbnails_pending: r.get(16)?,
                without_capture_date: r.get(17)?,
                ..Default::default()
            })
        },
    )?;

    let mut stmt = conn.prepare("SELECT media_type, COUNT(*), COALESCE(SUM(file_size),0) FROM media GROUP BY media_type ORDER BY 3 DESC")?;
    s.by_type = stmt
        .query_map([], |r| Ok(TypeBreakdown { media_type: r.get(0)?, count: r.get(1)?, bytes: r.get(2)? }))?
        .collect::<rusqlite::Result<_>>()?;

    let mut dist = vec![0i64; 6];
    let mut stmt = conn.prepare("SELECT rating, COUNT(*) FROM media GROUP BY rating")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (rating, c) = row?;
        if (0..=5).contains(&rating) {
            dist[rating as usize] = c;
        }
    }
    s.rating_distribution = dist;

    let mut stmt = conn.prepare(
        "SELECT COALESCE(substr(sort_datetime, 1, 4), 'Unknown') y,
            SUM(media_type='photo'), SUM(media_type='video'), SUM(media_type='audio')
         FROM media GROUP BY y ORDER BY y",
    )?;
    s.by_year = stmt
        .query_map([], |r| Ok(YearCount { year: r.get(0)?, photos: r.get(1)?, videos: r.get(2)?, audio: r.get(3)? }))?
        .collect::<rusqlite::Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT CASE WHEN camera_make IS NULL OR camera_model LIKE substr(camera_make, 1, instr(camera_make || ' ', ' ') - 1) || '%' THEN camera_model ELSE camera_make || ' ' || camera_model END c, COUNT(*) FROM media
         WHERE camera_model IS NOT NULL GROUP BY c ORDER BY 2 DESC LIMIT 8",
    )?;
    s.top_cameras = stmt.query_map([], |r| Ok(CameraCount { camera: r.get(0)?, count: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;

    let (groups, files, wasted): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(n),0), COALESCE(SUM((n-1)*sz),0) FROM
            (SELECT COUNT(*) n, MAX(file_size) sz FROM media WHERE file_hash IS NOT NULL GROUP BY file_hash HAVING COUNT(*) > 1)",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    s.duplicate_groups = groups;
    s.duplicate_files = files;
    s.duplicate_wasted_bytes = wasted;
    Ok(s)
}
