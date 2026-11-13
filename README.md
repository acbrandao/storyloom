# StoryLoom

A local, offline desktop app for turning large piles of raw photos, videos and audio into organized, rated, searchable collections. These collections become the raw material for highlight videos, slideshows, blogs and social posts.

Rust core + Tauri 2 shell + React/TypeScript UI, with SQLite (WAL) as the catalog. The first target is macOS (Apple Silicon). The core is portable, with OS-specific code isolated in `crates/curator-core/src/platform/`.

## Download

Get the latest `.dmg` from the [Releases](../../releases) page (macOS 12+ on Apple Silicon).

1. Open the `.dmg` and drag **StoryLoom** to Applications.
2. Install FFmpeg, which the app uses for video and audio: `brew install ffmpeg`. ExifTool (`brew install exiftool`) is optional.
3. The app is not yet signed with an Apple Developer ID, so the first time macOS will say it can't verify the developer. Either right-click the app → **Open** → **Open**, or run:
   ```bash
   xattr -dr com.apple.quarantine "/Applications/StoryLoom.app"
   ```

## Status

**Phase 1 (functional foundation) is complete, plus parts of the timestamp-safe file layer:**

- Add folders (read-only indexing) with recursive, incremental, cancellable scans. Re-scans never duplicate records; missing files are flagged, never deleted.
- **Import into a managed library**: each copy is verified with BLAKE3, keeps its original creation and modification dates, never overwrites a file, and is journaled. You preview every destination first.
- Metadata from the built-in EXIF reader, or ExifTool if installed, plus FFprobe. Includes the iPhone QuickTime local-time/offset, GoPro "local time in UTC field" quirk, DJI, filename dates, GPS, exposure and audio tags.
- Separate original vs. user-corrected capture dates; filesystem dates are never promoted to capture dates.
- Thumbnails: Rust decoder for JPEG/PNG/TIFF, macOS ImageIO for HEIC/RAW, FFmpeg for video and audio artwork. Visible items get thumbnails first, on demand.
- Virtualized grid that handles 50k+ items. Large viewer for photos (zoom), video (range-streamed, frame-step, speed, fullscreen) and audio.
- Ratings, keep/reject, needs-review, favorites, color labels, notes, description, location. Everything has keyboard shortcuts and auto-advance culling.
- Hierarchical tags, manual collections, smart collections (saved filters), drag-and-drop onto tags and collections.
- Full-text search plus combinable filters: type, rating, status, label, tag, date range, camera, duration, GPS.
- Dashboard with counts, storage breakdown, ratings, timeline, cameras, library health and source folders.
- Exact-duplicate finder. Only same-size files are hashed, in the background.
- Activity log, file-operation journal with verification results, daily catalog backups and integrity checks.

**Phase 2 (file organization) is complete, except XMP sidecars, which were deliberately skipped:**

- **Batch rename** with templates (`{capture_date}_{capture_time}_{location}_{description}_{sequence}` and more), a live preview, conflict detection (existing files and duplicates within a batch, skip or add a number), safe handling of name chains and swaps, and **undo**. Files are never overwritten. Content, capture dates and both filesystem dates are preserved and verified, and the original name stays in the catalog.
- **Organize into folders**: copy (verified, dates preserved, ratings, tags and notes carried over to the copies) or move (same drive only, atomic, nothing deleted) into subfolders built from a template such as `{year}/{capture_date}`. Each operation shows a preview and needs confirmation, and can be undone. Undoing a copy moves the copies to the Trash, and only copies that are still unmodified.
- **Operation history** with undo (Activity & History), plus an Undo button on the completion notice.
- **Batch tagging**: add tags to many items, or remove them, with per-tag counts for the selection.
- **Optional renaming during import.**

**Phase 3 (video clip extraction) is complete:**

- **Clip editor** in the video viewer (button or C key): a filmstrip timeline with draggable in/out handles that scrub to the exact frame, I/O keys, typed timecodes, play-selection, and any number of clips per video. Clips are saved as drafts until exported.
- **Three export presets**, with what each does stated before export:
  - *Original quality*: FFmpeg stream copy. No re-encoding, but the cut snaps back to the previous keyframe; the actual start and the difference are shown.
  - *Exact cut*: frame-accurate re-encode in the source codec (H.264, or HEVC with 10-bit kept), audio copied when possible.
  - *Most compatible*: H.264 MP4, with a warning when HDR or 10-bit would be lost.
