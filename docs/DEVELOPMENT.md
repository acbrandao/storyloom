# Development environment

```
crates/curator-core/   UI-independent core (all media, filesystem and database logic) + tests
src-tauri/             Tauri 2 shell: commands, custom protocols, app setup
src/                   React + TypeScript UI (Vite)
docs/                  Documentation
assets/icon.svg        App icon source (icons regenerated with `npx tauri icon assets/icon.png`)
```

- `npm run tauri dev` starts Vite on :1420 and the Rust app with hot reload (UI) and rebuild-on-change (Rust).
- `STORYLOOM_DATA_DIR=/tmp/x` uses an isolated catalog, cache and logs.
- `RUST_LOG=debug,curator_core=trace` sets verbose logging. Logs also go to the log directory.
- Dependencies are built with `opt-level = 3` even in dev, because image decoding is otherwise far too slow.
- To add a migration, create `database/migrations/00N_name.sql` and append it to `MIGRATIONS` in `migrations.rs`. Never edit a shipped migration.
- To add a command, write a core method on `Application`, then a wrapper in `src-tauri/src/commands.rs` (register it in `handler()`), then a typed wrapper in `src/api.ts`.

## Headless helpers

- `cargo run -p curator-core --example index_folder -- <base-dir> <folder>` indexes a folder into the catalog at `<base-dir>` (the same value as `STORYLOOM_DATA_DIR`) without the UI.
- `VITE_START_VIEW=library:photos`, `viewer:videos`, `rename:photos`, `organize:photos` or `clips:videos` (dev builds only) opens that view, the viewer, or a file-operation dialog at launch. `VITE_ORGANIZE_DEST` pre-fills the organize destination. This is useful for screenshots without UI automation.
- `RUST_LOG=info,storyloom_lib=debug` logs every `media://` request and Range response, which helps diagnose video playback.
- Opening `http://localhost:1420` in a normal browser while `npm run dev` runs shows the UI with mock data (`src/devMock.ts`).
- `STORYLOOM_TRASH_DIR=/some/dir` makes undo-of-copy move files there instead of the system Trash. The tests use this.
