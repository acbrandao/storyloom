//! Locations of the catalog database, caches and logs. The desktop shell passes in the
//! OS-appropriate directories (e.g. `~/Library/Application Support/...` on macOS).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Configuration {
    /// Holds `catalog.sqlite` and `backups/`.
    pub data_dir: PathBuf,
    /// Holds `thumbnails/` and `previews/`. Safe to delete; regenerated on demand.
    pub cache_dir: PathBuf,
    /// Maximum concurrent metadata/thumbnail tasks.
    pub worker_concurrency: usize,
    /// Longest edge of grid thumbnails, in pixels.
    pub thumbnail_size: u32,
    /// Longest edge of generated full-screen previews (for formats the webview cannot show).
    pub preview_size: u32,
}

impl Configuration {
    pub fn new(data_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Self {
            data_dir: data_dir.into(),
            cache_dir: cache_dir.into(),
            worker_concurrency: cpus.clamp(2, 12),
            thumbnail_size: 512,
            preview_size: 2560,
        }
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("catalog.sqlite")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }
    pub fn thumbnails_dir(&self) -> PathBuf {
        self.cache_dir.join("thumbnails")
    }
    pub fn previews_dir(&self) -> PathBuf {
        self.cache_dir.join("previews")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [&self.data_dir, &self.cache_dir, &self.backups_dir(), &self.thumbnails_dir(), &self.previews_dir()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    /// Thumbnail cache path for a media id, sharded to keep directories small.
    pub fn thumbnail_path(&self, media_id: i64) -> PathBuf {
        shard(&self.thumbnails_dir(), media_id, "jpg")
    }
    pub fn preview_path(&self, media_id: i64) -> PathBuf {
        shard(&self.previews_dir(), media_id, "jpg")
    }
}

fn shard(base: &Path, id: i64, ext: &str) -> PathBuf {
    base.join(format!("{:03}", id % 1000)).join(format!("{id}.{ext}"))
}
