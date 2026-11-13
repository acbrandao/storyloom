//! `Application`: the facade the desktop shell talks to. Owns the database, configuration,
//! tool locations and background jobs. Cheap to clone (Arc inside).

use super::configuration::Configuration;
use super::errors::{CuratorError, Result};
use super::events::SharedSink;
use crate::database::models::*;
use crate::database::repositories::{self as repo, jobs, media as media_repo};
use crate::database::Database;
use crate::media::importer::{self, IngestLayout, IngestPlan};
use crate::media::thumbnails::{self, ThumbOutcome};
use crate::platform;
use crate::tools::Tools;
use crate::workers::job_queue::{ActiveJob, JobContext, JobManager};
use crate::filesystem::organization::{OrganizeMode, OrganizePlan};
use crate::filesystem::renaming::{self, ConflictPolicy, RenamePlan};
use crate::filesystem::transactions;
use crate::database::repositories::segments::{self, Segment};
use crate::video::exporting::{self, ExportPlan, ExportRequest};
use crate::workers::{clip_worker, index_worker, ingest_worker, organize_worker};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;

pub struct AppInner {
    pub cfg: Configuration,
    pub db: Database,
    pub tools: Arc<Tools>,
    pub sink: SharedSink,
    pub jobs: JobManager,
    /// Shared cap on CPU-heavy work across all jobs and on-demand requests.
    pub cpu_slots: Arc<Semaphore>,
    pub runtime: Handle,
}

#[derive(Clone)]
pub struct Application {
    inner: Arc<AppInner>,
}

impl Application {
    pub fn open(cfg: Configuration, sink: SharedSink, extra_tool_dirs: &[PathBuf], runtime: Handle) -> Result<Self> {
        cfg.ensure_dirs()?;
        let db = Database::open(&cfg.database_path())?;
        {
            let c = db.conn()?;
            let n = jobs::recover_interrupted(&c)?;
            let m = repo::operations::recover_interrupted(&c)?;
            if n + m > 0 {
                tracing::warn!(jobs = n, operations = m, "recovered work interrupted by a previous shutdown");
            }
        }
        let tools = Arc::new(Tools::discover(extra_tool_dirs));
        let slots = cfg.worker_concurrency + 2;
        Ok(Self {
            inner: Arc::new(AppInner { cfg, db, tools, sink, jobs: JobManager::default(), cpu_slots: Arc::new(Semaphore::new(slots)), runtime }),
        })
    }

    pub fn config(&self) -> &Configuration {
        &self.inner.cfg
    }
    pub fn tools(&self) -> &Tools {
        &self.inner.tools
    }
    pub fn database(&self) -> &Database {
        &self.inner.db
    }

    fn conn(&self) -> Result<crate::database::DbConn> {
        self.inner.db.conn()
    }

    // ------------------------------------------------------------------ sources & jobs

    /// Register a folder (read-only) and start indexing it. Returns the job id.
    pub fn add_source(&self, path: &Path) -> Result<i64> {
        if !path.is_dir() {
            return Err(CuratorError::NotFound(format!("folder not found: {}", path.display())));
        }
        let path = path.canonicalize()?;
        let cache = self.inner.cfg.cache_dir.canonicalize().unwrap_or_else(|_| self.inner.cfg.cache_dir.clone());
        if cache.starts_with(&path) {
            return Err(CuratorError::invalid("this folder contains Media Curator's own cache; choose a different folder"));
        }
        let id = {
            let c = self.conn()?;
            repo::sources::get_or_create(&c, &path, false)?
        };
        self.rescan_source(id)
    }