- **Clip capture time = original capture time + actual in-point.** The time zone and sub-second precision are kept, and a source with no capture date gives a clip with no capture date (the export date is never substituted). It is written into the clip (QuickTime `creationdate` with offset, plus UTC `creation_time`) and stored in the catalog.
- **File dates of clips** follow a policy: the source file's dates (default, per spec), the clip's capture time, or the export time.
- **Every export is verified**: duration, embedded capture time (read back), file dates and the source left unchanged. The results are journaled.
- **Automatic clip names** from templates (`{capture_date}_{capture_time}_{location}_{description}_{sequence}`), with the clip name feeding `{description}`.
- **Permanent provenance**: every exported clip, including repeat exports of the same range, links to its source and in/out points. The details panel shows "Clip of …" and the clips made from a video.

Phases 4–5 (story projects, storyboards, exports and manifests) are planned; see [docs/ROADMAP.md](docs/ROADMAP.md).

## Requirements (macOS)

| Tool | Needed for | Install |
|---|---|---|
| Rust ≥ 1.80 | building | https://rustup.rs |
| Node.js ≥ 20 | building the UI | `brew install node` |
| Xcode Command Line Tools | building | `xcode-select --install` |
| FFmpeg (ffmpeg + ffprobe) | video/audio details and thumbnails | `brew install ffmpeg` |
| ExifTool *(optional)* | richer photo metadata (CR3, maker notes, IPTC) | `brew install exiftool` |

The app finds tools in Homebrew/MacPorts locations even when launched from Finder. Override with `STORYLOOM_FFMPEG`, `STORYLOOM_FFPROBE` or `STORYLOOM_EXIFTOOL`.

## Build and run

```bash
npm install
npm run tauri dev                    # development (hot reload)
npm run tauri build                  # release .app and .dmg in target/release/bundle/
```

Use a separate, throwaway catalog for development or demos:

```bash
STORYLOOM_DATA_DIR=/tmp/mc-dev npm run tauri dev
```

## Sample library

```bash
npm run sample-library -- ~/Desktop/SampleMedia            # ~50 files: phones, GoPro, drone, DSLR, audio, duplicates, broken files
npm run sample-library -- ~/Desktop/SampleMedia --bulk 10000   # plus 10,000 small photos for load testing
```

Then open the app, click **Add Media → Add a folder** and choose that folder. You can also drag the folder onto the window.

## Tests

```bash
cargo test -p curator-core                                         # unit + integration (pipeline, timestamps, organize, clips)
cargo test -p curator-core --release --test scale -- --ignored --nocapture   # 10,000-file acceptance test
cargo test -p storyloom                                        # protocol (HTTP Range) tests
npm run typecheck
```

Reference results on an Apple Silicon Mac: 10,000 photos indexed (metadata + thumbnails) in about 24 s, an incremental rescan in 0.3 s, and the slowest catalog query during indexing under 100 ms.

## Releasing

Releases are built locally on a Mac and uploaded with the GitHub CLI:

```bash
npm run tauri build -- --target aarch64-apple-darwin
# The .app is in target/aarch64-apple-darwin/release/bundle/macos/. If the DMG step fails
# (it scripts Finder), make the DMG directly:
#   hdiutil create -volname "StoryLoom" -srcfolder <folder with the .app + Applications link> -format UDZO StoryLoom_X.Y.Z_aarch64.dmg
gh release create vX.Y.Z StoryLoom_X.Y.Z_aarch64.dmg --title "StoryLoom X.Y.Z" --notes-file notes.md
```

## Documentation

- [User guide](docs/USER_GUIDE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Timestamp preservation](docs/TIMESTAMPS.md)
- [Backup and recovery](docs/BACKUP_AND_RECOVERY.md)
- [Porting to Windows and Linux](docs/PORTING.md)
- [Development environment](docs/DEVELOPMENT.md)
- [Roadmap](docs/ROADMAP.md)
- [Changelog](CHANGELOG.md)

## License

[MIT](LICENSE)
