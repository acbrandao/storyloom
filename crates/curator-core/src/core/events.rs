//! Events emitted by the core for the UI. The shell provides an [`EventSink`]
//! implementation (Tauri forwards them to the webview).

use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoreEvent {
    /// A background job changed state or progressed.
    #[serde(rename_all = "camelCase")]
    JobProgress {
        job_id: i64,
        job_type: String,
        status: String,
        phase: String,
        done: u64,
        total: u64,
        message: Option<String>,
    },
    /// Catalog content changed (new media, metadata ready, thumbnails ready).
    #[serde(rename_all = "camelCase")]
    LibraryChanged { reason: String },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

/// Sink that drops events (used in tests and CLI tools).
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _event: CoreEvent) {}
}

pub type SharedSink = Arc<dyn EventSink>;
