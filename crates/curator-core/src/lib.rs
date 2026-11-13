//! # curator-core
//!
//! The UI-independent heart of Media Curator. Everything that touches source media,
//! the catalog database, or the filesystem lives here, so the desktop shell (Tauri)
//! is only a thin command/event adapter and the logic is testable without a GUI.
//!
//! Module map:
//! - [`core`]: errors, configuration (data/cache dirs), events, the `Application` facade.
//! - [`database`]: SQLite connection pool, migrations, models and repositories.
//! - [`media`]: media type detection, scanning, metadata extraction, thumbnails, hashing.
//! - [`filesystem`]: timestamp-preserving, verified, journaled file operations.
//! - [`video`]: FFprobe inspection and clip capture-time math.
//! - [`workers`]: background job queue with progress and cancellation.
//! - [`platform`]: OS-specific code (birth-time access, native thumbnailers) behind portable APIs.
//! - [`tools`]: discovery of external tools (ffmpeg, ffprobe, exiftool).

pub mod core;
pub mod database;
pub mod filesystem;
pub mod media;
pub mod platform;
#[doc(hidden)]
pub mod testing;
pub mod tools;
pub mod video;
pub mod workers;

pub use crate::core::application::Application;
pub use crate::core::errors::{CuratorError, Result};
