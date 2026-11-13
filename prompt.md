# Project: Media Curator — Personal Media Curation & Asset Management System

## 1. Role and Project Objective

Act as a senior Rust software architect, desktop application developer, media-processing engineer, and UI/UX designer.

Design and develop a modern, high-performance, portable desktop application called **Media Curator**.

The primary purpose of this application is to help a person curate large collections of raw media into organized, meaningful collections that can later be used to tell a story through:

- Highlight videos and travel videos.
- Photo slideshows.
- Personal blogs and articles.
- Instagram and other social media posts.
- Travel journals and event documentation.
- Personal archives and creative projects.

This is not primarily a video editor, photo editor, or traditional file browser.

It is a **media discovery, selection, organization, and storytelling preparation tool**.

The application should make it exceptionally easy to review large quantities of photos, videos, and audio, identify the best material, assign ratings, organize everything, extract useful video segments, and create curated collections.

It should be a high performant application ,so scrubbing through videos, or reviewing large photos happpens quickly

The first release will target macOS, but the underlying architecture must support future Windows and Linux releases without requiring a major rewrite.

Do not build a web application. This should be a locally installed desktop application.

---

# 2. Technology and Architecture

Use Rust as the primary application and backend development language.

Recommended architecture:

| Component | Technology |
|---|---|
| Application core | Rust |
| Desktop framework | Tauri 2 |
| User interface | TypeScript + React |
| Frontend build | Vite |
| Database | SQLite |
| Database access | SQLx or rusqlite |
| Media metadata | ExifTool |
| Video inspection | FFprobe |
| Video processing | FFmpeg |
| Image thumbnails | libvips or an appropriately maintained Rust image library |
| Audio playback | Native-compatible Rust audio library |
| Logging | tracing |
| Async processing | Tokio |
| Serialization | Serde |

The Rust core must own all important functionality, including:

- Filesystem scanning and operations.
- Database access and management.
- Metadata extraction.
- Thumbnail generation and caching.
- Video inspection and trimming.
- File renaming and organization.
- Export operations.
- Timestamp preservation.
- Background processing.

The frontend must never directly manipulate source media or bypass the Rust application's file-operation safeguards.

Use a modular architecture with well-defined interfaces.

Suggested Rust modules:

```text
src-tauri/src/
    main.rs
    lib.rs

    core/
        application.rs
        configuration.rs
        errors.rs
        events.rs

    database/
        connection.rs
        migrations/
        models/
        repositories/

    media/
        scanner.rs
        importer.rs
        metadata.rs
        thumbnails.rs
        fingerprints.rs
        duplicates.rs
        media_types.rs

    filesystem/
        operations.rs
        renaming.rs
        organization.rs
        timestamps.rs
        transactions.rs

    video/
        probe.rs
        playback.rs
        trimming.rs
        exporting.rs

    audio/
        metadata.rs
        playback.rs

    collections/
        projects.rs
        albums.rs
        storyboards.rs

    export/
        export_manager.rs
        manifests.rs

    workers/
        job_queue.rs
        thumbnail_worker.rs
        metadata_worker.rs
```

Keep platform-specific functionality behind portable interfaces.

Do not introduce unnecessary microservices, cloud services, or external infrastructure.

Everything should operate locally and offline.

---

# 3. Core Application Philosophy

Follow these principles throughout development:

1. Never unnecessarily modify original media.
2. Never automatically discard or delete source material.
3. Preserve original capture dates and filesystem timestamps.
4. Make organization easy, reversible, and transparent.
5. Separate physical files from virtual collections.
6. Keep routine actions simple and hide advanced configuration.
7. Make the interface visual-first rather than metadata-first.
8. Keep the application responsive even when processing thousands of files.
9. Favor reliability and data integrity over unnecessary features.
10. Design for future functionality without overengineering the initial release.

The application should make it possible to start with one enormous folder containing thousands of unorganized files and gradually turn it into a useful, searchable media archive.

---

# 4. Media Import and Discovery

The application must accept media from arbitrary local folders and storage devices.

Typical source devices include:

- Smartphones (iPhone and Android).
- GoPro cameras.
- DJI drones.
- DSLR and mirrorless cameras.
- Action cameras.
- Digital cameras.
- External hard drives.
- NAS folders.
- Downloaded media collections.
- Audio and music libraries.

