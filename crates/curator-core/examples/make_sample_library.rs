//! Generate a sample media library for manual testing.
//!
//! cargo run -p curator-core --example make_sample_library -- ~/Desktop/SampleMedia [--bulk 10000]

use curator_core::tools::Tools;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(args.first().cloned().unwrap_or_else(|| "sample-media".into()));
    let tools = Tools::discover(&[]);
    if tools.ffmpeg.is_none() {
        eprintln!("FFmpeg not found: videos and audio will be skipped (brew install ffmpeg)");
    }
    let files = curator_core::testing::make_sample_library(&root, tools.ffmpeg.as_deref())?;
    println!("Created {} sample files in {}", files.len(), root.display());
    if let Some(pos) = args.iter().position(|a| a == "--bulk") {
        let n: usize = args.get(pos + 1).and_then(|s| s.parse().ok()).unwrap_or(10_000);
        curator_core::testing::make_bulk_photos(&root, n)?;
        println!("Created {n} bulk photos in {}/bulk", root.display());
    }
    Ok(())
}
