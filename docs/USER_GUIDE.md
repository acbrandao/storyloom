# User guide

## 1. Add media

Click **Add Media** (or drag a folder onto the window).

- **Add a folder** (recommended): catalogs files where they are. StoryLoom only reads them. Nothing is moved, renamed or modified.
- **Import into my library**: copies from a card, phone or folder into a library folder, organized by date taken or by the original folder structure. You see every destination before anything is copied. Each copy is verified and keeps its original dates. Files already in the library (identical content) are skipped.

Indexing runs in the background (see the bottom of the sidebar). You can browse, rate and tag while it runs, and cancel at any time. Problems such as unsupported, empty or unreadable files are listed under **Activity & History**.

To pick up new or changed files, right-click a folder → **Rescan**. Files that disappeared are marked *Missing* and are never removed automatically. External drives that are unplugged show as *offline*.

## 2. Review and curate

- Click a thumbnail to select it. Use ⌘-click or Shift-click for multiple, ⌘A for all.
- **Space / Enter / double-click** opens the large viewer. Photos zoom on click; videos have speed, frame-step and fullscreen controls.
- Use the keyboard:

| Key | Action |
|---|---|
| 0–5 | Star rating (0 clears) |
| P / X / U | Keep / Reject / clear |
| R | Needs review |
| F | Favorite |
| 6 7 8 9 | Red, yellow, green, blue label |
| ← → ↑ ↓ | Move (Shift extends the selection) |
| F2 | Rename the selected files |
| C, I, O | Viewer: clip editor, set in-point, set out-point |

Turn on **auto-advance** (⏩ in the toolbar) to jump to the next item after each rating. That makes it fast to cull hundreds of shots.

Ratings, keep/reject, review, favorites and labels are independent: a 5-star photo is not automatically "kept".

## 3. Organize

- **Tags** are hierarchical. Type `Locations/Brazil/Rio de Janeiro` to create the whole path. Filtering by a parent tag includes its children.
- **Collections** are virtual groups; files are never copied. Drag thumbnails onto a collection or tag in the sidebar.
- **Smart collections**: set filters, then **Save as Smart Collection**. They update automatically.
- **Folders** in the sidebar show the real folder tree of each source.

## 4. Rename and organize files on disk

Everything in sections 1–3 only changes StoryLoom's catalog. The following actions change files, so each one shows a full preview first, asks for confirmation, is recorded in **Activity & History**, and can be **undone**.

- **Rename** (F2, right-click → *Rename File(s)…*, or the details panel): build names from a template. Click the field buttons to insert `{capture_date}`, `{capture_time}`, `{location}`, `{description}`, `{camera}`, `{original_name}`, `{sequence}` and others. Numbers follow the order the photos were taken. Names that are already taken are either left alone or get a number added, as you choose. Existing files are never replaced. The original filename is always kept in the catalog and shown in the details panel.
- **Copy or Move to Folder…**: choose a destination and optional subfolders (by year, by day, by place, by type, or your own template).
  - *Copy* makes verified copies with the original dates and carries over ratings, tags and notes.
  - *Move* relocates files on the same drive. Moving between drives isn't offered, because it would mean deleting originals.
- **Undo**: from the notification that appears when an operation finishes, or later from *Activity & History → File changes you can undo*. Renames and moves are put back. Copies are moved to the Trash, and only if they haven't been changed since.
- **Rename on import**: in *Add Media → Import into my library*, tick *Rename the copies* and choose a template.

## 5. Make clips from videos

Open a video, then click **Clips** (or press C).

1. Mark a range. Drag the yellow handles on the filmstrip (the video follows the handle, so you see the exact frame), press **I** and **O** at the playhead, or type timecodes.
2. **Add clip**, optionally with a name such as "Jump". Repeat for as many moments as you like. Clips stay as drafts, and you can edit or delete them until they're exported.
3. **Export**: choose a folder and a quality:
   - **Original quality (fast)**: no quality loss. The clip may start a fraction of a second earlier, at the nearest keyframe; the preview shows exactly how much.
   - **Exact cut**: frame-accurate, re-encoded in the original format at very high quality.
   - **Most compatible**: a frame-accurate H.264 MP4 for any device or app. HDR/10-bit video becomes 8-bit, and the preview warns you first.

Each clip is named from a template using its own capture time (original + in-point), and records the date and time it was really taken. The original video is never changed. Clips appear in your library with a "Clip of …" link back to the original; from the original, the details panel shows how many clips were made.

## 6. Find

Search matches file names, folders, tags, notes, descriptions, camera and audio tags. **Filter** adds type, rating, status, label, tag, date range, camera, duration and GPS conditions, and they can be combined (for example, "five-star GoPro videos from July 2026").

## 7. Dates

The details panel shows when an item was taken and *where that date came from*. If a date is wrong or missing, use **Adjust…**. The correction is stored in the catalog only; the original value and the file stay as they were. **Library → No Capture Date** lists items that need attention.

## 8. Dashboard and tools

- **Dashboard**: library size and composition, ratings, timeline, cameras, library health and source folders. Everything is clickable.
- **Duplicate Finder**: byte-identical files. Nothing is deleted; mark unwanted copies as rejected or remove them in Finder yourself.
- **Activity & History**: scans, imports and a journal of every file operation, with verification results.
- **Settings**: theme, tool status, backups, integrity check, keyboard shortcuts.