Supported media categories:

**Photos**
- JPEG / JPG
- PNG
- HEIC / HEIF
- TIFF
- RAW formats where practical (CR2, CR3, NEF, ARW, DNG and others).

**Video**
- MP4
- MOV
- MKV
- M4V
- AVI
- Other FFmpeg-supported formats where practical.

**Audio**
- MP3
- AAC / M4A
- FLAC
- WAV
- AIFF
- OGG
- Other commonly used audio formats.

## Import workflow

Provide two importing methods:

**A. Add Folder / Scan Sources**

Allow the user to select an existing folder containing media.

Recursively scan subfolders and identify supported media files.

**B. Import / Ingest**

Allow the user to import media into a managed library.

Offer options to copy the original files into a user-selected library location or simply index them in their existing locations.

By default, importing must not alter or delete the source files.

Each imported file should be registered in SQLite and assigned a permanent internal media ID.

Avoid creating unnecessary duplicate database records when scanning an already-indexed folder.

Support incremental rescanning, so only new or changed files need to be processed.

Identify unsupported, inaccessible, corrupt, and incomplete files without stopping the entire import process.

---

# 5. Media Information and Metadata Extraction

Automatically extract as much useful metadata as possible.

For photographs:

- Original filename.
- File extension and MIME type.
- Capture date/time.
- Camera manufacturer and model.
- Lens information where available.
- Exposure information.
- Image width and height.
- Orientation.
- GPS coordinates.
- File size.
- EXIF and IPTC metadata.
- Embedded captions and keywords.

For videos:

- Original filename.
- Capture date/time.
- Duration.
- Video codec.
- Audio codec.
- Resolution.
- Frame rate.
- Bitrate.
- Aspect ratio.
- Camera/device metadata.
- GPS coordinates where available.
- File size.
- Embedded creation timestamps.

For audio:

- Title.
- Artist.
- Album.
- Genre.
- Duration.
- Codec.
- Bitrate.
- Sample rate.
- Embedded artwork where available.
- File size.
- Embedded date information where available.

Use multiple metadata sources and maintain a clear distinction between:

- Original capture date.
- Embedded media creation date.
- Filesystem creation/birth date.
- Filesystem modification date.
- Date added to the application.
- User-defined date corrections.

Do not assume filesystem modification dates are equivalent to actual capture dates.

Store original metadata values separately from user-edited metadata.

Where capture dates cannot be determined, allow manual corrections.

---

# 6. Main Media Browsing and Curation Interface

This is the most important part of the application.

Create a visually attractive, extremely responsive media browser.

The default view should present a large thumbnail grid, similar to a professional photo management application.

Support adjustable thumbnail sizes and multiple viewing layouts.

### Media browser requirements

- Display photos, videos, and audio files.
- Distinguish media types visually.
- Show video duration and media dimensions where appropriate.
- Display filenames.
- Display capture dates.
- Show star ratings.
- Show selection and review indicators.
- Support thumbnail previews.
- Support sorting and filtering.
- Support multi-selection.
- Support keyboard shortcuts.
- Support drag-and-drop.

Clicking a photo should open a large preview.

Clicking a video should open a video preview with playback controls.

Clicking audio should provide an audio player with basic playback controls.

Provide a useful metadata panel that can be opened or collapsed.

### Curation controls

Each media item must support:

**Five-star rating system**

- 1 star: Poor / unlikely to use.
- 2 stars: Weak / backup.
- 3 stars: Good / usable.
- 4 stars: Very good.
- 5 stars: Excellent / highlight material.

Allow the user to quickly assign ratings using keyboard shortcuts.

Also support:

- Keep / selected.
- Reject.
- Needs review.
- Favorite.
- Color labels.
- User notes.

These are separate properties. A rating should not automatically mean that a file has been selected or rejected.

Allow filtering by rating, selection status, media type, date, folder, tags, and collections.

Make it easy to identify the best media in a large collection.

---

# 7. Organization, Tags, and Collections

Support two complementary organization systems.

### A. Physical folders

Allow the user to create folders and optionally organize media into physical directory structures.

Example:

