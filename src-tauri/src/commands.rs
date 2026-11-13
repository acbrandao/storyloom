//! Tauri commands: thin wrappers that run core operations off the UI thread.
//! The webview only ever sends media *ids* and user intents; the core validates everything.

use crate::AppState;
use curator_core::database::models::*;
use curator_core::filesystem::organization::{OrganizeMode, OrganizePlan};
use curator_core::filesystem::renaming::{ConflictPolicy, RenamePlan};
use curator_core::media::importer::{IngestLayout, IngestPlan};
use curator_core::video::exporting::{ExportPlan, ExportRequest};
use curator_core::{Application, CuratorError};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

type R<T> = Result<T, CuratorError>;

/// Run a closure against the core on the blocking thread pool.
async fn blocking<T: Send + 'static>(state: State<'_, AppState>, f: impl FnOnce(Application) -> R<T> + Send + 'static) -> R<T> {
    let app = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || f(app)).await.map_err(|e| CuratorError::invalid(e.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    data_dir: String,
    cache_dir: String,
    database_path: String,
    tools: curator_core::tools::Tools,
    platform: String,
}

#[tauri::command]
async fn get_app_info(state: State<'_, AppState>) -> R<AppInfo> {
    let app = &state.0;
    let cfg = app.config();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: cfg.data_dir.to_string_lossy().into(),
        cache_dir: cfg.cache_dir.to_string_lossy().into(),
        database_path: cfg.database_path().to_string_lossy().into(),
        tools: app.tools().clone(),
        platform: std::env::consts::OS.into(),
    })
}

// ---------------------------------------------------------------- sources & jobs

