//! Background job bookkeeping: registration, throttled progress reporting, cancellation.

use crate::core::application::AppInner;
use crate::core::events::CoreEvent;
use crate::database::repositories::jobs;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveJob {
    pub id: i64,
    pub job_type: String,
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub message: Option<String>,
}

#[derive(Default)]
pub struct JobManager {
    active: Mutex<HashMap<i64, (CancellationToken, ActiveJob)>>,
}

impl JobManager {
    pub fn register(&self, id: i64, job_type: &str) -> CancellationToken {
        let token = CancellationToken::new();
        let job = ActiveJob { id, job_type: job_type.into(), phase: "Queued".into(), done: 0, total: 0, message: None };
        self.active.lock().unwrap().insert(id, (token.clone(), job));
        token
    }
    pub fn update(&self, id: i64, phase: &str, done: u64, total: u64, message: Option<String>) {
        if let Some((_, j)) = self.active.lock().unwrap().get_mut(&id) {
            j.phase = phase.into();
            j.done = done;
            j.total = total;
            j.message = message;
        }
    }
    pub fn unregister(&self, id: i64) {
        self.active.lock().unwrap().remove(&id);
    }
    pub fn cancel(&self, id: i64) -> bool {
        match self.active.lock().unwrap().get(&id) {
            Some((t, _)) => {
                t.cancel();
                true
            }
            None => false,
        }
    }
    pub fn cancel_all(&self) {
        for (t, _) in self.active.lock().unwrap().values() {
            t.cancel();
        }
    }
    pub fn list(&self) -> Vec<ActiveJob> {
        let mut v: Vec<_> = self.active.lock().unwrap().values().map(|(_, j)| j.clone()).collect();
        v.sort_by_key(|j| j.id);
        v
    }
    pub fn is_source_busy(&self, job_type_prefix: &str) -> bool {
        self.active.lock().unwrap().values().any(|(_, j)| j.job_type == job_type_prefix)
    }
}

/// Handle given to a running job for progress reporting.
pub struct JobContext {
    pub app: Arc<AppInner>,
    pub id: i64,
    pub job_type: String,
    pub token: CancellationToken,
    last_emit: Mutex<(Instant, String)>,
    last_db: Mutex<Instant>,
    last_library_event: Mutex<Instant>,
}

impl JobContext {
    pub fn new(app: Arc<AppInner>, id: i64, job_type: &str, token: CancellationToken) -> Self {
        let past = Instant::now() - Duration::from_secs(10);
        Self {
            app,
            id,
            job_type: job_type.into(),
            token,
            last_emit: Mutex::new((past, String::new())),
            last_db: Mutex::new(past),
            last_library_event: Mutex::new(Instant::now()),
        }
    }

    pub fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Report progress; throttled so tight loops don't flood the UI or the database.
    pub fn progress(&self, phase: &str, done: u64, total: u64) {
        self.progress_msg(phase, done, total, None)
    }

    pub fn progress_msg(&self, phase: &str, done: u64, total: u64, message: Option<String>) {
        self.app.jobs.update(self.id, phase, done, total, message.clone());
        let now = Instant::now();
        {
            let mut last = self.last_emit.lock().unwrap();
            let phase_changed = last.1 != phase;
            if !phase_changed && now.duration_since(last.0) < Duration::from_millis(120) && done != total {
                return;
            }
            *last = (now, phase.to_string());
        }
        self.app.sink.emit(CoreEvent::JobProgress {
            job_id: self.id,
            job_type: self.job_type.clone(),
            status: "running".into(),
            phase: phase.into(),
            done,
            total,
            message,
        });
        let mut last_db = self.last_db.lock().unwrap();
        if now.duration_since(*last_db) > Duration::from_secs(1) {
            *last_db = now;
            if let Ok(c) = self.app.db.conn() {
                let _ = jobs::update_progress(&c, self.id, "running", phase, done, total);
            }
        }
    }

    /// Tell the UI the catalog changed (at most about once per second while a job runs).
    pub fn library_changed(&self, reason: &str, force: bool) {
        let mut last = self.last_library_event.lock().unwrap();
        if force || last.elapsed() > Duration::from_millis(1200) {
            *last = Instant::now();
            self.app.sink.emit(CoreEvent::LibraryChanged { reason: reason.into() });
        }
    }

    pub fn finish(&self, status: &str, summary: Option<serde_json::Value>, error: Option<String>) {
        if let Ok(c) = self.app.db.conn() {
            let _ = jobs::finish(&c, self.id, status, summary.as_ref(), error.as_deref());
        }
        self.app.jobs.unregister(self.id);
        self.app.sink.emit(CoreEvent::JobProgress {
            job_id: self.id,
            job_type: self.job_type.clone(),
            status: status.into(),
            phase: match status {
                "completed" => "Done".into(),
                "cancelled" => "Cancelled".into(),
                _ => "Failed".into(),
            },
            done: 0,
            total: 0,
            message: error.or_else(|| summary.map(|s| s.to_string())),
        });
        self.app.sink.emit(CoreEvent::LibraryChanged { reason: format!("{}-finished", self.job_type) });
    }
}
