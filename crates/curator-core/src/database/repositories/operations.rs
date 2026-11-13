//! Journal of filesystem-modifying operations. Every copy/rename/move/export is recorded
//! *before* it starts (status 'pending') and updated when it finishes, so an interrupted
//! operation is always visible.

use crate::core::errors::Result;
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{self, params, Connection};
use serde::Serialize;

pub fn begin(conn: &Connection, batch_id: &str, op: &str, media_id: Option<i64>, original: &str, destination: Option<&str>, previous: &serde_json::Value) -> Result<i64> {
    conn.execute(
        "INSERT INTO file_operations (batch_id, operation_type, media_id, original_path, destination_path, previous_metadata, operation_status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)",
        params![batch_id, op, media_id, original, destination, previous.to_string(), now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn complete(conn: &Connection, id: i64, result: &serde_json::Value) -> Result<()> {
    conn.execute(
        "UPDATE file_operations SET operation_status = 'completed', result_metadata = ?2, completed_at = ?3 WHERE id = ?1",
        params![id, result.to_string(), now_ms()],
    )?;
    Ok(())
}

pub fn fail(conn: &Connection, id: i64, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE file_operations SET operation_status = 'failed', error_message = ?2, completed_at = ?3 WHERE id = ?1",
        params![id, error, now_ms()],
    )?;
    Ok(())
}

/// Operations left pending by a crash.
pub fn recover_interrupted(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE file_operations SET operation_status = 'failed', error_message = 'Interrupted (application closed); destination may be incomplete and was not registered'
         WHERE operation_status = 'pending'",
        [],
    )?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperation {
    pub id: i64,
    pub batch_id: Option<String>,
    pub operation_type: String,
    pub media_id: Option<i64>,
    pub original_path: String,
    pub destination_path: Option<String>,
    pub operation_status: String,
    pub error_message: Option<String>,
    pub result: Option<serde_json::Value>,
    pub created_at: i64,
}

pub fn recent(conn: &Connection, limit: i64) -> Result<Vec<FileOperation>> {
    let mut stmt = conn.prepare(
        "SELECT id, batch_id, operation_type, media_id, original_path, destination_path, operation_status, error_message, result_metadata, created_at
         FROM file_operations ORDER BY id DESC LIMIT ?1",
    )?;
    let v = stmt
        .query_map([limit], |r| {
            Ok(FileOperation {
                id: r.get(0)?,
                batch_id: r.get(1)?,
                operation_type: r.get(2)?,
                media_id: r.get(3)?,
                original_path: r.get(4)?,
                destination_path: r.get(5)?,
                operation_status: r.get(6)?,
                error_message: r.get(7)?,
                result: r.get::<_, Option<String>>(8)?.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
