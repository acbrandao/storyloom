//! FFmpeg clip extraction: keyframe lookup, export presets, command construction and a
//! cancellable runner with progress.

use crate::core::errors::{CuratorError, Result};
use crate::video::probe;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportPreset {
    /// Stream copy: original quality, no re-encoding. The clip starts on the keyframe at or
    /// before the in-point (reported to the user).
    Lossless,
    /// Re-encode with the source's codec family (H.264 → H.264, HEVC → HEVC incl. 10-bit) at
    /// visually lossless quality. Frame-accurate.
    Exact,
    /// Re-encode to 8-bit H.264 + AAC MP4 for maximum compatibility. Frame-accurate.
    Compatible,
}

impl ExportPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lossless => "lossless",
            Self::Exact => "exact",
            Self::Compatible => "compatible",
        }
    }
    pub fn method(&self) -> &'static str {
        match self {
            Self::Lossless => "stream_copy",
            _ => "reencode",
        }
    }
}

/// Facts about the source needed to plan an export.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub duration: f64,
    pub start_time: f64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub pix_fmt: Option<String>,
    pub bit_depth_10: bool,
    pub hdr: bool,
    pub fps: f64,
    pub container_ext: String,
}

pub fn source_info(ffprobe: &Path, src: &Path) -> Result<SourceInfo> {
    let v = probe::probe(ffprobe, src)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s.pointer("/disposition/attached_pic").and_then(Value::as_i64) != Some(1))
        .ok_or_else(|| CuratorError::invalid("this file has no video stream"))?;
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    let pix_fmt = video["pix_fmt"].as_str().map(str::to_string);
    let transfer = video["color_transfer"].as_str().unwrap_or("");
    Ok(SourceInfo {
        duration: v.pointer("/format/duration").and_then(|d| d.as_str()?.parse().ok()).unwrap_or(0.0),
        start_time: v.pointer("/format/start_time").and_then(|d| d.as_str()?.parse().ok()).unwrap_or(0.0),
        video_codec: video["codec_name"].as_str().unwrap_or("").to_string(),
        audio_codec: audio.and_then(|a| a["codec_name"].as_str()).map(str::to_string),
        bit_depth_10: pix_fmt.as_deref().is_some_and(|p| p.contains("10") || p.contains("12")),
        hdr: matches!(transfer, "smpte2084" | "arib-std-b67"),
        pix_fmt,
        fps: video["avg_frame_rate"].as_str().and_then(probe::parse_rational).or_else(|| video["r_frame_rate"].as_str().and_then(probe::parse_rational)).unwrap_or(30.0),
        container_ext: src.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default(),
    })
}

/// Latest keyframe at or before `t` (seconds from the start of the video).
pub fn keyframe_at_or_before(ffprobe: &Path, src: &Path, t: f64, start_time: f64) -> Result<f64> {
    if t <= 0.0 {
        return Ok(0.0);
    }
    let from = (t - 20.0).max(0.0) + start_time;
    let to = t + start_time + 0.01;
    let out = probe::run_with_timeout(
        Command::new(ffprobe)
            .args(["-v", "error", "-select_streams", "v:0", "-skip_frame", "nokey", "-show_entries", "frame=pts_time,best_effort_timestamp_time", "-of", "csv=p=0"])
            .args(["-read_intervals", &format!("{from:.3}%{to:.3}")])
            .arg(src),
        std::time::Duration::from_secs(60),
    )?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut best: Option<f64> = None;
    for line in text.lines() {
        for field in line.split(',') {
            if let Ok(pts) = field.trim().parse::<f64>() {
                let rel = pts - start_time;
                if rel <= t + 1e-3 && best.is_none_or(|b| rel > b) {
                    best = Some(rel.max(0.0));
                }
                break;
            }
        }
    }
    // Searching a window that starts at 0 and finding nothing means the first keyframe is at 0.
    Ok(best.unwrap_or(if from <= start_time { 0.0 } else { (t - 20.0).max(0.0) }))
}

/// Output file extension for a preset.
pub fn output_ext(preset: ExportPreset, info: &SourceInfo) -> &'static str {
    match preset {
        ExportPreset::Lossless => match info.container_ext.as_str() {
            "mov" => "mov",
            "mkv" => "mkv",
            "m4v" => "m4v",
            _ => "mp4",
        },
        ExportPreset::Exact => if info.container_ext == "mov" { "mov" } else { "mp4" },
        ExportPreset::Compatible => "mp4",
    }
}

