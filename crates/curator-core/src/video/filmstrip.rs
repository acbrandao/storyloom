//! Timeline filmstrip: a strip of evenly spaced frames shown behind the trim handles.
//! Frames are grabbed with fast keyframe seeks in parallel, so long 4K/5.3K files are quick.

use crate::core::configuration::Configuration;
use crate::core::errors::{CuratorError, Result};
use crate::database::repositories::media::MediaLocation;
use crate::media::thumbnails::ffmpeg_frame;
use crate::tools::Tools;
use image::{GenericImage, RgbImage};
use std::path::{Path, PathBuf};

pub const FRAMES: u32 = 16;
pub const HEIGHT: u32 = 72;

pub fn filmstrip(loc: &MediaLocation, tools: &Tools, cfg: &Configuration) -> Result<PathBuf> {
    let out = cfg.cache_dir.join("filmstrips").join(format!("{}-{}.jpg", loc.id, loc.updated_at));
    if out.exists() {
        return Ok(out);
    }
    let ffmpeg = tools.require_ffmpeg()?.to_path_buf();
    let duration = loc.duration.filter(|d| *d > 0.0).ok_or_else(|| CuratorError::invalid("video duration unknown"))?;
    std::fs::create_dir_all(out.parent().unwrap())?;
    let tmpdir = std::env::temp_dir().join(format!("mc-strip-{}-{}", loc.id, std::process::id()));
    std::fs::create_dir_all(&tmpdir)?;
    let src = PathBuf::from(&loc.path);
    let handles: Vec<_> = (0..FRAMES)
        .map(|i| {
            let (ffmpeg, src, frame) = (ffmpeg.clone(), src.clone(), tmpdir.join(format!("{i}.jpg")));
            let t = duration * (i as f64 + 0.5) / FRAMES as f64;
            std::thread::spawn(move || ffmpeg_frame(&ffmpeg, &src, Some(t), &frame, HEIGHT * 3).ok().map(|_| frame))
        })
        .collect();
    let frames: Vec<Option<PathBuf>> = handles.into_iter().map(|h| h.join().ok().flatten()).collect();
    let decoded: Vec<Option<image::DynamicImage>> = frames.iter().map(|f| f.as_deref().and_then(|p: &Path| image::open(p).ok())).collect();
    let first = decoded.iter().flatten().next().ok_or_else(|| CuratorError::ToolFailed("could not read frames from the video".into()))?;
    let fw = (first.width() as f64 * HEIGHT as f64 / first.height() as f64).round().max(1.0) as u32;
    let mut strip = RgbImage::new(fw * FRAMES, HEIGHT);
    for (i, img) in decoded.iter().enumerate() {
        if let Some(img) = img {
            let tile = img.resize_exact(fw, HEIGHT, image::imageops::FilterType::Triangle).to_rgb8();
            let _ = strip.copy_from(&tile, fw * i as u32, 0);
        }
    }
    let tmp = out.with_extension("tmp.jpg");
    strip.save(&tmp)?;
    std::fs::rename(&tmp, &out)?;
    let _ = std::fs::remove_dir_all(&tmpdir);
    Ok(out)
}