    pub fn rescan_source(&self, source_id: i64) -> Result<i64> {
        let job_type = format!("index:{source_id}");
        if self.inner.jobs.is_source_busy(&job_type) {
            return Err(CuratorError::invalid("this folder is already being scanned"));
        }
        if self.file_job_running() {
            return Err(CuratorError::invalid("files are being renamed or organized; scan again when that finishes"));
        }
        let name = {
            let c = self.conn()?;
            repo::sources::get(&c, source_id)?.name
        };
        let job_id = {
            let c = self.conn()?;
            jobs::create(&c, "index", &serde_json::json!({ "sourceId": source_id, "name": name }))?
        };
        let token = self.inner.jobs.register(job_id, &job_type);
        let ctx = Arc::new(JobContext::new(self.inner.clone(), job_id, &job_type, token));
        self.inner.runtime.spawn(async move {
            ctx.progress("Starting", 0, 0);
            match index_worker::run(ctx.clone(), source_id).await {
                Ok(s) => ctx.finish("completed", Some(index_worker::summary_json(&s)), None),
                Err(CuratorError::Cancelled) => ctx.finish("cancelled", None, None),
                Err(e) => {
                    tracing::error!(error = %e, source_id, "indexing failed");
                    ctx.finish("failed", None, Some(e.to_string()))
                }
            }
        });
        Ok(job_id)
    }

    /// Re-read metadata and thumbnails for items (e.g. after installing ExifTool) and process them.
    pub fn reprocess(&self, ids: &[i64]) -> Result<i64> {
        {
            let c = self.conn()?;
            index_worker::requeue(&c, ids)?;
            for id in ids {
                let _ = std::fs::remove_file(self.inner.cfg.thumbnail_path(*id));
                let _ = std::fs::remove_file(self.inner.cfg.preview_path(*id));
            }
        }
        let job_id = {
            let c = self.conn()?;
            jobs::create(&c, "reprocess", &serde_json::json!({ "count": ids.len() }))?
        };
        let token = self.inner.jobs.register(job_id, "reprocess");
        let ctx = Arc::new(JobContext::new(self.inner.clone(), job_id, "reprocess", token));
        self.inner.runtime.spawn(async move {
            let r = async {
                let (m, me) = index_worker::read_metadata(&ctx, None).await?;
                let (t, te) = index_worker::make_thumbnails(&ctx, None).await?;
                Ok::<_, CuratorError>(serde_json::json!({"metadataRead": m, "metadataErrors": me, "thumbnails": t, "thumbnailErrors": te}))
            }
            .await;
            match r {
                Ok(s) => ctx.finish("completed", Some(s), None),
                Err(CuratorError::Cancelled) => ctx.finish("cancelled", None, None),
                Err(e) => ctx.finish("failed", None, Some(e.to_string())),
            }
        });
        Ok(job_id)
    }

    pub fn remove_source(&self, source_id: i64) -> Result<usize> {
        if self.inner.jobs.is_source_busy(&format!("index:{source_id}")) {
            return Err(CuratorError::invalid("stop the running scan of this folder first"));
        }
        let ids: Vec<i64> = {
            let c = self.conn()?;
            let mut stmt = c.prepare("SELECT id FROM media WHERE source_id = ?1")?;
            let v = stmt.query_map([source_id], |r| r.get(0))?.collect::<std::result::Result<Vec<i64>, _>>()?;
            v
        };
        let mut c = self.conn()?;
        let n = repo::sources::remove(&mut c, source_id)?;
        for id in ids {
            let _ = std::fs::remove_file(self.inner.cfg.thumbnail_path(id));
            let _ = std::fs::remove_file(self.inner.cfg.preview_path(id));
        }
        Ok(n)
    }

    pub fn sources(&self) -> Result<Vec<Source>> {
        repo::sources::list(&*self.conn()?)
    }

    pub fn cancel_job(&self, id: i64) -> bool {
        self.inner.jobs.cancel(id)
    }
    pub fn cancel_all_jobs(&self) {
        self.inner.jobs.cancel_all()
    }
    pub fn active_jobs(&self) -> Vec<ActiveJob> {
        self.inner.jobs.list()
    }
    pub fn recent_jobs(&self, limit: i64) -> Result<Vec<BackgroundJob>> {
        jobs::recent(&*self.conn()?, limit)
    }
    pub fn job_issues(&self, job_id: i64) -> Result<Vec<jobs::ScanIssue>> {
        jobs::issues(&*self.conn()?, job_id)
    }

    // ------------------------------------------------------------------ ingest