#[tauri::command]
async fn add_source(state: State<'_, AppState>, path: String) -> R<i64> {
    blocking(state, move |a| a.add_source(&PathBuf::from(path))).await
}
#[tauri::command]
async fn rescan_source(state: State<'_, AppState>, id: i64) -> R<i64> {
    blocking(state, move |a| a.rescan_source(id)).await
}
#[tauri::command]
async fn remove_source(state: State<'_, AppState>, id: i64) -> R<usize> {
    blocking(state, move |a| a.remove_source(id)).await
}
#[tauri::command]
async fn list_sources(state: State<'_, AppState>) -> R<Vec<Source>> {
    blocking(state, |a| a.sources()).await
}
#[tauri::command]
async fn cancel_job(state: State<'_, AppState>, id: i64) -> R<bool> {
    Ok(state.0.cancel_job(id))
}
#[tauri::command]
async fn active_jobs(state: State<'_, AppState>) -> R<Vec<curator_core::workers::job_queue::ActiveJob>> {
    Ok(state.0.active_jobs())
}
#[tauri::command]
async fn recent_jobs(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<BackgroundJob>> {
    blocking(state, move |a| a.recent_jobs(limit.unwrap_or(30))).await
}
#[tauri::command]
async fn job_issues(state: State<'_, AppState>, id: i64) -> R<Vec<curator_core::database::repositories::jobs::ScanIssue>> {
    blocking(state, move |a| a.job_issues(id)).await
}
#[tauri::command]
async fn reprocess(state: State<'_, AppState>, ids: Vec<i64>) -> R<i64> {
    blocking(state, move |a| a.reprocess(&ids)).await
}

// ---------------------------------------------------------------- ingest

#[tauri::command]
async fn plan_ingest(state: State<'_, AppState>, source: String, library: String, layout: IngestLayout, rename_template: Option<String>) -> R<IngestPlan> {
    blocking(state, move |a| a.plan_ingest(&PathBuf::from(source), &PathBuf::from(library), layout, rename_template.as_deref())).await
}

// ---------------------------------------------------------------- rename / organize / undo

#[tauri::command]
async fn plan_rename(state: State<'_, AppState>, ids: Vec<i64>, template: String, start: u64, policy: ConflictPolicy) -> R<RenamePlan> {
    blocking(state, move |a| a.plan_rename(&ids, &template, start, policy)).await
}
#[tauri::command]
async fn start_rename(state: State<'_, AppState>, ids: Vec<i64>, template: String, start: u64, policy: ConflictPolicy) -> R<(i64, String)> {
    blocking(state, move |a| a.start_rename(&ids, &template, start, policy)).await
}
#[tauri::command]
async fn plan_organize(state: State<'_, AppState>, ids: Vec<i64>, destination: String, subfolders: Option<String>, mode: OrganizeMode, policy: ConflictPolicy) -> R<OrganizePlan> {
    blocking(state, move |a| a.plan_organize(&ids, &PathBuf::from(destination), subfolders.as_deref(), mode, policy)).await
}
#[tauri::command]
async fn start_organize(state: State<'_, AppState>, ids: Vec<i64>, destination: String, subfolders: Option<String>, mode: OrganizeMode, policy: ConflictPolicy) -> R<(i64, String)> {
    blocking(state, move |a| a.start_organize(&ids, &PathBuf::from(destination), subfolders.as_deref(), mode, policy)).await
}
#[tauri::command]
async fn undo_batch(state: State<'_, AppState>, batch_id: String) -> R<i64> {
    blocking(state, move |a| a.undo_batch(&batch_id)).await
}
#[tauri::command]
async fn operation_batches(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<curator_core::filesystem::transactions::BatchSummary>> {
    blocking(state, move |a| a.operation_batches(limit.unwrap_or(50))).await
}
#[tauri::command]
async fn list_segments(state: State<'_, AppState>, id: i64) -> R<Vec<curator_core::database::repositories::segments::Segment>> {
    blocking(state, move |a| a.segments(id)).await
}
#[tauri::command]
async fn create_segment(state: State<'_, AppState>, id: i64, in_point: f64, out_point: f64, name: Option<String>) -> R<i64> {
    blocking(state, move |a| a.create_segment(id, in_point, out_point, name.as_deref())).await
}
#[tauri::command]
async fn update_segment(state: State<'_, AppState>, id: i64, in_point: f64, out_point: f64, name: Option<String>) -> R<()> {
    blocking(state, move |a| a.update_segment(id, in_point, out_point, name.as_deref())).await
}
#[tauri::command]
async fn delete_segment(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state, move |a| a.delete_segment(id)).await
}
#[tauri::command]
async fn plan_clip_export(state: State<'_, AppState>, request: ExportRequest) -> R<ExportPlan> {
    blocking(state, move |a| a.plan_clip_export(&request)).await
}
#[tauri::command]
async fn start_clip_export(state: State<'_, AppState>, request: ExportRequest) -> R<(i64, String)> {
    blocking(state, move |a| a.start_clip_export(&request)).await
}

#[tauri::command]
async fn tag_counts(state: State<'_, AppState>, ids: Vec<i64>) -> R<Vec<(i64, i64)>> {
    blocking(state, move |a| a.tag_counts(&ids)).await
}
#[tauri::command]
async fn start_ingest(state: State<'_, AppState>, plan: IngestPlan) -> R<i64> {
    blocking(state, move |a| a.start_ingest(plan)).await
}
#[tauri::command]
async fn file_operations(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<curator_core::database::repositories::operations::FileOperation>> {
    blocking(state, move |a| a.file_operations(limit.unwrap_or(200))).await
}

// ---------------------------------------------------------------- media

#[tauri::command]
async fn query_media(state: State<'_, AppState>, query: MediaQuery) -> R<MediaPage> {
    blocking(state, move |a| a.query_media(&query)).await
}
#[tauri::command]
async fn media_ids(state: State<'_, AppState>, filter: MediaFilter, sort: SortField, descending: bool) -> R<Vec<i64>> {
    blocking(state, move |a| a.media_ids(&filter, sort, descending)).await
}
#[tauri::command]
async fn media_detail(state: State<'_, AppState>, id: i64) -> R<MediaDetail> {
    blocking(state, move |a| a.media_detail(id)).await
}
#[tauri::command]
async fn metadata_snapshot(state: State<'_, AppState>, id: i64) -> R<Option<serde_json::Value>> {
    blocking(state, move |a| a.metadata_snapshot(id)).await
}
#[tauri::command]
async fn update_curation(state: State<'_, AppState>, ids: Vec<i64>, update: CurationUpdate) -> R<usize> {
    blocking(state, move |a| a.update_curation(&ids, &update)).await
}
#[tauri::command]
async fn set_capture_correction(state: State<'_, AppState>, id: i64, local: Option<String>, offset: Option<String>) -> R<()> {
    blocking(state, move |a| a.set_capture_correction(id, local.as_deref().filter(|s| !s.is_empty()), offset.as_deref().filter(|s| !s.is_empty()))).await
}
#[tauri::command]
async fn cameras(state: State<'_, AppState>) -> R<Vec<String>> {
    blocking(state, |a| a.cameras()).await
}
#[tauri::command]
async fn folders(state: State<'_, AppState>) -> R<Vec<(String, i64)>> {
    blocking(state, |a| a.folders()).await
}
#[tauri::command]
async fn duplicates(state: State<'_, AppState>) -> R<Vec<curator_core::database::repositories::media::DuplicateGroup>> {
    blocking(state, |a| a.duplicates()).await
}
#[tauri::command]
async fn stats(state: State<'_, AppState>) -> R<curator_core::database::repositories::stats::LibraryStats> {
    blocking(state, |a| a.stats()).await
}
#[tauri::command]
async fn reveal_in_finder(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state, move |a| a.reveal_in_file_manager(id)).await
}
#[tauri::command]
async fn open_external(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state, move |a| a.open_with_default_app(id)).await
}

// ---------------------------------------------------------------- tags

