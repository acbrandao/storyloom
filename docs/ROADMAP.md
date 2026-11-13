# Roadmap

| Phase | Scope | Status |
|---|---|---|
| 1 Foundation | Scanning, metadata, thumbnails, browser, viewer, ratings/statuses, tags, collections, dashboard, search/filter, path and timestamp tracking | **Done** |
| — | Verified, timestamp-preserving library import; operation journal; smart collections; exact duplicates | **Done (pulled forward)** |
| 2 File organization | Batch rename (templates, preview, conflicts, undo), physical folder organization (preview + confirm), operation history, batch tagging, rename on import | **Done**. XMP sidecars were skipped at the user's request |
| 3 Video extraction | Timeline with in/out handles, multiple segments, stream-copy vs. re-encode (clearly labelled), clip naming, capture-time writing, provenance | **Done** |
| 4 Story projects | Projects, storyboard with scenes/captions, project exports, CSV/JSON manifests, output presets | Next (schema ready) |
| 5 Advanced | Perceptual duplicates, local AI tagging/transcription (optional, offline), event grouping | Later |

Known limitations in this release:
- MKV/AVI and other formats WebKit can't decode open in the default player instead of inline, so clips can't be marked on them yet. That needs a playback proxy.
- Without ExifTool, Canon CR3 and some RAW formats have no capture date unless the filename contains one.
- Ratings, statuses, tags and notes live in the Media Curator catalog only. XMP sidecars are not implemented, so other apps (Lightroom, Bridge) won't see them.
- Moving files between different drives is intentionally not offered; use Copy.
- When a source records no time zone (e.g. GoPro), the clip's embedded `creation_time` uses the camera's own convention (local wall-clock time in the UTC field). Media Curator's catalog holds the exact value; some other apps may show it shifted.
