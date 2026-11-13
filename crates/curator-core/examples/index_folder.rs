//! Index a folder into a catalog from the command line (headless; useful for demos and CI).
//!
//! cargo run -p curator-core --example index_folder -- <base-dir> <media-folder>
//! (<base-dir> is the same value you would pass as STORYLOOM_DATA_DIR)

use curator_core::core::configuration::Configuration;
use curator_core::core::events::NullSink;
use curator_core::Application;
use std::path::PathBuf;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (base, folder) = (PathBuf::from(&args[0]), PathBuf::from(&args[1]));
    let rt = tokio::runtime::Runtime::new()?;
    let app = Application::open(Configuration::new(base.join("data"), base.join("cache")), Arc::new(NullSink), &[], rt.handle().clone())?;
    let started = std::time::Instant::now();
    app.add_source(&folder)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    while !app.active_jobs().is_empty() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let s = app.stats()?;
    println!("Indexed {} items ({} photos, {} videos, {} audio) in {:?}", s.total_media, s.total_photos, s.total_videos, s.total_audio, started.elapsed());
    Ok(())
}
