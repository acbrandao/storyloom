#![allow(dead_code)]
use curator_core::core::configuration::Configuration;
use curator_core::core::events::NullSink;
use curator_core::Application;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn open_app(base: &Path, rt: &tokio::runtime::Runtime) -> Application {
    let mut cfg = Configuration::new(base.join("data"), base.join("cache"));
    cfg.worker_concurrency = 4;
    Application::open(cfg, Arc::new(NullSink), &[], rt.handle().clone()).expect("open app")
}

/// Wait until no background job is running.
pub fn wait_idle(app: &Application) {
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(50));
    while !app.active_jobs().is_empty() {
        assert!(start.elapsed() < Duration::from_secs(300), "jobs did not finish in time");
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap()
}
