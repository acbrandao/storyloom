//! StoryLoom desktop shell: wires the UI-independent `curator-core` to Tauri.
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
    let file = tracing_appender::rolling::daily(dir, "storyloom.log");
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

/// The app was called "Media Curator" (identifier `app.mediacurator.desktop`) before 0.3.1.
/// Move its catalog, cache and logs to the StoryLoom locations once, if the new ones don't exist yet.
fn migrate_from_old_identifier(new_dirs: &[&std::path::Path]) {
    for new in new_dirs {
        let Some(name) = new.file_name().and_then(|n| n.to_str()) else { continue };
        let old = new.with_file_name(name.replace("app.storyloom.desktop", "app.mediacurator.desktop"));
        if old != **new && old.exists() && !new.exists() {
            match std::fs::rename(&old, new) {
                Ok(()) => eprintln!("moved {} -> {}", old.display(), new.display()),
                Err(e) => eprintln!("could not move {}: {e}", old.display()),
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("thumb", protocols::thumbnail)
        .register_asynchronous_uri_scheme_protocol("media", protocols::media)
        .setup(|app| {
            let paths = app.path();
            // STORYLOOM_DATA_DIR keeps a separate catalog/cache (development, testing, demos).
            let (data_dir, cache_dir, log_dir) = match std::env::var_os("STORYLOOM_DATA_DIR") {
                Some(base) => {
                    let base = std::path::PathBuf::from(base);
                    (base.join("data"), base.join("cache"), base.join("logs"))
                }
                None => {
                    let dirs = (paths.app_data_dir()?, paths.app_cache_dir()?, paths.app_log_dir()?);
                    migrate_from_old_identifier(&[&dirs.0, &dirs.1, &dirs.2]);
                    dirs
                }
            };
            if let Some(guard) = init_logging(&log_dir) {
                // Keep the log writer alive for the lifetime of the app.
                app.manage(guard);
            }
            tracing::info!(data = %data_dir.display(), cache = %cache_dir.display(), "starting StoryLoom");
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
        .expect("error while running StoryLoom");
}