```text
Media Library/
    2026/
        Argentina-Brazil/
            Buenos-Aires/
                Photos/
                Videos/
            Florianopolis/
                Photos/
                Videos/
            Rio-de-Janeiro/
                Photos/
                Videos/

        Windsurfing/
            Sandy-Hook/
            Bonaire/

        Family/
            Birthdays/
            Holidays/
```

Allow selected files to be copied into or organized within these folders.

Moving files must be an explicit user action with a preview and confirmation.

### B. Virtual tags and collections

Allow multiple tags per media item.

Support hierarchical tags.

Example:

```text
Activities
    Windsurfing
    Kitesurfing
    Snorkeling
    Hiking

Locations
    New Jersey
        Sandy Hook
    Brazil
        Florianopolis
        Rio de Janeiro

People
    Family
    Friends

Events
    Vacations
    Birthdays
    Special Occasions
```

A single media item should be able to have multiple tags.

Create virtual collections that do not move or modify files.

Support manual collections and smart collections.

Smart collections should automatically update according to user-defined rules.

Example:

- All five-star videos.
- All photos from a particular date.
- All media tagged Windsurfing.
- All selected media from Brazil.
- All videos longer than 30 seconds.
- All media rated four stars or higher.

---

# 8. Projects and Storytelling Collections

The central concept of this application should be the ability to prepare media for a future creative project.

Introduce a first-class object called a **Story Project**.

Examples:

- Brazil 2026 Travel Highlights.
- Summer Windsurfing 2026.
- Family Birthday Celebration.
- Bonaire Adventure.
- Argentina and Brazil Travel Blog.

A project should be able to reference existing photos, videos, extracted video segments, and audio without duplicating the originals.

Each project should have:

- Project name.
- Description.
- Date range.
- Location.
- Tags.
- Cover image.
- Selected media.
- Ordered storyboard.
- User notes.
- Export settings.

### Storyboard

Allow the user to create a manually ordered sequence of selected media.

Provide:

- Drag-and-drop ordering.
- Grouping by scene or event.
- Scene descriptions.
- Captions and notes.
- Preferred photos.
- Preferred video segments.
- Suggested opening and closing shots.
- Optional associated music tracks.

Allow one original media item to participate in multiple projects.

Allow media to be added to multiple collections without copying the file.

The storyboard is a preparation tool, not necessarily a full video editor.

Its purpose is to help the user decide which material tells the best story and in what order.

---

# 9. Video Preview and Clip Extraction

Video trimming is a major feature.

Build a simple, intuitive video trimming interface integrated directly into the media browser.

Do not require users to open a separate video editing application for basic clip extraction.

### Video preview

Provide:

- Video playback.
- Pause and resume.
- Volume control.
- Playback speed.
- Timeline scrubber.
- Current timestamp.
- Total duration.
- Frame stepping where supported.
- Full-screen preview.

### In/out trimming

Allow users to select an in-point and out-point.

The interface should display a clear timeline with draggable range handles.

Show:

- Original video duration.
- In-point timecode.
- Out-point timecode.
- Selected segment duration.

Allow precise manual timecode entry as well as mouse-based selection.

Allow multiple ranges to be created from one source video.

Example:

```text
Original:
GoPro_20260714_153245.MP4
Duration: 12:30

Selected segments:

Clip 1: 00:01:12 - 00:01:47
Clip 2: 00:04:30 - 00:05:18
Clip 3: 00:09:05 - 00:09:42
```

Allow the user to export each segment separately or export all selected segments together.

### Clip processing

Use FFmpeg for video processing and FFprobe for inspection.

Prefer stream-copy / lossless extraction when technically possible.

Offer re-encoding when necessary for accurate frame-level cuts or format compatibility.

Clearly indicate whether a particular export requires re-encoding.

Never silently lower quality.

Preserve the original video and audio streams as much as possible.

Let the user choose output format and quality through simple presets, with advanced settings available separately.

### Clip naming

Allow automatically generated names such as:

```text
2026-07-14_153357_SandyHook_Windsurf_001.mp4
2026-07-14_160415_SandyHook_Windsurf_Jump_002.mp4
```

Allow manual editing of clip names.

Each extracted clip must retain a permanent relationship to its original source media in the database.

---

# 10. Critical Requirement: Timestamp and Timeline Preservation

