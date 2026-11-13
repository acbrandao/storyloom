# Timestamp preservation

StoryLoom tracks two families of time and never mixes them up.

| Kind | Where it lives | Example |
|---|---|---|
| **Capture time**: when the photo or video was taken | embedded metadata → `media.capture_datetime` + `capture_timezone` | `2026-07-14T15:32:45.250`, `-04:00` |
| **Filesystem times**: when this *file* was created or modified | the volume → `filesystem_creation_time`, `filesystem_modification_time` | unix ms |

Other dates are also kept separately: *date added* (`added_at`), *user correction* (`user_capture_datetime` / `user_capture_timezone`), and the immutable original extractor output (`metadata_snapshots`).

## How capture time is determined

Sources are tried in order. The winning source is recorded in `capture_time_source`.

1. Photos: EXIF `DateTimeOriginal` (+ `SubSecTimeOriginal`, `OffsetTimeOriginal`) → `DateTimeDigitized` → `DateTime`.
2. Video: `com.apple.quicktime.creationdate`, which is local time *with* offset (iPhone and many phones). Stored exactly as recorded.
3. Video: MP4/MOV `creation_time`, which is defined as UTC.
   - **GoPro** cameras write *local* time into this field. Detected by the GoPro handler or firmware tag, the value is kept as wall-clock time with **no offset** (`container_local`).
   - Otherwise it is an exact instant. It is expressed in this computer's time zone together with that offset (`container_utc`), so the instant stays exact.
4. Date and time in the filename (`IMG_20260714_153245`, `PXL_…123`, `Screenshot 2026-07-14 at 15.32.45`). Date-only names are ignored rather than inventing a time.
5. Otherwise the capture time is **unknown**. The item sorts provisionally by its file date, appears under *Library → No Capture Date*, and you can correct it from the details panel. The file date is never written into the capture-time columns.

The offset is never invented. Canon bodies, for example, record no offset, and that stays `NULL`.

## User corrections

*Adjust…* in the details panel writes only `user_capture_datetime` and `user_capture_timezone`. The extracted value and the file are untouched, and *Restore original* clears the correction.

## Copies (Import into library)

`filesystem::operations::safe_copy`:

1. Writes to a hidden temp file in the destination folder while hashing the source (BLAKE3).
2. Runs `fsync`, then re-hashes the copy. Any mismatch aborts and removes the temp file.
3. Applies the source's modification time, then its **birth time** (macOS `setattrlist(ATTR_CMN_CRTIME)`), and reads both back.
4. Places the file with a hard link, which is an atomic no-overwrite. On volumes without hard links (exFAT, SMB) it checks for an existing file, then renames.
5. Records a `TimestampReport` in the journal: requested vs. actual times, and explicit warnings if a volume cannot store a creation date. **Nothing is silently claimed.** The Activity view shows "✓ dates preserved" or the warning.

Tolerance is 2 s, which FAT volumes need.

## Renames and moves

`filesystem::transactions::execute_moves` renames or moves within one volume using a hard link followed by unlink. That is an atomic operation that never overwrites. Because the inode is the same, the birth and modification times are untouched by construction. If a volume has no hard links, it checks for an existing file and then renames. Timestamps are still read before and after, and the comparison is stored in the journal (`timestampsVerified`). Chains (A→B, B→C) are ordered automatically. Swaps (A↔B) are resolved by parking one file under a hidden temporary name, and the catalog follows each step. Moves between volumes are refused rather than done as copy-plus-delete. Undo applies the same no-overwrite move in reverse.

## Derived clips

```
New Capture Time = Original Capture Time + Clip In-Point
```

- **Which in-point:** the *actual* start of the exported file. For exact (re-encoded) exports that is the requested in-point. For lossless (stream-copy) exports it is the keyframe at or before the requested in-point, found with FFprobe and shown in the preview. A lossless clip of 00:04:30.5 starting on a keyframe at 00:04:30.0 is therefore timestamped from 00:04:30.0, which is when its first frame was filmed.
- **Arithmetic:** `video::timeline::clip_capture_time` keeps sub-second precision and the source's UTC offset, and crosses midnight and month boundaries. The effective source capture time is used, so a user correction wins.
- **Embedded metadata** (`video::exporting::clip_metadata`):
  - Offset known: `com.apple.quicktime.creationdate` = local time with offset, and `creation_time` = the exact UTC instant.
  - No offset (e.g. GoPro): `creation_time` = the wall-clock time in the camera's own convention, and no `creationdate`.
  - No source capture time: both fields are explicitly *cleared*, so the source's own date is not copied into the clip.
- **Catalog:** the clip's record stores the calculated value with `capture_time_source = 'derived_clip'`. Re-indexing never overwrites it.
- **Filesystem dates** are a separate policy (`FileDatePolicy`):
  - `sourceFile` (default, as specified): the original video file's creation and modification dates
  - `clipCapture`: the clip's capture instant
  - `exportTime`: leave the dates the new file was written with

  Requested and actual values are verified and journaled. Volumes that can't store a creation date produce a warning.
- **Verification after every export:** output duration (±0.15 s + 2 frames for exact cuts; ±0.5 s + 2 frames for lossless), embedded capture time read back with the same extractor used for indexing, file dates, and source timestamps unchanged. A clip that fails the duration check is deleted before it gets its final name.
- **Provenance:** `video_segments` rows keep the source id, requested and actual in/out points, method, preset, calculated capture time, output path, output media id and the verification record. Re-exporting a range creates a new row, so every version is traceable. The source cannot be removed from the catalog while clips reference it.

## Regression tests

- `crates/curator-core/tests/timestamps.rs`: birth-time support on this platform; `safe_copy` preserves content plus both timestamps and leaves the source byte- and time-identical; refuses to overwrite; ingest organizes by capture date (with *Unknown Date* for undated files), preserves timestamps, records provenance and the verified journal entry, and is idempotent; clip arithmetic, including the specification's example.
- `crates/curator-core/tests/pipeline.rs`: real files with known EXIF/QuickTime/GoPro/filename dates; time-zone handling; the "no capture date" case; corrections kept separate; source tree unchanged after indexing.
- `crates/curator-core/tests/clips.rs`: real FFmpeg exports from GoPro-style (no offset), iPhone-style (offset) and undated videos. Covers exact multi-segment export (durations, capture = source + in-point, embedded read-back, file dates = source file's, provenance, rescans), lossless keyframe alignment (capture follows the actual start), multiple versions of one clip, time-zone carry-over checked independently with FFprobe, the clip-capture file-date policy, no export date for undated sources, and segment validation.
- `crates/curator-core/tests/organize.rs`: batch rename with the specification's template (content, capture time, creation and modification times verified, original name kept, undo restores everything); conflicts never overwrite; chains, swaps and case-only renames; move and undo; copy with curation carried over, and undo that trashes only unmodified copies; renaming on import.
- Unit tests in `media/metadata.rs`, `media/timefmt.rs`, `video/timeline.rs`.

## Platform limits

| Platform | Modification time | Creation (birth) time |
|---|---|---|
| macOS (APFS, HFS+) | ✓ | ✓ via `setattrlist` |
| macOS → exFAT/FAT | ✓ (2 s resolution on FAT) | usually ✓; reported if refused |
| macOS → SMB/NFS | server-dependent | often refused → reported as a warning |
| Windows | ✓ | planned (`SetFileTime`) |
| Linux | ✓ | not settable by applications → reported |
