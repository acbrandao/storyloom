//! Hierarchical tags.

use crate::core::errors::{CuratorError, Result};
use crate::database::models::Tag;
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{self, params, Connection, OptionalExtension};

const TAG_TREE_CTE: &str = "WITH RECURSIVE tree(id, name, parent_id, path) AS (
        SELECT id, name, parent_id, name FROM tags WHERE parent_id IS NULL
        UNION ALL
        SELECT t.id, t.name, t.parent_id, tree.path || ' / ' || t.name FROM tags t JOIN tree ON t.parent_id = tree.id
    )";

pub fn list(conn: &Connection) -> Result<Vec<Tag>> {
    let sql = format!(
        "{TAG_TREE_CTE} SELECT tree.id, tree.name, tree.parent_id, tree.path,
            (SELECT COUNT(*) FROM media_tags mt WHERE mt.tag_id = tree.id)
         FROM tree ORDER BY tree.path COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let v = stmt
        .query_map([], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, path: r.get(3)?, media_count: r.get(4)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn tags_for_media(conn: &Connection, media_id: i64) -> Result<Vec<Tag>> {
    let sql = format!(
        "{TAG_TREE_CTE} SELECT tree.id, tree.name, tree.parent_id, tree.path, 0
         FROM tree JOIN media_tags mt ON mt.tag_id = tree.id WHERE mt.media_id = ?1 ORDER BY tree.path COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let v = stmt
        .query_map([media_id], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, path: r.get(3)?, media_count: r.get(4)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Create a tag (or return the existing one with the same name under the same parent).
pub fn create(conn: &Connection, name: &str, parent_id: Option<i64>) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CuratorError::invalid("tag name cannot be empty"));
    }
    if name.contains('/') {
        return Err(CuratorError::invalid("tag names cannot contain '/'; use the parent selector for hierarchy"));
    }
    if let Some(id) = find(conn, name, parent_id)? {
        return Ok(id);
    }
    conn.execute("INSERT INTO tags (name, parent_id, created_at) VALUES (?1, ?2, ?3)", params![name, parent_id, now_ms()])?;
    Ok(conn.last_insert_rowid())
}

fn find(conn: &Connection, name: &str, parent_id: Option<i64>) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM tags WHERE COALESCE(parent_id, 0) = COALESCE(?2, 0) AND name = ?1 COLLATE NOCASE",
            params![name, parent_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Create (if needed) every level of a path like "Locations/Brazil/Rio de Janeiro"; returns the leaf id.
pub fn ensure_path(conn: &Connection, path: &str) -> Result<i64> {
    let mut parent = None;
    let mut last = None;
    for part in path.split('/').map(str::trim).filter(|p| !p.is_empty()) {
        let id = create(conn, part, parent)?;
        parent = Some(id);
        last = Some(id);
    }
    last.ok_or_else(|| CuratorError::invalid("empty tag path"))
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(CuratorError::invalid("invalid tag name"));
    }
    conn.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![id, name])?;
    refresh_tagged_fts(conn, id)
}

pub fn set_parent(conn: &Connection, id: i64, parent_id: Option<i64>) -> Result<()> {
    if let Some(p) = parent_id {
        // Refuse cycles: the new parent must not be the tag itself or one of its descendants.
        let is_desc: bool = conn.query_row(
            "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL SELECT t.id FROM tags t JOIN d ON t.parent_id = d.id)
             SELECT EXISTS(SELECT 1 FROM d WHERE id = ?2)",
            params![id, p],
            |r| r.get(0),
        )?;
        if is_desc {
            return Err(CuratorError::invalid("a tag cannot be moved inside itself"));
        }
    }
    conn.execute("UPDATE tags SET parent_id = ?2 WHERE id = ?1", params![id, parent_id])?;
    Ok(())
}

/// Delete a tag and its children. Only catalog associations are removed; files are untouched.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let affected: Vec<i64> = {
        let mut stmt = conn.prepare(
            "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL SELECT t.id FROM tags t JOIN d ON t.parent_id = d.id)
             SELECT DISTINCT media_id FROM media_tags WHERE tag_id IN (SELECT id FROM d)",
        )?;
        let v = stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        v
    };
    conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
    for m in affected {
        super::media::refresh_fts(conn, m)?;
    }
    Ok(())
}

pub fn assign(conn: &mut Connection, media_ids: &[i64], tag_id: i64) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for m in media_ids {
        n += tx.execute("INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)", params![m, tag_id])?;
        super::media::refresh_fts(&tx, *m)?;
    }
    tx.commit()?;
    Ok(n)
}

pub fn unassign(conn: &mut Connection, media_ids: &[i64], tag_id: i64) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for m in media_ids {
        n += tx.execute("DELETE FROM media_tags WHERE media_id = ?1 AND tag_id = ?2", params![m, tag_id])?;
        super::media::refresh_fts(&tx, *m)?;
    }
    tx.commit()?;
    Ok(n)
}

fn refresh_tagged_fts(conn: &Connection, tag_id: i64) -> Result<()> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT media_id FROM media_tags WHERE tag_id = ?1")?;
        let v = stmt.query_map([tag_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        v
    };
    for m in ids {
        super::media::refresh_fts(conn, m)?;
    }
    Ok(())
}