This is a fundamental requirement. Treat it as a core architectural responsibility, not an optional metadata feature.

The application must preserve original timestamps so that photos and videos remain chronologically accurate even after renaming, copying, organizing, trimming, or exporting.

### Original source media

Never modify the original media content as part of normal curation.

When a user renames an original file, preserve its existing filesystem creation/birth timestamp and modification timestamp.

When copying an original file to another location, preserve its original filesystem timestamps wherever the destination filesystem supports doing so.

Retain original embedded capture metadata.

Store the original path and filename in SQLite even if the file is renamed.

### Derived video clips

For every extracted clip, calculate its new capture time using:

```text
New Capture Time = Original Capture Time + Clip In-Point
```

For example:

```text
Original capture time: 2026-07-14 15:32:45
Clip in-point:         00:04:30

New capture time:      2026-07-14 15:37:15
```

Apply the calculated capture time to the appropriate output metadata fields.

Preserve the original timezone and timestamp precision where available.

Maintain the source file's filesystem creation/birth timestamp and modification timestamp for derived exports as a separate preservation policy, wherever the operating system and destination filesystem permit.

Do not confuse filesystem timestamps with the adjusted capture timestamp.

If a filesystem does not permit preserving a particular timestamp, report this clearly rather than silently claiming success.

### Metadata strategy

Support:

1. Embedded metadata where technically appropriate.
2. XMP sidecar files.
3. Internal SQLite metadata.

Prefer XMP sidecars for descriptive metadata so original files do not need to be modified.

Allow the user to choose whether metadata changes are stored:

- Only in the SQLite catalog.
- In XMP sidecars.
- In both SQLite and XMP.

Support exporting metadata alongside selected media.

For video files, use appropriate QuickTime/FFmpeg metadata capabilities, accounting for differences between formats.

Retain an original metadata snapshot for every source file.

### Timestamp verification

After any rename, copy, trim, or export operation, verify the resulting timestamps.

Maintain a clear distinction between:

- The original capture time.
- The exported file's capture time.
- The original filesystem timestamps.
- The exported file's filesystem timestamps.

Never silently substitute the date of export for the original event date.

Make timestamp handling testable and provide automated regression tests.

---

# 11. File Renaming

Implement a powerful but simple batch-renaming engine.

Support naming templates using available metadata.

Example template:

```text
{capture_date}_{capture_time}_{location}_{description}_{sequence}
```

Example output:

```text
2026-07-14_153245_SandyHook_Windsurfing_001.jpg
2026-07-14_153357_SandyHook_Windsurfing_002.mp4
2026-07-14_154122_SandyHook_Beach_003.jpg
```

Allow users to:

- Rename individual files.
- Batch rename selected files.
- Select metadata fields for filenames.
- Preview changes before applying.
- Detect duplicate names.
- Handle filename conflicts.
- Undo a rename operation.
- Preserve original names in the catalog.

Renaming source files must be an explicit action.

By default, use descriptive names in the application and maintain original physical filenames until the user chooses to rename them.

An optional setting should allow automatic renaming during import.

For every rename, maintain a complete record of the original and new paths.

Treat a rename as a reversible filesystem operation.

---

# 12. Dashboard and Media Statistics

Create a useful visual dashboard.

It must clearly display the size and composition of the media library.

At the top of the main dashboard, show:

- Total media files.
- Total photos.
- Total videos.
- Total audio files.
- Total storage used.
- Total selected media.
- Total five-star media.
- Total video duration.

Include a storage breakdown:

| Media Type | File Count | Storage Used |
|---|---:|---:|
| Photos | 2,450 | 18.4 GB |
| Videos | 380 | 126.7 GB |
| Audio | 95 | 2.3 GB |
| Other | 12 | 1.1 GB |

These are illustrative values only.

Calculate actual values from the indexed media.

Provide additional statistics for:

- Individual source folders.
- Projects.
- Collections.
- Date ranges.
- Unrated files.
- Duplicate candidates.
- Missing or inaccessible media.

Make the statistics visually attractive without overwhelming the interface.

Include a clear indication of storage totals and indexing progress.

---

# 13. Search and Filtering

Provide fast, flexible media search.

Search by:

- Filename.
- Original filename.
- Location.
- Date range.
- Tags.
- Rating.
- Description.
- Project.
- Media type.
- Camera/device.
- Video duration.
- Resolution.

