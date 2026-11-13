//! Source folders registered for scanning.

use crate::core::errors::Result;
use crate::database::models::Source;
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{self, params, Connection, OptionalExtension};
use std::path::Path;

pub fn get_or_create(conn: &Connection, path: &Path, is_library: bool) -> Result<i64> {
    let p = path.to_string_lossy().to_string();
    if let Some(id) = conn.query_row("SELECT id FROM sources WHERE path = ?1", [&p], |r| r.get(0)).optional()? {
        if is_library {
            conn.execute("UPDATE sources SET is_library = 1 WHERE id = ?1", [id])?;
        }
        return Ok(id);
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
    conn.execute(
        "INSERT INTO sources (path, name, is_library, added_at) VALUES (?1, ?2, ?3, ?4)",
        params![p, name, is_library, now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Source> {
    Ok(list(conn)?.into_iter().find(|s| s.id == id).ok_or_else(|| crate::CuratorError::NotFound(format!("source {id}")))?)
}

pub fn list(conn: &Connection) -> Result<Vec<Source>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.path, s.name, s.is_library, s.added_at, s.last_scan_at,
            COUNT(m.id), COALESCE(SUM(m.file_size), 0)
         FROM sources s LEFT JOIN media m ON m.source_id = s.id
         GROUP BY s.id ORDER BY s.name COLLATE NOCASE",
    )?;
    let v = stmt
        .query_map([], |r| {
            let path: String = r.get(1)?;
            let is_available = Path::new(&path).is_dir();
            Ok(Source {
                id: r.get(0)?,
                path,
                name: r.get(2)?,
                is_library: r.get(3)?,
                added_at: r.get(4)?,
                last_scan_at: r.get(5)?,
                media_count: r.get(6)?,
                total_size: r.get(7)?,
                is_available,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn set_scanned(conn: &Connection, id: i64, at: i64) -> Result<()> {
    conn.execute("UPDATE sources SET last_scan_at = ?2 WHERE id = ?1", params![id, at])?;
    Ok(())
}

/// Removes a source and its catalog records (ratings, tags, collection membership).
/// Files on disk are never touched.
pub fn remove(conn: &mut Connection, id: i64) -> Result<usize> {
    let tx = conn.transaction()?;
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM media WHERE source_id = ?1")?;
        let v = stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        v
    };
    // Media that are the source of extracted clips are protected by a RESTRICT foreign key.
    let protected: i64 = tx.query_row(
        "SELECT COUNT(*) FROM video_segments vs JOIN media m ON m.id = vs.source_media_id WHERE m.source_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if protected > 0 {
        return Err(crate::CuratorError::Safety(format!(
            "{protected} clip(s) were extracted from media in this folder; their provenance would be lost"
        )));
    }
    for m in &ids {
        tx.execute("DELETE FROM media_fts WHERE rowid = ?1", [m])?;
    }
    let n = tx.execute("DELETE FROM media WHERE source_id = ?1", [id])?;
    tx.execute("DELETE FROM sources WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(n)
}
