//! Virtual collections (manual and smart). Collections never move or modify files.

use crate::core::errors::{CuratorError, Result};
use crate::database::models::{Collection, MediaFilter};
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{self, params, Connection};

pub fn list(conn: &Connection) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, collection_type, rules_json, created_at,
            (SELECT COUNT(*) FROM collection_media cm WHERE cm.collection_id = c.id)
         FROM collections c ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, name, description, ctype, rules_json, created_at, manual_count) in rows {
        let rules: Option<MediaFilter> = rules_json.as_deref().map(serde_json::from_str).transpose()?;
        let media_count = if ctype == "smart" {
            super::media::count(conn, &MediaFilter { collection_id: Some(id), ..Default::default() })?
        } else {
            manual_count
        };
        out.push(Collection { id, name, description, collection_type: ctype, rules, media_count, created_at });
    }
    Ok(out)
}

pub fn create_manual(conn: &Connection, name: &str, description: Option<&str>) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CuratorError::invalid("collection name cannot be empty"));
    }
    let now = now_ms();
    conn.execute(
        "INSERT INTO collections (name, description, collection_type, created_at, updated_at) VALUES (?1, ?2, 'manual', ?3, ?3)",
        params![name, description, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn create_smart(conn: &Connection, name: &str, rules: &MediaFilter) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CuratorError::invalid("collection name cannot be empty"));
    }
    let now = now_ms();
    conn.execute(
        "INSERT INTO collections (name, collection_type, rules_json, created_at, updated_at) VALUES (?1, 'smart', ?2, ?3, ?3)",
        params![name, serde_json::to_string(rules)?, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_rules(conn: &Connection, id: i64, rules: &MediaFilter) -> Result<()> {
    conn.execute(
        "UPDATE collections SET rules_json = ?2, updated_at = ?3 WHERE id = ?1 AND collection_type = 'smart'",
        params![id, serde_json::to_string(rules)?, now_ms()],
    )?;
    Ok(())
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CuratorError::invalid("collection name cannot be empty"));
    }
    conn.execute("UPDATE collections SET name = ?2, updated_at = ?3 WHERE id = ?1", params![id, name, now_ms()])?;
    Ok(())
}

/// Removes the collection only; media and files are unaffected.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM collections WHERE id = ?1", [id])?;
    Ok(())
}

fn ensure_manual(conn: &Connection, id: i64) -> Result<()> {
    let t: String = conn.query_row("SELECT collection_type FROM collections WHERE id = ?1", [id], |r| r.get(0))?;
    if t != "manual" {
        return Err(CuratorError::invalid("smart collections update automatically; items cannot be added by hand"));
    }
    Ok(())
}

pub fn add_media(conn: &mut Connection, id: i64, media_ids: &[i64]) -> Result<usize> {
    ensure_manual(conn, id)?;
    let tx = conn.transaction()?;
    let mut next: i64 = tx.query_row("SELECT COALESCE(MAX(sequence_order), 0) + 1 FROM collection_media WHERE collection_id = ?1", [id], |r| r.get(0))?;
    let now = now_ms();
    let mut n = 0;
    for m in media_ids {
        let added = tx.execute(
            "INSERT OR IGNORE INTO collection_media (collection_id, media_id, sequence_order, added_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, m, next, now],
        )?;
        next += added as i64;
        n += added;
    }
    tx.execute("UPDATE collections SET updated_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.commit()?;
    Ok(n)
}

pub fn remove_media(conn: &mut Connection, id: i64, media_ids: &[i64]) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for m in media_ids {
        n += tx.execute("DELETE FROM collection_media WHERE collection_id = ?1 AND media_id = ?2", params![id, m])?;
    }
    tx.commit()?;
    Ok(n)
}
