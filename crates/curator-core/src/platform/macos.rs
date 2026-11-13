use super::BirthTimeSupport;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};

#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_CRTIME: u32 = 0x0000_0200;
const FSOPT_NOFOLLOW: u32 = 0x0000_0001;

extern "C" {
    fn setattrlist(path: *const libc::c_char, attr_list: *mut libc::c_void, attr_buf: *mut libc::c_void, size: libc::size_t, options: u32) -> libc::c_int;
}

/// Uses `setattrlist(ATTR_CMN_CRTIME)`, supported on APFS and HFS+. FAT/exFAT volumes
/// do support a creation time; network volumes may refuse, which we report.
pub fn set_birth_time(path: &Path, ms: i64) -> BirthTimeSupport {
    let Ok(cpath) = CString::new(path.as_os_str().as_bytes()) else {
        return BirthTimeSupport::Failed("path contains a NUL byte".into());
    };
    let mut attrs = AttrList { bitmapcount: ATTR_BIT_MAP_COUNT, reserved: 0, commonattr: ATTR_CMN_CRTIME, volattr: 0, dirattr: 0, fileattr: 0, forkattr: 0 };
    let mut ts = libc::timespec { tv_sec: ms.div_euclid(1000) as libc::time_t, tv_nsec: (ms.rem_euclid(1000) * 1_000_000) as libc::c_long };
    let rc = unsafe {
        setattrlist(
            cpath.as_ptr(),
            &mut attrs as *mut AttrList as *mut libc::c_void,
            &mut ts as *mut libc::timespec as *mut libc::c_void,
            std::mem::size_of::<libc::timespec>(),
            FSOPT_NOFOLLOW,
        )
    };
    if rc == 0 {
        BirthTimeSupport::Set
    } else {
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::ENOTSUP) | Some(libc::EINVAL) => BirthTimeSupport::Unsupported(format!("this volume does not support creation times ({err})")),
            _ => BirthTimeSupport::Failed(err.to_string()),
        }
    }
}

/// Render through macOS ImageIO using `sips` (handles HEIC and most camera RAW formats).
/// sips keeps the EXIF orientation tag, which the webview honours when displaying.
pub fn sips_render(src: &Path, dest: &Path, max_edge: u32) -> std::io::Result<bool> {
    let status = Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "80", "-Z", &max_edge.to_string()])
        .arg(src)
        .arg("--out")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success() && dest.exists())
}

/// (width, height) after ImageIO applies container/EXIF orientation.
pub fn sips_dimensions(src: &Path) -> Option<(i64, i64)> {
    let out = Command::new("/usr/bin/sips").args(["-g", "pixelWidth", "-g", "pixelHeight"]).arg(src).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let get = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key)?.trim().parse::<i64>().ok());
    Some((get("pixelWidth:")?, get("pixelHeight:")?))
}