/// Human-readable notes about what a preset will do to this source (shown before export).
pub fn preset_notes(preset: ExportPreset, info: &SourceInfo, requested_in: f64, actual_in: f64) -> Vec<String> {
    let mut notes = vec![];
    match preset {
        ExportPreset::Lossless => {
            let early = requested_in - actual_in;
            if early > 0.001 {
                notes.push(format!("Starts {early:.2} s earlier, at the nearest keyframe (lossless cuts can only start on keyframes)"));
            }
        }
        ExportPreset::Exact => notes.push(format!("Re-encoded ({}) at visually lossless quality for a frame-accurate cut", if info.video_codec == "hevc" { "HEVC" } else { "H.264" })),
        ExportPreset::Compatible => {
            notes.push("Re-encoded to H.264 for maximum compatibility; frame-accurate".into());
            if info.hdr || info.bit_depth_10 {
                notes.push("Converted to 8-bit SDR: HDR / 10-bit colour is not kept with this preset".into());
            }
        }
    }
    notes
}

/// Metadata written into the clip.
pub struct ClipMetadata {
    /// UTC instant for `creation_time` ("2026-07-14T19:37:15.000000Z").
    pub creation_time: Option<String>,
    /// QuickTime local time with offset ("2026-07-14T15:37:15-0400").
    pub apple_creationdate: Option<String>,
    pub comment: String,
}

pub fn build_command(ffmpeg: &Path, src: &Path, out: &Path, preset: ExportPreset, info: &SourceInfo, start: f64, duration: f64, meta: &ClipMetadata) -> Command {
    let mut c = Command::new(ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-v", "error", "-y", "-progress", "pipe:1", "-nostats"]);
    // Input seeking: with stream copy it lands on the keyframe we computed; when re-encoding,
    // FFmpeg decodes from the prior keyframe and discards frames, so the cut is frame-accurate.
    c.args(["-ss", &format!("{start:.6}")]).arg("-i").arg(src).args(["-t", &format!("{duration:.6}")]);
    c.args(["-map", "0:v:0", "-map", "0:a?", "-map_metadata", "0"]);
    let audio_copyable = |container: &str| match info.audio_codec.as_deref() {
        Some("aac") | Some("alac") => true,
        Some(a) if a.starts_with("pcm_") => container == "mov",
        None => true,
        _ => false,
    };
    match preset {
        ExportPreset::Lossless => {
            c.args(["-c", "copy", "-avoid_negative_ts", "make_zero"]);
        }
        ExportPreset::Exact => {
            let container = output_ext(preset, info);
            if info.video_codec == "hevc" {
                c.args(["-c:v", "libx265", "-crf", "18", "-preset", "medium", "-tag:v", "hvc1", "-x265-params", "log-level=error"]);
            } else {
                c.args(["-c:v", "libx264", "-crf", "16", "-preset", "medium"]);
            }
            if let Some(p) = &info.pix_fmt {
                c.args(["-pix_fmt", p]);
            }
            if audio_copyable(container) {
                c.args(["-c:a", "copy"]);
            } else {
                c.args(["-c:a", "aac", "-b:a", "320k"]);
            }
        }
        ExportPreset::Compatible => {
            c.args(["-c:v", "libx264", "-crf", "18", "-preset", "medium", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "256k"]);
        }
    }
    if let Some(ct) = &meta.creation_time {
        c.args(["-metadata", &format!("creation_time={ct}")]);
    }
    if let Some(cd) = &meta.apple_creationdate {
        c.args(["-metadata", &format!("com.apple.quicktime.creationdate={cd}")]);
    }
    c.args(["-metadata", &format!("comment={}", meta.comment)]);
    let ext = out.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if matches!(ext.as_str(), "mp4" | "mov" | "m4v") {
        c.args(["-movflags", "+faststart+use_metadata_tags"]);
        c.args(["-f", if ext == "mov" { "mov" } else { "mp4" }]);
    }
    c.arg(out);
    c
}

/// Run FFmpeg, reporting progress (0..1). Kills the process on cancellation.
pub fn run(mut cmd: Command, duration: f64, cancel: &CancellationToken, mut progress: impl FnMut(f64)) -> Result<()> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stderr = child.stderr.take();
    let err_h = std::thread::spawn(move || {
        let mut v = String::new();
        if let Some(mut s) = stderr {
            let _ = std::io::Read::read_to_string(&mut s, &mut v);
        }
        v
    });
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel::<f64>();
    let out_h = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(|l| l.ok()) {
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                let _ = tx.send(us / 1_000_000.0);
            }
        }
    });
    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CuratorError::Cancelled);
        }
        while let Ok(t) = rx.try_recv() {
            if duration > 0.0 {
                progress((t / duration).clamp(0.0, 1.0));
            }
        }
        if let Some(s) = child.try_wait()? {
            break s;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let _ = out_h.join();
    let err = err_h.join().unwrap_or_default();
    if !status.success() {
        let last = err.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("unknown error");
        return Err(CuratorError::ToolFailed(format!("FFmpeg could not export the clip: {last}")));
    }
    Ok(())
}