Provide combinations of filters.

Examples:

- All five-star GoPro videos from July 2026.
- All photographs taken in Florianopolis.
- All unrated media from a particular import folder.
- All selected media tagged Windsurfing.
- All video clips between 10 and 60 seconds.

Use appropriate SQLite indexes and full-text search where useful.

---

# 14. Duplicate and Similar Media Detection

Implement duplicate identification carefully.

Initially support exact duplicate detection using file hashes.

Use a background process to compute hashes without making the initial scanning experience unnecessarily slow.

Potential future functionality:

- Perceptual image similarity.
- Similar photographs from burst sequences.
- Near-duplicate detection.
- Similar video segment detection.

Do not automatically delete or discard duplicates.

Display duplicate candidates and let the user make the decision.

---

# 15. Export and Handoff

The application should prepare organized media collections for use in other creative applications.

Allow users to export an entire project or selected collection.

Offer output structures such as:

```text
Brazil-2026-Highlights/
    Photos/
    Videos/
    Audio/
    Metadata/
    Storyboard/
```

Export options should include:

- Copy selected original files.
- Include extracted video clips.
- Include associated audio.
- Include XMP sidecars where available.
- Generate a CSV inventory.
- Generate a JSON manifest.
- Include project descriptions and captions.
- Preserve storyboard ordering.
- Include original source references.

Provide simple output presets:

**Highlight Video**
- Selected video clips.
- Best photographs.
- Associated music tracks.
- Storyboard sequence.

**Photo Slideshow**
- Selected photographs.
- Optional video clips.
- Captions.
- Sequence order.

**Blog Post**
- Selected photographs and videos.
- Captions.
- Descriptions.
- Location and date information.

**Social Media**
- Selected images and videos.
- Relevant captions.
- Export into an organized folder.

Do not initially implement direct publishing to Instagram or other services.

The goal is to create a clean, reusable handoff package for downstream creative work.

---

# 16. User Interface and Experience

The interface must look and feel like a polished commercial desktop application.

Design principles:

- Modern and clean.
- Attractive dark theme by default, with light theme support.
- Fast.
- Minimal visual clutter.
- Large, attractive media previews.
- Clear navigation.
- Consistent icons.
- Intuitive selection controls.
- Responsive panels.
- Keyboard shortcuts.
- Context menus.
- Drag-and-drop organization.

Suggested main navigation:

```text
MEDIA CURATOR

Dashboard
    Overview
    Recent Imports

Library
    All Media
    Photos
    Videos
    Audio
    Unrated
    Favorites
    Recently Added

Collections
    My Collections
    Smart Collections

Projects
    Story Projects
    Storyboards

Tools
    Duplicate Finder
    Batch Rename
    Export Manager

Settings
```

### Main window layout

Use a practical three-panel layout:

**Left panel**
- Navigation.
- Folder tree.
- Collections.
- Tags.
- Projects.

**Center panel**
- Main media grid.
- Sorting and filtering.
- Selection controls.
- Thumbnail size adjustment.

**Right panel**
- Media metadata.
- Star rating.
- Tags.
- Notes.
- Project assignments.

All side panels should be collapsible.

### Advanced options

Hide complex options behind Advanced buttons, expandable sections, or contextual menus.

The normal workflow should not expose technical details such as codecs, FFmpeg parameters, EXIF tag names, database settings, or timestamp manipulation.

Keep these available for users who need them.

Use friendly language rather than technical jargon.

Provide clear confirmation dialogs before potentially destructive operations.

---

# 17. Performance and Reliability

Performance is a primary requirement.

Assume the application may eventually manage:

- 50,000+ media files.
- Hundreds of gigabytes of media.
- Multiple external drives.
- Large high-resolution images.
- Long 4K and 5.3K video recordings.

Requirements:

- Asynchronous scanning.
- Parallel metadata extraction with bounded concurrency.
- Background thumbnail generation.
- Thumbnail and preview caching.
- Lazy loading of thumbnails.
- Virtualized media grid.
- Incremental database updates.
- Responsive UI during background operations.
- Efficient memory management.
- Cancellation of long-running jobs.
- Clear background-job progress indicators.