    pub fn plan_ingest(&self, source: &Path, library: &Path, layout: IngestLayout, rename_template: Option<&str>) -> Result<IngestPlan> {
        importer::plan(source, library, layout, rename_template, &self.inner.tools, &tokio_util::sync::CancellationToken::new())
    }

    pub fn start_ingest(&self, plan: IngestPlan) -> Result<i64> {
        let job_id = {
            let c = self.conn()?;
            jobs::create(&c, "ingest", &serde_json::json!({"source": plan.source_dir, "library": plan.library_dir, "files": plan.items.len()}))?
        };
        let token = self.inner.jobs.register(job_id, "ingest");
        let ctx = Arc::new(JobContext::new(self.inner.clone(), job_id, "ingest", token));
        self.inner.runtime.spawn(async move {
            match ingest_worker::run(ctx.clone(), plan).await {
                Ok(s) => ctx.finish("completed", serde_json::to_value(&s).ok(), None),
                Err(CuratorError::Cancelled) => ctx.finish("cancelled", None, None),
                Err(e) => ctx.finish("failed", None, Some(e.to_string())),
            }
        });
        Ok(job_id)
    }

    fn file_job_running(&self) -> bool {
        self.inner.jobs.list().iter().any(|j| matches!(j.job_type.as_str(), "rename" | "organize" | "undo" | "clip-export"))
    }

    // ------------------------------------------------------------------ video clips

    pub fn segments(&self, source_media_id: i64) -> Result<Vec<Segment>> {
        segments::for_source(&*self.conn()?, source_media_id)
    }
    pub fn create_segment(&self, source_media_id: i64, in_point: f64, out_point: f64, name: Option<&str>) -> Result<i64> {
        segments::create(&*self.conn()?, source_media_id, in_point, out_point, name)
    }
    pub fn update_segment(&self, id: i64, in_point: f64, out_point: f64, name: Option<&str>) -> Result<()> {
        segments::update(&*self.conn()?, id, in_point, out_point, name)
    }
    pub fn delete_segment(&self, id: i64) -> Result<()> {
        segments::delete(&*self.conn()?, id)
    }

    /// Preview a clip export (names, keyframe-aligned starts, capture times, conflicts).
    pub fn plan_clip_export(&self, req: &ExportRequest) -> Result<ExportPlan> {
        let protected = [self.inner.cfg.data_dir.clone(), self.inner.cfg.cache_dir.clone()];
        exporting::plan(&*self.conn()?, &self.inner.tools, req, &protected)
    }

    /// Re-plan (so disk state is checked again) and export in the background. Returns (job, batch).
    pub fn start_clip_export(&self, req: &ExportRequest) -> Result<(i64, String)> {
        self.ensure_idle_for_file_ops()?;
        let plan = self.plan_clip_export(req)?;
        if plan.ready == 0 {
            return Err(CuratorError::invalid("there are no clips ready to export"));
        }
        let batch = renaming::batch_id("clips");
        let b2 = batch.clone();
        let job = self.spawn_job("clip-export", serde_json::json!({ "batch": batch, "count": plan.ready }), move |ctx| clip_worker::export(ctx, plan, b2))?;
        Ok((job, batch))
    }

    /// Timeline strip image for a video (cached).
    pub fn filmstrip(&self, id: i64) -> Result<PathBuf> {
        let loc = media_repo::location(&*self.conn()?, id)?;
        crate::video::filmstrip::filmstrip(&loc, &self.inner.tools, &self.inner.cfg)
    }

    fn ensure_idle_for_file_ops(&self) -> Result<()> {
        if !self.inner.jobs.list().is_empty() {
            return Err(CuratorError::invalid("wait for the running background task to finish (or cancel it) before changing files on disk"));
        }
        Ok(())
    }

