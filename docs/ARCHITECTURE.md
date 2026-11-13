# Architecture

```
┌──────────────────────────── Tauri window (WKWebView on macOS) ───────────────────────────┐
│ React + TypeScript UI (src/)                                                               │
│   Sidebar · Toolbar/FilterBar · virtualized MediaGrid · Inspector · Viewer · Dashboard     │
│        │ invoke(ids, intents)            ▲ "core-event" (job progress, library changed)    │
│        │ <img src="thumb://localhost/42">  <video src="media://localhost/42"> (Range)      │
└────────┼──────────────────────────────────┼───────────────────────────────────────────────┘
         ▼                                  │
┌──────── src-tauri (thin shell) ───────────┴──────────────────────────────────────────────┐
│ commands.rs  – one command per core operation, executed on the blocking pool              │
│ protocols.rs – thumb:// (on-demand thumbnails), media:// (Range streaming), ids only      │
│ lib.rs       – app dirs, logging (tracing → daily log file), event sink, daily backup     │
└────────┬─────────────────────────────────────────────────────────────────────────────────┘
         ▼
┌──────── crates/curator-core (no GUI dependencies) ───────────────────────────────────────┐
│ core/       Application facade, Configuration, CuratorError, CoreEvent/EventSink          │
│ database/   r2d2 pool (WAL), migrations (PRAGMA user_version), models, repositories       │
│ media/      media_types, scanner, metadata, thumbnails, fingerprints, importer, timefmt   │
│ filesystem/ timestamps (read/apply/verify), operations (verified no-clobber safe_copy)    │
│ video/      probe (ffprobe, timeouts), timeline (clip capture-time arithmetic)            │
│ workers/    job_queue (progress, cancellation), index_worker, ingest_worker               │
│ platform/   macOS setattrlist birth time, sips rendering, Finder reveal (stubs elsewhere) │
│ tools.rs    ffmpeg/ffprobe/exiftool discovery                                             │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

## Principles enforced by the design

- **The webview never sees or sends file paths for media.** It sends catalog ids. Thumbnails and media come through custom protocols that resolve ids in the catalog, so the UI cannot read arbitrary files or bypass the core.
- **Indexing is read-only.** The scanner opens files only for reading. Thumbnails and previews go to the app cache. `tests/pipeline.rs` hashes and timestamps every sample file before and after indexing and asserts that nothing changed.
- **Everything that writes to disk is journaled** in `file_operations` *before* it starts. On startup, interrupted operations are marked failed.
- **The core is UI-independent.** `curator-core` has no Tauri dependency, and every test runs headless.

## Indexing pipeline (`workers/index_worker.rs`)

1. **Discover**: walk the folder on the blocking pool and upsert in batches of 500 inside transactions. Change detection uses size + mtime, and the unique `current_path` prevents duplicate records. Hidden, AppleDouble and NAS index folders are skipped. Unsupported, empty and inaccessible files are recorded in `scan_issues` without stopping the job.
2. **Metadata**: bounded parallelism (`worker_concurrency`, about one per CPU core, max 12), sharing a global CPU semaphore with on-demand thumbnails. ExifTool runs in batches of 40 files per process. Results go through a single writer in batched transactions.
3. **Thumbnails**: same bounded pool. The UI also requests visible thumbnails on demand through `thumb://`, so what's on screen comes first.
4. **Duplicates**: only files sharing a size with another file are hashed (BLAKE3), two at a time.

Progress events are throttled to about 8/s for the UI and 1/s for the database. "Library changed" events are throttled to about 1/s, and the UI refetches only the pages currently loaded.

## Catalog (`database/migrations/001_initial.sql`)

The tables cover media, tags, collections, projects, video segments, file operations, metadata snapshots and background jobs, all with foreign keys and indexes. Highlights:

- Capture time is stored as **local wall-clock text plus an optional UTC offset**, separate from the user correction columns. `sort_datetime` is the effective value used for sorting.
- Filesystem birth and modification times are stored separately, as UTC milliseconds.
- `metadata_snapshots` keeps the immutable, complete extractor output from first read.
- `video_segments.source_media_id` is `ON DELETE RESTRICT`, so clip provenance cannot be lost.
- `media_fts` (FTS5) indexes filenames, folders, notes, descriptions, camera, tags and audio tags.

Migrations are forward-only SQL files tracked with `PRAGMA user_version`.

## Frontend

- `store.ts`: a tiny `useSyncExternalStore` store holding view, filters, selection and UI preferences.
- `hooks/mediaList.ts`: a sparse paged result set (240 rows per page) shared by the grid and the viewer. It refreshes loaded pages in place when the catalog changes.
- `MediaGrid.tsx`: row virtualization with `@tanstack/react-virtual`. Only visible rows exist in the DOM.
