# Backup and recovery

## What needs backing up

| Item | Location (macOS) | Notes |
|---|---|---|
| Catalog | `~/Library/Application Support/app.mediacurator.desktop/catalog.sqlite` (+ `-wal`, `-shm`) | Ratings, tags, collections, notes, corrections, journal |
| Catalog backups | `…/app.mediacurator.desktop/backups/` | Automatic daily snapshots |
| Cache | `~/Library/Caches/app.mediacurator.desktop/` | Thumbnails and previews; safe to delete |
| Logs | `~/Library/Logs/app.mediacurator.desktop/` | Daily rolling `media-curator.log` |

Your media files are never stored inside the app. Back them up with your normal tools (Time Machine and so on).

## Automatic backups

On launch, if the newest backup is more than 24 hours old, Media Curator writes `backups/catalog-YYYYMMDD-HHMMSS.sqlite` using SQLite `VACUUM INTO`. This produces a consistent snapshot while the app runs. The 20 most recent backups are kept. *Settings → Back up now* makes one on demand. *Check catalog integrity* runs `PRAGMA integrity_check`.

## Restoring a backup

1. Quit Media Curator.
2. In `~/Library/Application Support/app.mediacurator.desktop/`, move `catalog.sqlite`, `catalog.sqlite-wal` and `catalog.sqlite-shm` somewhere safe. Don't delete them yet.
3. Copy the chosen `backups/catalog-….sqlite` to `catalog.sqlite`.
4. Start the app and rescan your folders (right-click a folder → *Rescan*) to pick up files added since the backup.

## Crash safety

- SQLite WAL mode with transactions: a crash never leaves a half-applied batch.
- Background jobs and file operations left "running" are marked *failed / interrupted* on next launch and shown in *Activity & History*.
- Copies are written to a hidden `.<name>.mcpartial-…` temp file. An interrupted copy never appears under the final name. Such temp files can be deleted safely.

## Moving to a new Mac

Copy the `app.mediacurator.desktop` Application Support folder. If your media lives at different paths on the new Mac, re-add those folders; matching by path means old records show as *Missing* until the original paths are available again.