    /// Spawn a job; `work` returns a serializable summary.
    fn spawn_job<T, F, Fut>(&self, job_type: &str, payload: serde_json::Value, work: F) -> Result<i64>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(Arc<JobContext>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + Send + 'static,
    {
        let job_id = jobs::create(&*self.conn()?, job_type, &payload)?;
        let token = self.inner.jobs.register(job_id, job_type);
        let ctx = Arc::new(JobContext::new(self.inner.clone(), job_id, job_type, token));
        self.inner.runtime.spawn(async move {
            ctx.progress("Starting", 0, 0);
            match work(ctx.clone()).await {
                Ok(s) => ctx.finish("completed", serde_json::to_value(&s).ok(), None),
                Err(CuratorError::Cancelled) => ctx.finish("cancelled", None, None),
                Err(e) => ctx.finish("failed", None, Some(e.to_string())),
            }
        });
        Ok(job_id)
    }

    // ------------------------------------------------------------------ rename / organize / undo

    /// Preview a batch rename (reads only).
    pub fn plan_rename(&self, ids: &[i64], template: &str, start: u64, policy: ConflictPolicy) -> Result<RenamePlan> {
        renaming::plan_rename(&*self.conn()?, ids, template, start, policy)
    }

    /// Re-plan and run a rename. The plan is recomputed so the disk state is checked again
    /// right before anything changes. Returns (job id, batch id).
    pub fn start_rename(&self, ids: &[i64], template: &str, start: u64, policy: ConflictPolicy) -> Result<(i64, String)> {
        self.ensure_idle_for_file_ops()?;
        let plan = self.plan_rename(ids, template, start, policy)?;
        let batch = renaming::batch_id("rename");
        let b2 = batch.clone();
        let job = self.spawn_job("rename", serde_json::json!({ "batch": batch, "count": plan.ready }), move |ctx| organize_worker::rename(ctx, plan, b2))?;
        Ok((job, batch))
    }

    pub fn plan_organize(&self, ids: &[i64], destination: &Path, subfolders: Option<&str>, mode: OrganizeMode, policy: ConflictPolicy) -> Result<OrganizePlan> {
        let protected = [self.inner.cfg.data_dir.clone(), self.inner.cfg.cache_dir.clone()];
        crate::filesystem::organization::plan(&*self.conn()?, ids, destination, subfolders, mode, policy, &protected)
    }

    pub fn start_organize(&self, ids: &[i64], destination: &Path, subfolders: Option<&str>, mode: OrganizeMode, policy: ConflictPolicy) -> Result<(i64, String)> {
        self.ensure_idle_for_file_ops()?;
        let plan = self.plan_organize(ids, destination, subfolders, mode, policy)?;
        let batch = renaming::batch_id(if mode == OrganizeMode::Move { "move" } else { "copy" });
        let b2 = batch.clone();
        let job = self.spawn_job("organize", serde_json::json!({ "batch": batch, "count": plan.ready, "mode": mode }), move |ctx| organize_worker::organize(ctx, plan, b2))?;
        Ok((job, batch))
    }

    pub fn undo_batch(&self, batch_id: &str) -> Result<i64> {
        self.ensure_idle_for_file_ops()?;
        let b = batch_id.to_string();
        self.spawn_job("undo", serde_json::json!({ "batch": batch_id }), move |ctx| organize_worker::undo(ctx, b))
    }

    pub fn operation_batches(&self, limit: i64) -> Result<Vec<transactions::BatchSummary>> {
        transactions::batches(&*self.conn()?, limit)
    }

    pub fn tag_counts(&self, ids: &[i64]) -> Result<Vec<(i64, i64)>> {
        media_repo::tag_counts(&*self.conn()?, ids)
    }

    pub fn file_operations(&self, limit: i64) -> Result<Vec<repo::operations::FileOperation>> {
        repo::operations::recent(&*self.conn()?, limit)
    }

    // ------------------------------------------------------------------ media

    pub fn query_media(&self, q: &MediaQuery) -> Result<MediaPage> {
        media_repo::query(&*self.conn()?, q)
    }
    pub fn media_ids(&self, filter: &MediaFilter, sort: SortField, descending: bool) -> Result<Vec<i64>> {
        media_repo::query_ids(&*self.conn()?, filter, sort, descending)
    }
    pub fn media_detail(&self, id: i64) -> Result<MediaDetail> {
        media_repo::detail(&*self.conn()?, id)
    }
    pub fn metadata_snapshot(&self, id: i64) -> Result<Option<serde_json::Value>> {
        media_repo::metadata_snapshot(&*self.conn()?, id)
    }
    pub fn update_curation(&self, ids: &[i64], u: &CurationUpdate) -> Result<usize> {
        media_repo::update_curation(&mut *self.conn()?, ids, u)
    }
    pub fn set_capture_correction(&self, id: i64, local: Option<&str>, offset: Option<&str>) -> Result<()> {
        media_repo::set_capture_correction(&*self.conn()?, id, local, offset)
    }
    pub fn cameras(&self) -> Result<Vec<String>> {
        media_repo::cameras(&*self.conn()?)
    }
    pub fn folders(&self) -> Result<Vec<(String, i64)>> {
        media_repo::folders(&*self.conn()?)
    }
    pub fn duplicates(&self) -> Result<Vec<media_repo::DuplicateGroup>> {
        media_repo::duplicate_groups(&*self.conn()?)
    }
    pub fn stats(&self) -> Result<repo::stats::LibraryStats> {
        repo::stats::library_stats(&*self.conn()?)
    }

    /// Path of the cached thumbnail, generating it now if needed (called by the UI's image loader,
    /// so on-screen items are prioritised over the background queue).
    pub fn thumbnail(&self, id: i64) -> Result<Option<PathBuf>> {
        let path = self.inner.cfg.thumbnail_path(id);
        if path.exists() {
            return Ok(Some(path));
        }
        let loc = media_repo::location(&*self.conn()?, id)?;
        let status = match thumbnails::generate_thumbnail(&loc, &self.inner.tools, &self.inner.cfg) {
            Ok(ThumbOutcome::Created) => "done",
            Ok(ThumbOutcome::Unsupported) => "unsupported",
            Err(CuratorError::NotFound(_)) => return Ok(None),
            Err(e) => {
                tracing::debug!(id, error = %e, "on-demand thumbnail failed");
                "error"
            }
        };
        media_repo::set_thumbnail_status(&*self.conn()?, id, status)?;
        Ok(if status == "done" { Some(path) } else { None })
    }

    /// A file the webview can display for the large viewer (original or generated preview).
    pub fn display_file(&self, id: i64) -> Result<PathBuf> {
        let loc = media_repo::location(&*self.conn()?, id)?;
        match loc.media_type {
            MediaType::Photo => thumbnails::display_image_path(&loc, &self.inner.tools, &self.inner.cfg),
            _ => Ok(PathBuf::from(loc.path)),
        }
    }

    pub fn reveal_in_file_manager(&self, id: i64) -> Result<()> {
        let loc = media_repo::location(&*self.conn()?, id)?;
        platform::reveal_in_file_manager(Path::new(&loc.path))?;
        Ok(())
    }
    pub fn open_with_default_app(&self, id: i64) -> Result<()> {
        let loc = media_repo::location(&*self.conn()?, id)?;
        platform::open_with_default_app(Path::new(&loc.path))?;
        Ok(())
    }

    // ------------------------------------------------------------------ tags

    pub fn tags(&self) -> Result<Vec<Tag>> {
        repo::tags::list(&*self.conn()?)
    }
    /// `name` may be a path ("Locations/Brazil/Rio") to create a hierarchy in one step.
    pub fn create_tag(&self, name: &str, parent_id: Option<i64>) -> Result<i64> {
        let c = self.conn()?;
        if parent_id.is_none() && name.contains('/') {
            repo::tags::ensure_path(&c, name)
        } else {
            repo::tags::create(&c, name, parent_id)
        }
    }
    pub fn rename_tag(&self, id: i64, name: &str) -> Result<()> {
        repo::tags::rename(&*self.conn()?, id, name)
    }
    pub fn move_tag(&self, id: i64, parent_id: Option<i64>) -> Result<()> {
        repo::tags::set_parent(&*self.conn()?, id, parent_id)
    }
    pub fn delete_tag(&self, id: i64) -> Result<()> {
        repo::tags::delete(&*self.conn()?, id)
    }
    pub fn assign_tag(&self, ids: &[i64], tag_id: i64) -> Result<usize> {
        repo::tags::assign(&mut *self.conn()?, ids, tag_id)
    }
    pub fn unassign_tag(&self, ids: &[i64], tag_id: i64) -> Result<usize> {
        repo::tags::unassign(&mut *self.conn()?, ids, tag_id)
    }

    // ------------------------------------------------------------------ collections

    pub fn collections(&self) -> Result<Vec<Collection>> {
        repo::collections::list(&*self.conn()?)
    }
    pub fn create_collection(&self, name: &str, description: Option<&str>) -> Result<i64> {
        repo::collections::create_manual(&*self.conn()?, name, description)
    }
    pub fn create_smart_collection(&self, name: &str, rules: &MediaFilter) -> Result<i64> {
        repo::collections::create_smart(&*self.conn()?, name, rules)
    }
    pub fn update_smart_collection(&self, id: i64, rules: &MediaFilter) -> Result<()> {
        repo::collections::update_rules(&*self.conn()?, id, rules)
    }
    pub fn rename_collection(&self, id: i64, name: &str) -> Result<()> {
        repo::collections::rename(&*self.conn()?, id, name)
    }
    pub fn delete_collection(&self, id: i64) -> Result<()> {
        repo::collections::delete(&*self.conn()?, id)
    }
    pub fn add_to_collection(&self, id: i64, ids: &[i64]) -> Result<usize> {
        repo::collections::add_media(&mut *self.conn()?, id, ids)
    }
    pub fn remove_from_collection(&self, id: i64, ids: &[i64]) -> Result<usize> {
        repo::collections::remove_media(&mut *self.conn()?, id, ids)
    }

    // ------------------------------------------------------------------ maintenance

    /// Write a consistent copy of the catalog to `backups/`. Keeps the 20 most recent.
    pub fn backup_database(&self) -> Result<PathBuf> {
        let dir = self.inner.cfg.backups_dir();
        std::fs::create_dir_all(&dir)?;
        let name = format!("catalog-{}.sqlite", chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let dest = dir.join(name);
        self.inner.db.backup_to(&dest)?;
        let mut backups = self.backups()?;
        backups.sort_by(|a, b| b.0.cmp(&a.0));
        for (p, _, _) in backups.iter().skip(20) {
            let _ = std::fs::remove_file(p);
        }
        Ok(dest)
    }

    /// (path, size, modified ms)
    pub fn backups(&self) -> Result<Vec<(PathBuf, u64, i64)>> {
        let mut v = vec![];
        for e in std::fs::read_dir(self.inner.cfg.backups_dir())? {
            let e = e?;
            if e.path().extension().is_some_and(|x| x == "sqlite") {
                let m = e.metadata()?;
                v.push((e.path(), m.len(), crate::filesystem::timestamps::from_metadata(&m).modified_ms));
            }
        }
        v.sort_by(|a, b| b.2.cmp(&a.2));
        Ok(v)
    }

    /// Backup at most once per day (called at startup).
    pub fn auto_backup(&self) -> Result<Option<PathBuf>> {
        let newest = self.backups()?.first().map(|b| b.2).unwrap_or(0);
        if crate::database::now_ms() - newest > 24 * 3600 * 1000 {
            let has_media: i64 = self.conn()?.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))?;
            if has_media > 0 {
                return self.backup_database().map(Some);
            }
        }
        Ok(None)
    }

    pub fn integrity_check(&self) -> Result<String> {
        self.inner.db.integrity_check()
    }

    pub fn clear_thumbnail_cache(&self) -> Result<()> {
        let dir = self.inner.cfg.thumbnails_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        self.conn()?.execute("UPDATE media SET thumbnail_status = 'pending' WHERE thumbnail_status <> 'unsupported'", [])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        use r2d2_sqlite::rusqlite::OptionalExtension;
        Ok(self.conn()?.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn inner(&self) -> &Arc<AppInner> {
        &self.inner
    }
}