Do not load entire original images or videos into memory simply to display thumbnails.

Use SQLite WAL mode, appropriate indexes, foreign keys, and database migrations.

Include database backups and recovery mechanisms.

Provide clear logs for failed operations.

Use a job queue for expensive processing tasks.

Do not block the UI thread with filesystem or media-processing work.

---

# 18. Database Design

Use SQLite as the primary persistent database.

Design a normalized schema.

At minimum, include:

**media**
- id
- original_filename
- current_filename
- original_path
- current_path
- media_type
- mime_type
- file_size
- file_hash
- capture_datetime
- capture_timezone
- capture_time_source
- filesystem_creation_time
- filesystem_modification_time
- width
- height
- duration
- codec
- camera_model
- gps_latitude
- gps_longitude
- rating
- selection_status
- review_status
- description
- notes
- added_at
- last_scanned_at

**tags**
- id
- name
- parent_id

**media_tags**
- media_id
- tag_id

**collections**
- id
- name
- description
- collection_type
- created_at

**collection_media**
- collection_id
- media_id
- sequence_order

**projects**
- id
- name
- description
- date_start
- date_end
- location
- created_at
- updated_at

**project_media**
- project_id
- media_id
- sequence_order
- scene_name
- caption
- notes

**video_segments**
- id
- source_media_id
- output_media_id
- in_point
- out_point
- calculated_capture_datetime
- exported_path
- processing_method

**file_operations**
- id
- operation_type
- original_path
- destination_path
- previous_metadata
- operation_status
- created_at

**metadata_snapshots**
- id
- media_id
- original_metadata_json
- extracted_at

**background_jobs**
- id
- job_type
- status
- progress
- error_message
- created_at
- updated_at

The schema may be expanded or normalized further as required.

Keep media records independent of collection membership so that files can belong to multiple projects without duplication.

Use database migrations.

---

# 19. File Safety and Data Integrity

This application manages potentially irreplaceable personal memories. Reliability is more important than convenience.

Implement the following safeguards:

- Never overwrite source media.
- Never automatically delete originals.
- Never move files without explicit approval.
- Preview batch operations before execution.
- Detect filename conflicts.
- Support undo for reversible filesystem operations.
- Use transactional operation journals.
- Verify copies using hashes.
- Preserve source metadata snapshots.
- Record every rename, copy, export, and trimming operation.
- Provide a clear warning when a source file is missing or inaccessible.
- Avoid modifying embedded source metadata by default.

Use safe temporary output files followed by atomic replacement where supported.

Make the first import/indexing experience read-only by default.

All operations that modify filesystem state must be explicitly initiated by the user.

---

# 20. macOS Initial Release and Future Portability

The first release must be built and tested on macOS.

Prioritize Apple Silicon support and ensure the design does not prevent Intel Mac support if practical.

Requirements:

- Native desktop application experience.
- Proper macOS application packaging.
- Correct filesystem permission handling.
- Support external drives.
- Support large file paths.
- Proper application data directory usage.
- Local database storage.
- Application settings and cache directories.
- No dependency on a running web server.

Keep the core implementation portable.

Avoid unnecessary dependencies on macOS-only frameworks.

Encapsulate platform-specific filesystem timestamp operations.

Future target operating systems:

1. macOS.
2. Windows 11.
3. Linux.

All platform-specific differences should be isolated behind Rust interfaces.

---

# 21. Development Phases and Priorities

Do not attempt to implement every feature simultaneously.

Build a genuinely usable application through incremental development.

## Phase 1 — Functional Foundation (MVP)

Implement:

- Tauri + Rust application.
- SQLite database and migrations.
- Folder selection and recursive scanning.
- Photo/video/audio file detection.
- Metadata extraction.
- Thumbnail generation.
- Main media browser.
- Media preview.
- Five-star ratings.
- Keep/reject/review statuses.
- Tags and basic collections.
- Media statistics dashboard.
- Search, sorting and filtering.
- Original media path and timestamp tracking.

The application should already be useful for reviewing and rating large media collections.

## Phase 2 — File Organization

Implement:

- Batch tagging.
- Physical folder organization.
- Batch renaming.
- Rename previews.
- Operation history.
- Undo operations.
- XMP sidecar support.
- Duplicate detection.
- Smart collections.

