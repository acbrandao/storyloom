# Changelog

## 0.3.1 — 2026-11-12

- The app is now called **StoryLoom** everywhere (window, menu bar, Dock, Launchpad, docs). The bundle identifier is now `app.storyloom.desktop`; an existing catalog from the old identifier is moved over automatically on first launch.
- Video dates are read correctly from files written by FFmpeg 9, which stores `creation_time` twice.
- Environment variables are renamed from `MEDIA_CURATOR_*` to `STORYLOOM_*`.

## 0.3.0 — 2026-11-12

First public release. Apple Silicon macOS build.

### Phase 1: foundation
- Read-only folder indexing (recursive, incremental, cancellable) and verified, date-preserving import into a managed library.
- Metadata from EXIF (built in), ExifTool (optional) and FFprobe. Handles the iPhone QuickTime local time/offset and the GoPro "local time in the UTC field" convention, with filename dates as a fallback. Filesystem dates are never treated as capture dates.
- Thumbnails (Rust, macOS ImageIO for HEIC/RAW, FFmpeg for video and audio artwork), a virtualized grid for 50k+ items, and a viewer for photos, video (range-streamed) and audio.
- Ratings, keep/reject, review, favorites, color labels, notes; hierarchical tags; manual and smart collections; full-text search and filters.
- Dashboard, exact-duplicate finder, activity log, daily catalog backups.

### Phase 2: file organization
- Batch rename with templates, preview, conflict handling, chain/swap safety and undo.
- Copy or move into folder structures, with preview, confirmation and undo (copies go to the Trash only if unmodified).
- Batch tag removal and renaming on import. Every change is journaled with verification.

### Phase 3: video clip extraction
- Clip editor with a filmstrip timeline, draggable in/out handles, I/O keys, typed timecodes and multiple clips per video.
- Lossless (keyframe-aligned), exact (frame-accurate re-encode) and most-compatible presets, with their trade-offs stated before export.
- Clip capture time = original capture + actual in-point, embedded in the file and stored in the catalog. The file-date policy, verification and permanent clip-to-source provenance are included.

XMP sidecar files were intentionally not implemented.
