//! Media Curator desktop shell: wires the UI-independent `curator-core` to Tauri.
//! All media and filesystem work happens in the core; this crate only exposes commands,
//! forwards events and serves thumbnails/media to the webview through custom protocols.

mod commands;
mod protocols;

use curator_core::core::configuration::Configuration;
use curator_core::core::events::{CoreEvent, EventSink};
use curator_core::Application;
use std::sync::Arc;
use tauri::{Emitter, Manager};

struct TauriSink(tauri::AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: CoreEvent) {
        let _ = self.0.emit("core-event", event);
    }
}

pub struct AppState(pub Application);

fn init_logging(dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    std::fs::create_dir_all(dir).ok()?;
    let file = tracing_appender::rolling::daily(dir, "media-curator.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,curator_core=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_ansi(false).with_writer(writer))
        .try_init()
        .ok()?;
    Some(guard)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("thumb", protocols::thumbnail)
        .register_asynchronous_uri_scheme_protocol("media", protocols::media)
        .setup(|app| {
            let paths = app.path();
            // MEDIA_CURATOR_DATA_DIR keeps a separate catalog/cache (development, testing, demos).
            let (data_dir, cache_dir, log_dir) = match std::env::var_os("MEDIA_CURATOR_DATA_DIR") {
                Some(base) => {
                    let base = std::path::PathBuf::from(base);
                    (base.join("data"), base.join("cache"), base.join("logs"))
                }
                None => (paths.app_data_dir()?, paths.app_cache_dir()?, paths.app_log_dir()?),
            };
            if let Some(guard) = init_logging(&log_dir) {
                // Keep the log writer alive for the lifetime of the app.
                app.manage(guard);
            }
            tracing::info!(data = %data_dir.display(), cache = %cache_dir.display(), "starting Media Curator");
            let cfg = Configuration::new(&data_dir, &cache_dir);
            // Tools may be bundled next to the executable in the future (Resources/bin).
            let mut extra = vec![];
            if let Ok(res) = paths.resource_dir() {
                extra.push(res.join("bin"));
            }
            let sink = Arc::new(TauriSink(app.handle().clone()));
            let runtime = tauri::async_runtime::handle().inner().clone();
            let core = Application::open(cfg, sink, &extra, runtime)?;
            match core.auto_backup() {
                Ok(Some(p)) => tracing::info!(path = %p.display(), "daily catalog backup written"),
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "automatic backup failed"),
            }
            app.manage(AppState(core));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.try_state::<AppState>() {
                    state.0.cancel_all_jobs();
                }
            }
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("error while running Media Curator");
}
