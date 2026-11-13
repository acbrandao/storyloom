//! Thumbnail and preview generation. Output goes to the cache directory only; sources are
//! opened read-only. Files are written to a temporary name and renamed into place so a
//! half-written thumbnail is never served.

use super::media_types;
use crate::core::configuration::Configuration;
use crate::core::errors::{CuratorError, Result};
use crate::database::models::MediaType;
use crate::database::repositories::media::MediaLocation;
use crate::platform;
use crate::tools::Tools;
use crate::video::probe::run_with_timeout;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbOutcome {
    Created,
    /// Nothing to show (e.g. audio without cover art).
    Unsupported,
}

pub fn generate_thumbnail(loc: &MediaLocation, tools: &Tools, cfg: &Configuration) -> Result<ThumbOutcome> {
    let dest = cfg.thumbnail_path(loc.id);
    render(loc, tools, &dest, cfg.thumbnail_size)
}

/// Path of an image the webview can display for this photo: the original when the webview
/// supports its format, otherwise a cached JPEG preview (RAW files etc.).
pub fn display_image_path(loc: &MediaLocation, tools: &Tools, cfg: &Configuration) -> Result<PathBuf> {
    if media_types::WEBVIEW_IMAGE_EXTENSIONS.contains(&loc.extension.as_str()) {
        return Ok(PathBuf::from(&loc.path));
    }
    let dest = cfg.preview_path(loc.id);
    if !dest.exists() {
        if render(loc, tools, &dest, cfg.preview_size)? == ThumbOutcome::Unsupported {
            return Err(CuratorError::invalid("no preview can be generated for this file"));
        }
    }
    Ok(dest)
}

fn render(loc: &MediaLocation, tools: &Tools, dest: &Path, size: u32) -> Result<ThumbOutcome> {
    let src = Path::new(&loc.path);
    if !src.exists() {
        return Err(CuratorError::NotFound(format!("file is missing: {}", loc.path)));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dest.with_extension(format!("tmp{}-{n}.jpg", std::process::id()));
    let result = match loc.media_type {
        MediaType::Photo => photo(src, &loc.extension, tools, &tmp, size),
        MediaType::Video => video(src, loc.duration, tools, &tmp, size),
        MediaType::Audio => audio_art(src, tools, &tmp, size),
    };
    match result {
        Ok(ThumbOutcome::Created) => {
            std::fs::rename(&tmp, dest)?;
            Ok(ThumbOutcome::Created)
        }
        other => {
            let _ = std::fs::remove_file(&tmp);
            other
        }
    }
}

fn photo(src: &Path, ext: &str, tools: &Tools, out: &Path, size: u32) -> Result<ThumbOutcome> {
    if media_types::RUST_DECODABLE.contains(&ext) {
        match rust_decode(src, out, size) {
            Ok(()) => return Ok(ThumbOutcome::Created),
            Err(e) => tracing::debug!(error = %e, path = %src.display(), "built-in decoder failed; trying fallbacks"),
        }
    }
    // HEIC / RAW / anything the built-in decoder rejected: the OS image framework.
    if platform::native_image_render(src, out, size)? {
        return Ok(ThumbOutcome::Created);
    }
    // Last resort: FFmpeg can decode many still formats.
    if let Some(ffmpeg) = &tools.ffmpeg {
        if ffmpeg_frame(ffmpeg, src, None, out, size).is_ok() {
            return Ok(ThumbOutcome::Created);
        }
    }
    Err(CuratorError::invalid("this image format could not be decoded"))
}

fn rust_decode(src: &Path, out: &Path, size: u32) -> Result<()> {
    let mut reader = ImageReader::open(src)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    // Downscale before rotating (cheaper), then bake orientation into pixels.
    img = img.thumbnail(size, size);
    img.apply_orientation(orientation);
    let rgb = img.to_rgb8();
    let mut f = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 82);
    enc.encode_image(&rgb)?;
    Ok(())
}

fn video(src: &Path, duration: Option<f64>, tools: &Tools, out: &Path, size: u32) -> Result<ThumbOutcome> {
    let ffmpeg = tools.require_ffmpeg()?;
    // A frame slightly into the clip avoids black first frames.
    let at = duration.map(|d| (d * 0.1).min(5.0)).filter(|t| *t > 0.0);
    if ffmpeg_frame(ffmpeg, src, at, out, size).is_ok() {
        return Ok(ThumbOutcome::Created);
    }
    ffmpeg_frame(ffmpeg, src, None, out, size)?;
    Ok(ThumbOutcome::Created)
}

fn audio_art(src: &Path, tools: &Tools, out: &Path, size: u32) -> Result<ThumbOutcome> {
    let Some(ffmpeg) = &tools.ffmpeg else { return Ok(ThumbOutcome::Unsupported) };
    let scale = format!("scale={size}:{size}:force_original_aspect_ratio=decrease");
    let o = run_with_timeout(
        Command::new(ffmpeg)
            .args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(src)
            .args(["-an", "-map", "0:v:0", "-frames:v", "1", "-vf", &scale, "-q:v", "4", "-f", "image2"])
            .arg(out),
        Duration::from_secs(30),
    )?;
    if o.status.success() && out.exists() && std::fs::metadata(out)?.len() > 0 {
        Ok(ThumbOutcome::Created)
    } else {
        Ok(ThumbOutcome::Unsupported)
    }
}

/// Extract one frame (auto-rotated by FFmpeg) scaled to fit `size`.
pub fn ffmpeg_frame(ffmpeg: &Path, src: &Path, at: Option<f64>, out: &Path, size: u32) -> Result<()> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-y"]);
    if let Some(t) = at {
        cmd.args(["-ss", &format!("{t:.3}")]); // input seeking: fast, keyframe-based
    }
    let scale = format!("scale={size}:{size}:force_original_aspect_ratio=decrease");
    cmd.arg("-i").arg(src).args(["-frames:v", "1", "-vf", &scale, "-q:v", "4", "-f", "image2"]).arg(out);
    let o = run_with_timeout(&mut cmd, Duration::from_secs(60))?;
    if o.status.success() && out.exists() && std::fs::metadata(out)?.len() > 0 {
        Ok(())
    } else {
        Err(CuratorError::ToolFailed(format!("ffmpeg could not extract a frame: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or(""))))
    }
}
