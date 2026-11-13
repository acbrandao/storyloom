//! Recognised file types. Detection is by extension (fast enough for 50k-file scans);
//! files that later fail to decode are flagged by the metadata/thumbnail workers.

use crate::database::models::MediaType;

pub const PHOTO_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "heic", "heif", "hif", "tif", "tiff", "webp", "gif", "bmp", "avif",
    // RAW
    "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "orf", "rw2", "pef", "srw", "x3f", "3fr", "iiq", "erf", "mos", "raw",
];
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "mkv", "avi", "mts", "m2ts", "ts", "3gp", "3g2", "wmv", "webm", "mpg", "mpeg", "flv", "insv", "lrv",
];
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "aif", "aiff", "aifc", "ogg", "oga", "opus", "wma", "alac", "caf", "m4b"];

/// Photo formats the WebKit/Chromium webview can display directly (no preview needed).
pub const WEBVIEW_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "jpe", "png", "gif", "webp", "bmp", "avif", "heic", "heif", "tif", "tiff"];
/// Formats decoded in-process by the `image` crate.
pub const RUST_DECODABLE: &[&str] = &["jpg", "jpeg", "jpe", "png", "gif", "webp", "bmp", "tif", "tiff"];

pub fn is_raw(ext: &str) -> bool {
    PHOTO_EXTENSIONS.iter().position(|e| *e == ext).map(|i| i >= 13).unwrap_or(false)
}

pub fn classify(ext: &str) -> Option<MediaType> {
    let ext = ext.to_ascii_lowercase();
    if PHOTO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaType::Photo)
    } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaType::Video)
    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaType::Audio)
    } else {
        None
    }
}

pub fn mime_for(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "png" => "image/png",
        "heic" | "hif" => "image/heic",
        "heif" => "image/heif",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "dng" => "image/x-adobe-dng",
        "cr2" => "image/x-canon-cr2",
        "cr3" => "image/x-canon-cr3",
        "nef" => "image/x-nikon-nef",
        "arw" => "image/x-sony-arw",
        "raf" => "image/x-fuji-raf",
        "orf" => "image/x-olympus-orf",
        "rw2" => "image/x-panasonic-rw2",
        "mp4" | "insv" | "lrv" => "video/mp4",
        "mov" => "video/quicktime",
        "m4v" => "video/x-m4v",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mts" | "m2ts" | "ts" => "video/mp2t",
        "3gp" => "video/3gpp",
        "wmv" => "video/x-ms-wmv",
        "mpg" | "mpeg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" | "alac" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "aif" | "aiff" | "aifc" => "audio/aiff",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "caf" => "audio/x-caf",
        "wma" => "audio/x-ms-wma",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifies() {
        assert_eq!(classify("JPG"), Some(MediaType::Photo));
        assert_eq!(classify("cr3"), Some(MediaType::Photo));
        assert_eq!(classify("MOV"), Some(MediaType::Video));
        assert_eq!(classify("flac"), Some(MediaType::Audio));
        assert_eq!(classify("txt"), None);
        assert!(is_raw("nef"));
        assert!(!is_raw("jpg"));
        assert!(!is_raw("avif"));
    }
}
