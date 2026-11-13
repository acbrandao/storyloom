// Typed wrappers around Tauri commands. The UI never touches files directly:
// it sends ids and intents, and the Rust core performs (and validates) all work.
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type * as T from "./types";

export const api = {
  appInfo: () => invoke<T.AppInfo>("get_app_info"),
  addSource: (path: string) => invoke<number>("add_source", { path }),
  rescanSource: (id: number) => invoke<number>("rescan_source", { id }),
  removeSource: (id: number) => invoke<number>("remove_source", { id }),
  sources: () => invoke<T.Source[]>("list_sources"),
  cancelJob: (id: number) => invoke<boolean>("cancel_job", { id }),
  activeJobs: () => invoke<T.ActiveJob[]>("active_jobs"),
  recentJobs: (limit = 30) => invoke<T.BackgroundJob[]>("recent_jobs", { limit }),
  jobIssues: (id: number) => invoke<T.ScanIssue[]>("job_issues", { id }),
  reprocess: (ids: number[]) => invoke<number>("reprocess", { ids }),
  planIngest: (source: string, library: string, layout: T.IngestLayout, renameTemplate: string | null = null) => invoke<T.IngestPlan>("plan_ingest", { source, library, layout, renameTemplate }),
  planRename: (ids: number[], template: string, start: number, policy: T.ConflictPolicy) => invoke<T.RenamePlan>("plan_rename", { ids, template, start, policy }),
  startRename: (ids: number[], template: string, start: number, policy: T.ConflictPolicy) => invoke<[number, string]>("start_rename", { ids, template, start, policy }),
  planOrganize: (ids: number[], destination: string, subfolders: string | null, mode: T.OrganizeMode, policy: T.ConflictPolicy) => invoke<T.OrganizePlan>("plan_organize", { ids, destination, subfolders, mode, policy }),
  startOrganize: (ids: number[], destination: string, subfolders: string | null, mode: T.OrganizeMode, policy: T.ConflictPolicy) => invoke<[number, string]>("start_organize", { ids, destination, subfolders, mode, policy }),
  undoBatch: (batchId: string) => invoke<number>("undo_batch", { batchId }),
  operationBatches: (limit = 50) => invoke<T.BatchSummary[]>("operation_batches", { limit }),
  segments: (id: number) => invoke<T.Segment[]>("list_segments", { id }),
  createSegment: (id: number, inPoint: number, outPoint: number, name: string | null = null) => invoke<number>("create_segment", { id, inPoint, outPoint, name }),
  updateSegment: (id: number, inPoint: number, outPoint: number, name: string | null) => invoke<void>("update_segment", { id, inPoint, outPoint, name }),
  deleteSegment: (id: number) => invoke<void>("delete_segment", { id }),
  planClipExport: (request: T.ExportRequest) => invoke<T.ExportPlan>("plan_clip_export", { request }),
  startClipExport: (request: T.ExportRequest) => invoke<[number, string]>("start_clip_export", { request }),
  tagCounts: (ids: number[]) => invoke<[number, number][]>("tag_counts", { ids }),
  startIngest: (plan: T.IngestPlan) => invoke<number>("start_ingest", { plan }),
  fileOperations: (limit = 200) => invoke<T.FileOperation[]>("file_operations", { limit }),

  queryMedia: (query: T.MediaQuery) => invoke<T.MediaPage>("query_media", { query }),
  mediaIds: (filter: T.MediaFilter, sort: T.SortField, descending: boolean) => invoke<number[]>("media_ids", { filter, sort, descending }),
  mediaDetail: (id: number) => invoke<T.MediaDetail>("media_detail", { id }),
  metadataSnapshot: (id: number) => invoke<Record<string, unknown> | null>("metadata_snapshot", { id }),
  updateCuration: (ids: number[], update: T.CurationUpdate) => invoke<number>("update_curation", { ids, update }),
  setCaptureCorrection: (id: number, local: string | null, offset: string | null) => invoke<void>("set_capture_correction", { id, local, offset }),
  cameras: () => invoke<string[]>("cameras"),
  folders: () => invoke<[string, number][]>("folders"),
  duplicates: () => invoke<T.DuplicateGroup[]>("duplicates"),
  stats: () => invoke<T.LibraryStats>("stats"),
  revealInFinder: (id: number) => invoke<void>("reveal_in_finder", { id }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  openExternal: (id: number) => invoke<void>("open_external", { id }),

  tags: () => invoke<T.Tag[]>("list_tags"),
  createTag: (name: string, parentId: number | null = null) => invoke<number>("create_tag", { name, parentId }),
  renameTag: (id: number, name: string) => invoke<void>("rename_tag", { id, name }),
  moveTag: (id: number, parentId: number | null) => invoke<void>("move_tag", { id, parentId }),
  deleteTag: (id: number) => invoke<void>("delete_tag", { id }),
  assignTag: (ids: number[], tagId: number) => invoke<number>("assign_tag", { ids, tagId }),
  unassignTag: (ids: number[], tagId: number) => invoke<number>("unassign_tag", { ids, tagId }),

  collections: () => invoke<T.Collection[]>("list_collections"),
  createCollection: (name: string, description: string | null = null) => invoke<number>("create_collection", { name, description }),
  createSmartCollection: (name: string, rules: T.MediaFilter) => invoke<number>("create_smart_collection", { name, rules }),
  updateSmartCollection: (id: number, rules: T.MediaFilter) => invoke<void>("update_smart_collection", { id, rules }),
  renameCollection: (id: number, name: string) => invoke<void>("rename_collection", { id, name }),
  deleteCollection: (id: number) => invoke<void>("delete_collection", { id }),
  addToCollection: (id: number, ids: number[]) => invoke<number>("add_to_collection", { id, ids }),
  removeFromCollection: (id: number, ids: number[]) => invoke<number>("remove_from_collection", { id, ids }),

  backupDatabase: () => invoke<string>("backup_database"),
  listBackups: () => invoke<{ path: string; size: number; createdMs: number }[]>("list_backups"),
  integrityCheck: () => invoke<string>("integrity_check"),
  clearThumbnailCache: () => invoke<void>("clear_thumbnail_cache"),
  getSetting: (key: string) => invoke<string | null>("get_setting", { key }),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
};

/** URL of the cached thumbnail for a media id (generated on demand by the core). */
export const thumbUrl = (id: number, version: string | number = "") => `${convertFileSrc(String(id), "thumb")}?v=${version}`;
/** URL of the full-size displayable media (Range-capable, for video scrubbing). */
export const mediaUrl = (id: number) => convertFileSrc(String(id), "media");

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}

/** Timeline filmstrip for a video. */
export const filmstripUrl = (id: number) => `${convertFileSrc(String(id), "thumb")}/strip`;