#[tauri::command]
async fn list_tags(state: State<'_, AppState>) -> R<Vec<Tag>> {
    blocking(state, |a| a.tags()).await
}
#[tauri::command]
async fn create_tag(state: State<'_, AppState>, name: String, parent_id: Option<i64>) -> R<i64> {
    blocking(state, move |a| a.create_tag(&name, parent_id)).await
}
#[tauri::command]
async fn rename_tag(state: State<'_, AppState>, id: i64, name: String) -> R<()> {
    blocking(state, move |a| a.rename_tag(id, &name)).await
}
#[tauri::command]
async fn move_tag(state: State<'_, AppState>, id: i64, parent_id: Option<i64>) -> R<()> {
    blocking(state, move |a| a.move_tag(id, parent_id)).await
}
#[tauri::command]
async fn delete_tag(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state, move |a| a.delete_tag(id)).await
}
#[tauri::command]
async fn assign_tag(state: State<'_, AppState>, ids: Vec<i64>, tag_id: i64) -> R<usize> {
    blocking(state, move |a| a.assign_tag(&ids, tag_id)).await
}
#[tauri::command]
async fn unassign_tag(state: State<'_, AppState>, ids: Vec<i64>, tag_id: i64) -> R<usize> {
    blocking(state, move |a| a.unassign_tag(&ids, tag_id)).await
}

// ---------------------------------------------------------------- collections

#[tauri::command]
async fn list_collections(state: State<'_, AppState>) -> R<Vec<Collection>> {
    blocking(state, |a| a.collections()).await
}
#[tauri::command]
async fn create_collection(state: State<'_, AppState>, name: String, description: Option<String>) -> R<i64> {
    blocking(state, move |a| a.create_collection(&name, description.as_deref())).await
}
#[tauri::command]
async fn create_smart_collection(state: State<'_, AppState>, name: String, rules: MediaFilter) -> R<i64> {
    blocking(state, move |a| a.create_smart_collection(&name, &rules)).await
}
#[tauri::command]
async fn update_smart_collection(state: State<'_, AppState>, id: i64, rules: MediaFilter) -> R<()> {
    blocking(state, move |a| a.update_smart_collection(id, &rules)).await
}
#[tauri::command]
async fn rename_collection(state: State<'_, AppState>, id: i64, name: String) -> R<()> {
    blocking(state, move |a| a.rename_collection(id, &name)).await
}
#[tauri::command]
async fn delete_collection(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state, move |a| a.delete_collection(id)).await
}
#[tauri::command]
async fn add_to_collection(state: State<'_, AppState>, id: i64, ids: Vec<i64>) -> R<usize> {
    blocking(state, move |a| a.add_to_collection(id, &ids)).await
}
#[tauri::command]
async fn remove_from_collection(state: State<'_, AppState>, id: i64, ids: Vec<i64>) -> R<usize> {
    blocking(state, move |a| a.remove_from_collection(id, &ids)).await
}

// ---------------------------------------------------------------- maintenance

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    path: String,
    size: u64,
    created_ms: i64,
}

#[tauri::command]
async fn backup_database(state: State<'_, AppState>) -> R<String> {
    blocking(state, |a| a.backup_database().map(|p| p.to_string_lossy().to_string())).await
}
#[tauri::command]
async fn list_backups(state: State<'_, AppState>) -> R<Vec<BackupInfo>> {
    blocking(state, |a| {
        Ok(a.backups()?.into_iter().map(|(p, size, created_ms)| BackupInfo { path: p.to_string_lossy().into(), size, created_ms }).collect())
    })
    .await
}
#[tauri::command]
async fn integrity_check(state: State<'_, AppState>) -> R<String> {
    blocking(state, |a| a.integrity_check()).await
}
#[tauri::command]
async fn clear_thumbnail_cache(state: State<'_, AppState>) -> R<()> {
    blocking(state, |a| a.clear_thumbnail_cache()).await
}
#[tauri::command]
async fn get_setting(state: State<'_, AppState>, key: String) -> R<Option<String>> {
    blocking(state, move |a| a.get_setting(&key)).await
}
#[tauri::command]
async fn set_setting(state: State<'_, AppState>, key: String, value: String) -> R<()> {
    blocking(state, move |a| a.set_setting(&key, &value)).await
}
#[tauri::command]
async fn reveal_path(path: String) -> R<()> {
    curator_core::platform::reveal_in_file_manager(std::path::Path::new(&path))?;
    Ok(())
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        get_app_info,
        add_source,
        rescan_source,
        remove_source,
        list_sources,
        cancel_job,
        active_jobs,
        recent_jobs,
        job_issues,
        reprocess,
        plan_ingest,
        start_ingest,
        file_operations,
        plan_rename,
        start_rename,
        plan_organize,
        start_organize,
        undo_batch,
        operation_batches,
        tag_counts,
        list_segments,
        create_segment,
        update_segment,
        delete_segment,
        plan_clip_export,
        start_clip_export,
        query_media,
        media_ids,
        media_detail,
        metadata_snapshot,
        update_curation,
        set_capture_correction,
        cameras,
        folders,
        duplicates,
        stats,
        reveal_in_finder,
        open_external,
        list_tags,
        create_tag,
        rename_tag,
        move_tag,
        delete_tag,
        assign_tag,
        unassign_tag,
        list_collections,
        create_collection,
        create_smart_collection,
        update_smart_collection,
        rename_collection,
        delete_collection,
        add_to_collection,
        remove_from_collection,
        backup_database,
        list_backups,
        integrity_check,
        clear_thumbnail_cache,
        get_setting,
        set_setting,
        reveal_path,
    ]
}
