//! Platform-specific functionality behind portable functions. Everything OS-dependent
//! (birth-time writes, native image decoders, file-manager integration) lives here so
//! Windows/Linux ports only need to fill in these functions.

#[cfg(target_os = "macos")]
mod macos;

use std::path::Path;
use std::process::Command;

/// Outcome of trying to set a file's creation ("birth") time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "status", content = "detail")]
pub enum BirthTimeSupport {
    Set,
    /// The OS or filesystem cannot store/set birth times (reported to the user, never hidden).
    Unsupported(String),
    Failed(String),
}

/// Set the creation/birth time of `path` (unix milliseconds).
pub fn set_birth_time(path: &Path, ms: i64) -> BirthTimeSupport {
    #[cfg(target_os = "macos")]
    {
        macos::set_birth_time(path, ms)
    }
    #[cfg(target_os = "windows")]
    {
        // TODO(windows): SetFileTime(lpCreationTime) via the windows-sys crate.
        let _ = (path, ms);
        BirthTimeSupport::Unsupported("setting creation time is not implemented on Windows yet".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (path, ms);
        BirthTimeSupport::Unsupported("Linux does not allow applications to set a file's birth time".into())
    }
}

/// Produce a JPEG rendition using the OS image framework (HEIC, RAW...). Returns false if unavailable.
pub fn native_image_render(src: &Path, dest: &Path, max_edge: u32) -> std::io::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        macos::sips_render(src, dest, max_edge)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (src, dest, max_edge);
        Ok(false)
    }
}

/// Displayed (orientation-applied) pixel size as the OS image framework sees it. Used for
/// HEIF, where rotation lives in the container rather than the EXIF orientation tag.
pub fn display_dimensions(src: &Path) -> Option<(i64, i64)> {
    #[cfg(target_os = "macos")]
    {
        macos::sips_dimensions(src)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = src;
        None
    }
}

/// Reveal a file in Finder / Explorer / the default file manager.
pub fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg("-R").arg(path).status()?;
    #[cfg(target_os = "windows")]
    let status = Command::new("explorer").arg(format!("/select,{}", path.display())).status()?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = Command::new("xdg-open").arg(path.parent().unwrap_or(path)).status()?;
    let _ = status;
    Ok(())
}

/// Open a file with the system's default application (read-only use).
pub fn open_with_default_app(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    Command::new("open").arg(path).status()?;
    #[cfg(target_os = "windows")]
    Command::new("cmd").args(["/C", "start", ""]).arg(path).status()?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Command::new("xdg-open").arg(path).status()?;
    Ok(())
}
