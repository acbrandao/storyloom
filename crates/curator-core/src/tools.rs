//! Discovery of external command-line tools. GUI apps on macOS do not inherit the shell
//! PATH, so common install locations (Homebrew, MacPorts) are checked explicitly.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub exiftool: Option<PathBuf>,
    pub ffmpeg_version: Option<String>,
    pub exiftool_version: Option<String>,
}

const SEARCH_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin", "/usr/bin", "/bin"];

fn find(name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let env_key = format!("STORYLOOM_{}", name.to_ascii_uppercase());
    if let Ok(p) = std::env::var(&env_key) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let path_dirs = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    extra_dirs
        .iter()
        .cloned()
        .chain(SEARCH_DIRS.iter().map(PathBuf::from))
        .chain(path_dirs)
        .map(|d| d.join(&exe))
        .find(|p| p.is_file())
}

fn first_line(cmd: &Path, arg: &str) -> Option<String> {
    let out = Command::new(cmd).arg(arg).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().next().map(|s| s.trim().to_string())
}

impl Tools {
    /// `extra_dirs`: e.g. the app bundle's resources folder when tools are bundled.
    pub fn discover(extra_dirs: &[PathBuf]) -> Self {
        let ffmpeg = find("ffmpeg", extra_dirs);
        let ffprobe = find("ffprobe", extra_dirs);
        let exiftool = find("exiftool", extra_dirs);
        let ffmpeg_version = ffmpeg.as_deref().and_then(|p| first_line(p, "-version")).map(|l| {
            l.split_whitespace().nth(2).unwrap_or(&l).to_string()
        });
        let exiftool_version = exiftool.as_deref().and_then(|p| first_line(p, "-ver"));
        let t = Tools { ffmpeg, ffprobe, exiftool, ffmpeg_version, exiftool_version };
        tracing::info!(?t, "external tools discovered");
        t
    }

    pub fn require_ffprobe(&self) -> crate::Result<&Path> {
        self.ffprobe.as_deref().ok_or_else(|| {
            crate::CuratorError::ToolMissing("FFprobe".into(), "Install FFmpeg (e.g. `brew install ffmpeg`) to read video and audio details.".into())
        })
    }
    pub fn require_ffmpeg(&self) -> crate::Result<&Path> {
        self.ffmpeg.as_deref().ok_or_else(|| {
            crate::CuratorError::ToolMissing("FFmpeg".into(), "Install FFmpeg (e.g. `brew install ffmpeg`) for video thumbnails and clips.".into())
        })
    }
}
