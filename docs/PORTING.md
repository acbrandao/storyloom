# Porting to Windows and Linux

Everything except `crates/curator-core/src/platform/` and the Tauri bundle configuration is already portable: Rust std, SQLite (bundled), the `image` crate, FFmpeg/ExifTool subprocesses, and WebView2/WebKitGTK through Tauri.

## Work items

| Area | File | macOS | Windows | Linux |
|---|---|---|---|---|
| Set birth time | `platform/mod.rs::set_birth_time` | `setattrlist` ✓ | implement with `windows-sys` `SetFileTime(lpCreationTime)` | not possible; already reports `Unsupported` |
| Read birth time | `std::fs::Metadata::created` | ✓ | ✓ | ✓ via `statx` (kernel ≥ 4.11, filesystem-dependent) |
| HEIC/RAW thumbnails | `platform::native_image_render` | `sips` ✓ | WIC (`windows` crate) or bundled `libheif`/`libraw` | `libheif`/`libraw` or ImageMagick |
| Reveal / open file | `platform::reveal_in_file_manager` | `open -R` ✓ | `explorer /select,` ✓ | `xdg-open <dir>` ✓ |
| Free space | `media/importer.rs::free_space` | `statvfs` ✓ | `GetDiskFreeSpaceExW` | `statvfs` ✓ |
| Tool discovery | `tools.rs` | Homebrew paths ✓ | add `%ProgramFiles%`, winget/choco locations; `.exe` suffix handled | `/usr/bin` ✓ |
| Custom protocols | `src-tauri/src/protocols.rs` | `thumb://localhost` | `http://thumb.localhost` (handled by `convertFileSrc`) | `thumb://localhost` |
| Video playback | webview | WKWebView: H.264/HEVC/ProRes | WebView2: H.264 (HEVC needs an extension) | WebKitGTK: GStreamer plugins |
| Bundling | `tauri.conf.json` `bundle.targets` | app, dmg | add `msi`/`nsis` | add `deb`/`appimage` |

## Steps

1. Implement `set_birth_time` for Windows. The existing `tests/timestamps.rs::birth_time_can_be_set_on_this_platform` covers it; update its expectation for the platform.
2. Add a native HEIC/RAW renderer or bundle a decoder. Everything else falls back to FFmpeg.
3. Add bundle targets and test `npm run tauri build` on each OS.
4. Run `cargo test -p curator-core` on each OS. The tests are designed to pass where a capability is reported as unsupported, rather than claiming success.