## Phase 3 — Video Extraction

Implement:

- Integrated video player.
- Timeline.
- In/out controls.
- Multiple clip segments.
- FFmpeg export.
- Automatic clip naming.
- Capture timestamp calculation.
- Filesystem timestamp preservation.
- Source-to-clip provenance.

Implement comprehensive timestamp tests before considering this phase complete.

## Phase 4 — Storytelling Projects

Implement:

- Story Projects.
- Project collections.
- Storyboard.
- Drag-and-drop sequencing.
- Scene organization.
- Captions and descriptions.
- Project-specific exports.
- CSV and JSON manifests.

## Phase 5 — Advanced Features

Consider:

- Perceptual duplicate detection.
- AI-assisted descriptions and tagging.
- Scene grouping.
- Optional local image recognition.
- Optional speech transcription.
- Automatic event grouping.
- Advanced timeline visualization.
- More sophisticated media export presets.

AI functionality must remain optional. The application must work fully without cloud AI services or an internet connection.

---

# 22. Testing and Acceptance Criteria

Do not consider a feature complete simply because its interface exists.

Implement actual functionality and test it.

Important acceptance tests:

**Importing**
- Import 10,000 sample media files.
- Ensure the scan completes without blocking the interface.
- Verify that different media types are correctly identified.
- Rescan and verify no duplicate records are created.

**Ratings and organization**
- Assign ratings to files.
- Filter by ratings.
- Apply multiple tags.
- Create collections.
- Restart the application and verify everything persists.

**Renaming**
- Rename a source file.
- Verify file content is unchanged.
- Verify its original capture timestamp is unchanged.
- Verify its filesystem timestamps are preserved where supported.
- Verify its original name is retained in the catalog.
- Verify undo works.

**Video extraction**
- Extract multiple segments from a video.
- Verify the exported durations and playback.
- Verify the source remains unchanged.
- Verify the calculated capture timestamps.
- Verify the original source relationship remains intact.
- Test both stream-copy and re-encoded exports.

**Timestamp preservation**

Create automated tests using sample photos and videos containing known capture timestamps.

Test:
- Original import.
- File rename.
- File copy.
- Video extraction.
- Multiple exported versions.
- Timezone handling.
- Missing capture metadata.
- Filesystem timestamp limitations.

Verify that the calculated capture time of each clip is the source capture time plus its in-point.

**Statistics**
- Ensure total media counts are correct.
- Ensure counts per media type are correct.
- Ensure storage totals match actual indexed file sizes.
- Ensure dashboard statistics update after imports and exports.

**Portability**
- Keep the core Rust libraries independent of the macOS UI environment.
- Test platform-specific filesystem behavior separately.
- Document the process for adding Windows and Linux support.

---

# 23. Documentation and Deliverables

Produce:

1. Complete Rust source code.
2. Complete frontend source code.
3. SQLite schema and migrations.
4. FFmpeg/FFprobe integration.
5. ExifTool integration.
6. Installation and build instructions for macOS.
7. Development environment instructions.
8. User documentation.
9. Architecture documentation.
10. Timestamp preservation documentation.
11. Backup and recovery documentation.
12. Automated tests.
13. A sample media library for testing.

Provide a working application, not just a prototype or an architectural proposal.

---

# 24. Final Development Instructions

Before implementing:

1. Examine the development environment and existing repository.
2. Identify the installed Rust toolchain and available build dependencies.
3. Establish the application architecture.
4. Define database migrations and core data structures.
5. Establish the media timestamp and provenance model before building file-processing features.
6. Create the initial user interface and application shell.

Then implement the application incrementally.

Prioritize a cohesive, polished experience rather than a collection of disconnected features.

Do not create nonfunctional buttons, placeholder screens, or simulated media operations.

Do not sacrifice timestamp accuracy, data safety, or source-file integrity to simplify implementation.

Keep dependencies actively maintained and compatible with the selected Rust and Tauri versions.

Use clear error handling, logging, comments where helpful, and maintainable code.

When making architectural tradeoffs, prioritize portability, reliability, performance, and ease of future development.

**The ultimate goal: A simple, powerful personal media curation system that transforms an overwhelming pile of raw photos, videos, and audio into a beautifully organized collection of the best moments, ready to tell a story.**