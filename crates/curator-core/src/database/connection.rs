use crate::core::errors::Result;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::rusqlite::{self, OpenFlags};
use r2d2_sqlite::SqliteConnectionManager;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub type DbConn = PooledConnection<SqliteConnectionManager>;

/// Pooled SQLite catalog. WAL mode allows the UI to read while background workers write.
#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Enable WAL once up front: switching journal mode needs an exclusive lock, so doing it
        // from every pooled connection at once makes them contend.
        {
            let c = rusqlite::Connection::open(path)?;
            c.busy_timeout(Duration::from_secs(10))?;
            c.pragma_update(None, "journal_mode", "WAL")?;
        }
        let manager = SqliteConnectionManager::file(path)
            .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .with_init(|c| {
                c.busy_timeout(Duration::from_secs(10))?;
                c.execute_batch(
                    "PRAGMA synchronous = NORMAL;
                     PRAGMA foreign_keys = ON;
                     PRAGMA temp_store = MEMORY;
                     PRAGMA cache_size = -32000;",
                )
            });
        let pool = Pool::builder().max_size(8).min_idle(Some(1)).build(manager)?;
        let db = Self { pool, path: path.to_path_buf() };
        {
            let mut conn = db.conn()?;
            super::migrations::run(&mut conn)?;
        }
        Ok(db)
    }

    pub fn conn(&self) -> Result<DbConn> {
        Ok(self.pool.get()?)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consistent online backup using SQLite's `VACUUM INTO` (safe while the app is running).
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            return Err(crate::CuratorError::Safety(format!("backup target already exists: {}", dest.display())));
        }
        let conn = self.conn()?;
        conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
        Ok(())
    }

    /// Runs SQLite's integrity check; returns "ok" when healthy.
    pub fn integrity_check(&self) -> Result<String> {
        let conn = self.conn()?;
        let r: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok(r)
    }
}

pub fn in_transaction<T>(conn: &mut rusqlite::Connection, f: impl FnOnce(&rusqlite::Transaction) -> Result<T>) -> Result<T> {
    let tx = conn.transaction()?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}
