//! Persistence for background job state (so the UI can show history after restart).

use crate::core::errors::Result;
use crate::database::models::BackgroundJob;
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{self, params, Connection};

pub fn create(conn: &Connection, job_type: &str, payload: &serde_json::Value) -> Result<i64> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO background_jobs (job_type, status, payload_json, created_at, updated_at) VALUES (?1, 'queued', ?2, ?3, ?3)",
        params![job_type, payload.to_string(), now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_progress(conn: &Connection, id: i64, status: &str, phase: &str, done: u64, total: u64) -> Result<()> {
    let progress = if total == 0 { 0.0 } else { done as f64 / total as f64 };
    conn.execute(
        "UPDATE background_jobs SET status = ?2, phase = ?3, done = ?4, total = ?5, progress = ?6, updated_at = ?7 WHERE id = ?1",
        params![id, status, phase, done as i64, total as i64, progress, now_ms()],
    )?;
    Ok(())
}

pub fn finish(conn: &Connection, id: i64, status: &str, summary: Option<&serde_json::Value>, error: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE background_jobs SET status = ?2, summary_json = ?3, error_message = ?4, updated_at = ?5,
            progress = CASE WHEN ?2 = 'completed' THEN 1 ELSE progress END WHERE id = ?1",
        params![id, status, summary.map(|s| s.to_string()), error, now_ms()],
    )?;
    Ok(())
}

/// Jobs left "running" by a crash or forced quit are marked failed at startup.
pub fn recover_interrupted(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE background_jobs SET status = 'failed', error_message = 'Interrupted (application closed)', updated_at = ?1
         WHERE status IN ('queued','running')",
        [now_ms()],
    )?)
}

pub fn recent(conn: &Connection, limit: i64) -> Result<Vec<BackgroundJob>> {
    let mut stmt = conn.prepare(
        "SELECT id, job_type, status, phase, progress, done, total, summary_json, error_message, created_at, updated_at
         FROM background_jobs ORDER BY id DESC LIMIT ?1",
    )?;
    let v = stmt
        .query_map([limit], |r| {
            Ok(BackgroundJob {
                id: r.get(0)?,
                job_type: r.get(1)?,
                status: r.get(2)?,
                phase: r.get(3)?,
                progress: r.get(4)?,
                done: r.get(5)?,
                total: r.get(6)?,
                summary: r.get::<_, Option<String>>(7)?.and_then(|s| serde_json::from_str(&s).ok()),
                error_message: r.get(8)?,
                created_at: r.get(9)?,
                updated_at: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn add_issue(conn: &Connection, job_id: i64, path: &str, issue: &str, detail: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO scan_issues (job_id, path, issue, detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![job_id, path, issue, detail, now_ms()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub path: String,
    pub issue: String,
    pub detail: Option<String>,
    pub created_at: i64,
}

pub fn issues(conn: &Connection, job_id: i64) -> Result<Vec<ScanIssue>> {
    let mut stmt = conn.prepare("SELECT path, issue, detail, created_at FROM scan_issues WHERE job_id = ?1 ORDER BY id LIMIT 1000")?;
    let v = stmt
        .query_map([job_id], |r| Ok(ScanIssue { path: r.get(0)?, issue: r.get(1)?, detail: r.get(2)?, created_at: r.get(3)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
