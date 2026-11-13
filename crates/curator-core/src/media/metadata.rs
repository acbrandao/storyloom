//! Metadata extraction.
//!
//! Sources, in order of preference:
//! - Photos: ExifTool (if installed; best RAW/CR3 coverage) → built-in EXIF reader (kamadak-exif).
//! - Video/audio: FFprobe (QuickTime/MP4/Matroska tags).
//! - Capture time fallbacks: date/time patterns in the filename. Filesystem dates are **never**
//!   promoted to capture times; items without one keep `capture = None` and sort provisionally.

use super::timefmt;
use crate::core::errors::{CuratorError, Result};
use crate::database::models::MediaType;
use crate::tools::Tools;
use crate::video::probe;
use chrono::DateTime;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSource {
    /// EXIF DateTimeOriginal (shutter time).
    ExifOriginal,
    /// EXIF DateTimeDigitized / CreateDate.
    ExifDigitized,
    /// EXIF DateTime (last modified by camera software).
    ExifDateTime,
    /// com.apple.quicktime.creationdate — local time with offset (iPhone, many phones).
    QuicktimeCreationDate,
    /// MP4/MOV creation_time recorded in UTC; displayed in this computer's timezone.
    ContainerUtc,
    /// MP4 creation_time from cameras known to store *local* time in the UTC field (GoPro).
    ContainerLocal,
    /// Parsed from the filename (e.g. PXL_20260714_153245123.jpg).
    Filename,
}

impl CaptureSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExifOriginal => "exif_original",
            Self::ExifDigitized => "exif_digitized",
            Self::ExifDateTime => "exif_datetime",
            Self::QuicktimeCreationDate => "quicktime_creationdate",
            Self::ContainerUtc => "container_utc",
            Self::ContainerLocal => "container_local",
            Self::Filename => "filename",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureTime {
    /// Local wall-clock time, "YYYY-MM-DDTHH:MM:SS[.fff]".
    pub local: String,
    /// UTC offset "+HH:MM" if the device recorded one.
    pub offset: Option<String>,
    pub source: CaptureSource,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExtractedMetadata {
    pub capture: Option<CaptureTime>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub duration: Option<f64>,
    pub codec: Option<String>,
    pub audio_codec: Option<String>,
    pub frame_rate: Option<f64>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub gps_altitude: Option<f64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub caption: Option<String>,
    pub keywords: Option<String>,
    pub mime_type: Option<String>,
    /// Everything the extractor returned, stored as the immutable original snapshot.
    pub raw: Value,
    pub extractor: String,
}

/// Extract metadata for one file (ExifTool batches are handled by [`extract_photos_exiftool`]).
pub fn extract(path: &Path, media_type: MediaType, tools: &Tools) -> Result<ExtractedMetadata> {
    let mut m = match media_type {
        MediaType::Photo => extract_photo_builtin(path)?,
        MediaType::Video | MediaType::Audio => extract_av(path, media_type, tools)?,
    };
    finish(path, &mut m);
    Ok(m)
}

fn finish(path: &Path, m: &mut ExtractedMetadata) {
    // HEIF: display rotation is a container property (irot/imir); the EXIF orientation tag is
    // informational only and must not be applied again. Store display dimensions instead.
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if matches!(ext.as_str(), "heic" | "heif" | "hif" | "avif") {
        m.orientation = None;
        if let Some((w, h)) = crate::platform::display_dimensions(path) {
            m.width = Some(w);
            m.height = Some(h);
        }
    }
    if m.capture.is_none() {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            m.capture = capture_from_filename(name);
        }
    }
    clean(&mut m.camera_make);
    clean(&mut m.camera_model);
    clean(&mut m.lens);
    clean(&mut m.caption);
    clean(&mut m.title);
}

fn clean(v: &mut Option<String>) {
    if let Some(s) = v {
        let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\0' || c == '"').to_string();
        *v = if t.is_empty() || t.eq_ignore_ascii_case("unknown") { None } else { Some(t) };
    }
}

// ------------------------------------------------------------------------------------------
// Photos: built-in EXIF reader
// ------------------------------------------------------------------------------------------

fn extract_photo_builtin(path: &Path) -> Result<ExtractedMetadata> {
    let mut m = ExtractedMetadata { extractor: "builtin-exif".into(), ..Default::default() };
    if let Ok(size) = imagesize::size(path) {
        m.width = Some(size.width as i64);
        m.height = Some(size.height as i64);
    }
    let mut raw = Map::new();
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    match exif::Reader::new().read_from_container(&mut reader) {
        Ok(ex) => {
            use exif::{In, Tag};
            for f in ex.fields() {
                if f.ifd_num == In::PRIMARY || f.ifd_num == In(0) {
                    raw.insert(f.tag.to_string(), Value::String(f.display_value().with_unit(&ex).to_string()));
                }
            }
            let ascii = |tag: Tag| -> Option<String> {
                ex.get_field(tag, In::PRIMARY).and_then(|f| match &f.value {
                    exif::Value::Ascii(v) => v.first().map(|b| String::from_utf8_lossy(b).trim().to_string()),
                    _ => None,
                })
            };
            let uint = |tag: Tag| ex.get_field(tag, In::PRIMARY).and_then(|f| f.value.get_uint(0));
            let rational = |tag: Tag| -> Option<f64> {
                ex.get_field(tag, In::PRIMARY).and_then(|f| match &f.value {
                    exif::Value::Rational(v) => v.first().map(|r| r.to_f64()),
                    exif::Value::SRational(v) => v.first().map(|r| r.to_f64()),
                    _ => None,
                })
            };
            for (dt_tag, off_tag, sub_tag, src) in [
                (Tag::DateTimeOriginal, Tag::OffsetTimeOriginal, Tag::SubSecTimeOriginal, CaptureSource::ExifOriginal),
                (Tag::DateTimeDigitized, Tag::OffsetTimeDigitized, Tag::SubSecTimeDigitized, CaptureSource::ExifDigitized),
                (Tag::DateTime, Tag::OffsetTime, Tag::SubSecTime, CaptureSource::ExifDateTime),
            ] {
                if let Some(c) = ascii(dt_tag).and_then(|d| build_capture(&d, ascii(sub_tag).as_deref(), ascii(off_tag).as_deref(), src)) {
                    m.capture = Some(c);
                    break;
                }
            }
            m.camera_make = ascii(Tag::Make);
            m.camera_model = ascii(Tag::Model);
            m.lens = ascii(Tag::LensModel).or_else(|| ascii(Tag::LensMake));
            m.orientation = uint(Tag::Orientation).map(|o| o as i64);
            m.caption = ascii(Tag::ImageDescription);
            if m.width.is_none() {
                m.width = uint(Tag::PixelXDimension).map(|v| v as i64);
                m.height = uint(Tag::PixelYDimension).map(|v| v as i64);
            }
            m.exposure = exposure_summary(rational(Tag::ExposureTime), rational(Tag::FNumber), uint(Tag::PhotographicSensitivity), rational(Tag::FocalLength));
            let gps_coord = |tag: Tag, ref_tag: Tag, neg: u8| -> Option<f64> {
                let f = ex.get_field(tag, In::PRIMARY)?;
                let exif::Value::Rational(v) = &f.value else { return None };
                if v.len() < 3 {
                    return None;
                }
                let mut deg = v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0;
                if let Some(r) = ex.get_field(ref_tag, In::PRIMARY) {
                    if let exif::Value::Ascii(a) = &r.value {
                        if a.first().and_then(|s| s.first()) == Some(&neg) {
                            deg = -deg;
                        }
                    }
                }
                deg.is_finite().then_some(deg)
            };
            m.gps_latitude = gps_coord(Tag::GPSLatitude, Tag::GPSLatitudeRef, b'S');
            m.gps_longitude = gps_coord(Tag::GPSLongitude, Tag::GPSLongitudeRef, b'W');
            if m.gps_latitude == Some(0.0) && m.gps_longitude == Some(0.0) {
                m.gps_latitude = None; // "null island": cameras write zeros when there is no fix
                m.gps_longitude = None;
            }
            m.gps_altitude = rational(Tag::GPSAltitude).map(|a| if uint(Tag::GPSAltitudeRef) == Some(1) { -a } else { a });
        }
        Err(exif::Error::NotFound(_)) => {}
        Err(e) => {
            raw.insert("_exifError".into(), Value::String(e.to_string()));
        }
    }
    if m.width.is_none() && raw.is_empty() {
        // Neither header nor EXIF readable: likely corrupt or an unsupported container.
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if !super::media_types::is_raw(&ext) && ext != "heic" && ext != "heif" {
            return Err(CuratorError::invalid("the image header could not be read (file may be corrupt or incomplete)"));
        }
    }
    m.raw = Value::Object(raw);
    Ok(m)
}

fn build_capture(dt: &str, subsec: Option<&str>, offset: Option<&str>, source: CaptureSource) -> Option<CaptureTime> {
    let mut base = dt.trim().to_string();
    if let Some(ss) = subsec.map(str::trim).filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())) {
        if !base.contains('.') {
            base = format!("{base}.{ss}");
        }
    }
    let parsed = timefmt::parse_local(&base).or_else(|| timefmt::parse_local(dt))?;
    use chrono::Datelike;
    if parsed.year() < 1900 {
        return None;
    }
    Some(CaptureTime {
        local: timefmt::format_local(&parsed),
        offset: offset.and_then(timefmt::parse_offset).map(|o| timefmt::format_offset(&o)),
        source,
    })
}

fn exposure_summary(time: Option<f64>, f: Option<f64>, iso: Option<u32>, focal: Option<f64>) -> Option<String> {
    let mut parts = vec![];
    if let Some(t) = time.filter(|t| *t > 0.0) {
        parts.push(if t < 1.0 { format!("1/{}s", (1.0 / t).round()) } else { format!("{}s", (t * 10.0).round() / 10.0) });
    }
    if let Some(f) = f.filter(|f| *f > 0.0) {
        parts.push(format!("f/{}", (f * 10.0).round() / 10.0));
    }
    if let Some(i) = iso {
        parts.push(format!("ISO {i}"));
    }
    if let Some(fl) = focal.filter(|v| *v > 0.0) {
        parts.push(format!("{}mm", fl.round()));
    }
    (!parts.is_empty()).then(|| parts.join("  "))
}

// ------------------------------------------------------------------------------------------
// Photos: ExifTool (batch)
// ------------------------------------------------------------------------------------------

/// Read many photos with a single ExifTool process (much faster than one process per file).
pub fn extract_photos_exiftool(exiftool: &Path, paths: &[&Path]) -> Result<Vec<Result<ExtractedMetadata>>> {
    let mut cmd = std::process::Command::new(exiftool);
    cmd.args(["-json", "-n", "-api", "LargeFileSupport=1", "-charset", "filename=utf8"]);
    for p in paths {
        cmd.arg(p);
    }
    let out = probe::run_with_timeout(&mut cmd, std::time::Duration::from_secs(120 + paths.len() as u64))?;
    let arr: Vec<Value> = serde_json::from_slice(&out.stdout).map_err(|e| CuratorError::ToolFailed(format!("exiftool output: {e}")))?;
    let mut results = vec![];
    for p in paths {
        let ps = p.to_string_lossy();
        let found = arr.iter().find(|v| v.get("SourceFile").and_then(Value::as_str) == Some(ps.as_ref()));
        results.push(match found {
            Some(v) => {
                let mut m = from_exiftool(v);
                // Fill dimensions from the header if ExifTool did not report them.
                if m.width.is_none() {
                    if let Ok(s) = imagesize::size(p) {
                        m.width = Some(s.width as i64);
                        m.height = Some(s.height as i64);
                    }
                }
                finish(p, &mut m);
                Ok(m)
            }
            None => Err(CuratorError::ToolFailed("exiftool returned no data for this file".into())),
        });
    }
    Ok(results)
}

fn from_exiftool(v: &Value) -> ExtractedMetadata {
    let s = |k: &str| v.get(k).and_then(|x| match x {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())));
    let mut m = ExtractedMetadata { extractor: "exiftool".into(), raw: v.clone(), ..Default::default() };
    for (dt, off, sub, src) in [
        ("DateTimeOriginal", "OffsetTimeOriginal", "SubSecTimeOriginal", CaptureSource::ExifOriginal),
        ("CreateDate", "OffsetTimeDigitized", "SubSecTimeDigitized", CaptureSource::ExifDigitized),
        ("ModifyDate", "OffsetTime", "SubSecTime", CaptureSource::ExifDateTime),
    ] {
        if let Some(c) = s(dt).and_then(|d| build_capture(&d, s(sub).as_deref(), s(off).as_deref(), src)) {
            m.capture = Some(c);
            break;
        }
    }
    m.camera_make = s("Make");
    m.camera_model = s("Model");
    m.lens = s("LensModel").or_else(|| s("Lens")).or_else(|| s("LensID"));
    m.orientation = f("Orientation").map(|o| o as i64);
    m.width = f("ImageWidth").or_else(|| f("ExifImageWidth")).map(|x| x as i64);
    m.height = f("ImageHeight").or_else(|| f("ExifImageHeight")).map(|x| x as i64);
    m.exposure = exposure_summary(f("ExposureTime"), f("FNumber"), f("ISO").map(|i| i as u32), f("FocalLength"));
    m.gps_latitude = f("GPSLatitude");
    m.gps_longitude = f("GPSLongitude");
    m.gps_altitude = f("GPSAltitude");
    m.caption = s("Caption-Abstract").or_else(|| s("Description")).or_else(|| s("ImageDescription"));
    m.title = s("Title").or_else(|| s("ObjectName"));
    m.keywords = match v.get("Keywords").or_else(|| v.get("Subject")) {
        Some(Value::Array(a)) => Some(a.iter().filter_map(|x| x.as_str().map(str::to_string).or_else(|| Some(x.to_string()))).collect::<Vec<_>>().join(", ")),
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    };
    m.mime_type = s("MIMEType");
    m
}

// ------------------------------------------------------------------------------------------
// Video & audio: FFprobe
// ------------------------------------------------------------------------------------------

fn extract_av(path: &Path, media_type: MediaType, tools: &Tools) -> Result<ExtractedMetadata> {
    let ffprobe = tools.require_ffprobe()?;
    let v = probe::probe(ffprobe, path)?;
    let mut m = parse_ffprobe(&v, media_type);
    m.raw = v;
    Ok(m)
}

fn tag<'a>(tags: Option<&'a Value>, key: &str) -> Option<&'a str> {
    let obj = tags?.as_object()?;
    obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).and_then(|(_, v)| v.as_str())
}

/// Interpret ffprobe JSON. Public for tests.
pub fn parse_ffprobe(v: &Value, media_type: MediaType) -> ExtractedMetadata {
    let mut m = ExtractedMetadata { extractor: "ffprobe".into(), ..Default::default() };
    let format = v.get("format");
    let ftags = format.and_then(|f| f.get("tags"));
    let streams = v.get("streams").and_then(Value::as_array).cloned().unwrap_or_default();
    let is_attached_pic = |s: &Value| s.pointer("/disposition/attached_pic").and_then(Value::as_i64) == Some(1);
    let video = streams.iter().find(|s| s.get("codec_type").and_then(Value::as_str) == Some("video") && !is_attached_pic(s));
    let audio = streams.iter().find(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio"));

    m.duration = format
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str()?.parse::<f64>().ok())
        .or_else(|| video.or(audio).and_then(|s| s.get("duration")?.as_str()?.parse().ok()))
        .filter(|d: &f64| d.is_finite() && *d > 0.0);
    m.bitrate = format.and_then(|f| f.get("bit_rate")).and_then(|b| b.as_str()?.parse().ok());

    if media_type == MediaType::Video {
        if let Some(vs) = video {
            m.codec = vs.get("codec_name").and_then(Value::as_str).map(str::to_string);
            let (mut w, mut h) = (vs.get("width").and_then(Value::as_i64), vs.get("height").and_then(Value::as_i64));
            let rotation = vs
                .get("side_data_list")
                .and_then(Value::as_array)
                .and_then(|l| l.iter().find_map(|sd| sd.get("rotation").and_then(Value::as_f64)))
                .or_else(|| tag(vs.get("tags"), "rotate").and_then(|r| r.parse().ok()))
                .unwrap_or(0.0);
            if (rotation.abs() - 90.0).abs() < 1.0 || (rotation.abs() - 270.0).abs() < 1.0 {
                std::mem::swap(&mut w, &mut h); // store display dimensions
            }
            m.width = w;
            m.height = h;
            m.frame_rate = vs
                .get("avg_frame_rate")
                .and_then(Value::as_str)
                .and_then(probe::parse_rational)
                .or_else(|| vs.get("r_frame_rate").and_then(Value::as_str).and_then(probe::parse_rational))
                .map(|f| (f * 1000.0).round() / 1000.0);
        }
    }
    if let Some(a) = audio {
        m.audio_codec = a.get("codec_name").and_then(Value::as_str).map(str::to_string);
        m.sample_rate = a.get("sample_rate").and_then(|s| s.as_str()?.parse().ok());
        if media_type == MediaType::Audio {
            m.codec = m.audio_codec.clone();
            if let Some(br) = a.get("bit_rate").and_then(|b| b.as_str()?.parse().ok()) {
                m.bitrate = Some(br);
            }
        }
    }

    // Device
    m.camera_make = tag(ftags, "com.apple.quicktime.make").or_else(|| tag(ftags, "make")).map(str::to_string);
    m.camera_model = tag(ftags, "com.apple.quicktime.model").or_else(|| tag(ftags, "model")).map(str::to_string);
    let handler = video.and_then(|vs| tag(vs.get("tags"), "handler_name")).unwrap_or("");
    let is_gopro = handler.contains("GoPro") || tag(ftags, "firmware").is_some_and(|f| f.starts_with('H') || f.starts_with("HD"));
    if is_gopro && m.camera_make.is_none() {
        m.camera_make = Some("GoPro".into());
    }
    if let Some(enc) = tag(ftags, "encoder").filter(|e| e.starts_with("DJI")) {
        m.camera_make.get_or_insert_with(|| "DJI".into());
        m.camera_model.get_or_insert_with(|| enc.to_string());
    }

    // Location
    if let Some((lat, lon, alt)) = tag(ftags, "com.apple.quicktime.location.ISO6709").or_else(|| tag(ftags, "location")).and_then(probe::parse_iso6709) {
        m.gps_latitude = Some(lat);
        m.gps_longitude = Some(lon);
        m.gps_altitude = alt;
    }

    // Capture time
    // FFprobe joins repeated tags with ';' (FFmpeg 9 writes creation_time both in the movie header
    // and as a metadata key), so every value is tried in turn.
    let each = |v: &str| v.split(';').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect::<Vec<_>>();
    if let Some(c) = tag(ftags, "com.apple.quicktime.creationdate").and_then(|v| each(v).iter().find_map(|x| parse_offset_datetime(x))) {
        m.capture = Some(c);
    } else if let Some(ct) = tag(ftags, "creation_time").or_else(|| video.or(audio).and_then(|s| tag(s.get("tags"), "creation_time"))) {
        m.capture = each(ct).iter().find_map(|x| parse_container_time(x, is_gopro));
    }

    // Descriptive tags
    m.title = tag(ftags, "title").or_else(|| tag(ftags, "com.apple.quicktime.title")).map(str::to_string);
    m.artist = tag(ftags, "artist").or_else(|| tag(ftags, "album_artist")).map(str::to_string);
    m.album = tag(ftags, "album").map(str::to_string);
    m.genre = tag(ftags, "genre").map(str::to_string);
    m.caption = tag(ftags, "comment").or_else(|| tag(ftags, "description")).or_else(|| tag(ftags, "com.apple.quicktime.description")).map(str::to_string);
    m.keywords = tag(ftags, "com.apple.quicktime.keywords").or_else(|| tag(ftags, "keywords")).map(str::to_string);
    m
}

/// "2026-07-14T15:32:45-0400" (QuickTime creationdate) → local + offset, kept as recorded.
fn parse_offset_datetime(s: &str) -> Option<CaptureTime> {
    let dt = DateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S%z")
        .or_else(|_| DateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S%.f%z"))
        .or_else(|_| DateTime::parse_from_rfc3339(s.trim()))
        .ok()?;
    Some(CaptureTime {
        local: timefmt::format_local(&dt.naive_local()),
        offset: Some(timefmt::format_offset(dt.offset())),
        source: CaptureSource::QuicktimeCreationDate,
    })
}

/// MP4/MOV `creation_time` is specified as UTC. GoPro cameras write local time into it, so
/// for GoPro the wall-clock value is kept as-is with no offset.
fn parse_container_time(s: &str, is_local_quirk: bool) -> Option<CaptureTime> {
    let dt = DateTime::parse_from_rfc3339(s.trim()).ok()?.with_timezone(&chrono::Utc);
    use chrono::Datelike;
    if dt.year() < 1971 {
        return None; // unset QuickTime epoch (1904/1970) values
    }
    if is_local_quirk {
        return Some(CaptureTime { local: timefmt::format_local(&dt.naive_utc()), offset: None, source: CaptureSource::ContainerLocal });
    }
    let (local, offset) = timefmt::utc_to_system_local(&dt);
    Some(CaptureTime { local, offset: Some(offset), source: CaptureSource::ContainerUtc })
}

// ------------------------------------------------------------------------------------------
// Filename patterns
// ------------------------------------------------------------------------------------------

static FILENAME_DT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9])((?:19|20)\d{2})[-_.]?(0[1-9]|1[0-2])[-_.]?(0[1-9]|[12]\d|3[01])(?:\s+at\s+|[ _T-]{0,3})([01]\d|2[0-3])[-_.:h]?([0-5]\d)[-_.:m]?([0-5]\d)(\d{0,3})")
        .expect("valid regex")
});

/// Recognise common camera/phone naming schemes: IMG_20260714_153245.jpg, PXL_20260714_153245123.mp4,
/// "2026-07-14 15.32.45.jpg", "Screen Recording 2026-07-14 at 15.32.45.mov", VID-20260714-WA0001 (date only: ignored).
pub fn capture_from_filename(name: &str) -> Option<CaptureTime> {
    let c = FILENAME_DT.captures(name)?;
    let ms = c.get(7).map(|m| m.as_str()).filter(|s| s.len() == 3);
    let s = format!("{}-{}-{}T{}:{}:{}{}", &c[1], &c[2], &c[3], &c[4], &c[5], &c[6], ms.map(|m| format!(".{m}")).unwrap_or_default());
    let dt = timefmt::parse_local(&s)?;
    Some(CaptureTime { local: timefmt::format_local(&dt), offset: None, source: CaptureSource::Filename })
}

/// JSON helper for UI: describe a capture source in friendly words.
pub fn describe_source(src: &str) -> &'static str {
    match src {
        "exif_original" => "Camera (photo taken)",
        "exif_digitized" => "Camera (digitized)",
        "exif_datetime" => "Camera (file date)",
        "quicktime_creationdate" => "Device (local time)",
        "container_utc" => "Video file (UTC, shown in your timezone)",
        "container_local" => "Video file (camera local time)",
        "filename" => "File name",
        _ => "Unknown",
    }
}

pub fn summarize_for_log(m: &ExtractedMetadata) -> Value {
    json!({"capture": m.capture, "w": m.width, "h": m.height, "dur": m.duration})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_patterns() {
        let c = capture_from_filename("IMG_20260714_153245.jpg").unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        let c = capture_from_filename("PXL_20260714_153245123.MP.jpg").unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45.123");
        let c = capture_from_filename("Screen Recording 2026-07-14 at 15.32.45.mov").unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        let c = capture_from_filename("2026-07-14 15.32.45.jpg").unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        assert!(capture_from_filename("GOPR0042.MP4").is_none());
        assert!(capture_from_filename("VID-20260714-WA0001.mp4").is_none());
        assert!(capture_from_filename("IMG_20261340_250000.jpg").is_none());
    }

    #[test]
    fn quicktime_creationdate_keeps_local_and_offset() {
        let v = json!({"format": {"duration": "12.5", "tags": {
            "creation_time": "2026-07-14T19:32:45.000000Z",
            "com.apple.quicktime.creationdate": "2026-07-14T15:32:45-0400",
            "com.apple.quicktime.make": "Apple", "com.apple.quicktime.model": "iPhone 16 Pro",
            "com.apple.quicktime.location.ISO6709": "+40.4329-073.9876+005.000/"
        }}, "streams": [{"codec_type": "video", "codec_name": "hevc", "width": 3840, "height": 2160,
            "avg_frame_rate": "30/1", "side_data_list": [{"rotation": -90}]}]});
        let m = parse_ffprobe(&v, MediaType::Video);
        let c = m.capture.unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        assert_eq!(c.offset.as_deref(), Some("-04:00"));
        assert_eq!(c.source, CaptureSource::QuicktimeCreationDate);
        assert_eq!((m.width, m.height), (Some(2160), Some(3840)));
        assert_eq!(m.camera_model.as_deref(), Some("iPhone 16 Pro"));
        assert!(m.gps_latitude.is_some());
        assert_eq!(m.duration, Some(12.5));
    }

    #[test]
    fn gopro_creation_time_is_local() {
        let v = json!({"format": {"duration": "750.0", "tags": {"creation_time": "2026-07-14T15:32:45.000000Z", "firmware": "H23.01.01.10.00"}},
            "streams": [{"codec_type": "video", "codec_name": "h264", "width": 5312, "height": 2988, "avg_frame_rate": "60000/1001",
            "tags": {"handler_name": "\u{0010}GoPro AVC  "}}]});
        let m = parse_ffprobe(&v, MediaType::Video);
        let c = m.capture.unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        assert_eq!(c.offset, None);
        assert_eq!(c.source, CaptureSource::ContainerLocal);
        assert_eq!(m.camera_make.as_deref(), Some("GoPro"));
        assert!((m.frame_rate.unwrap() - 59.94).abs() < 0.01);
    }

    #[test]
    fn repeated_creation_time_values_from_ffmpeg9() {
        // FFprobe 9 reports duplicated tags joined with ';'.
        let v = json!({"format": {"tags": {"creation_time": "2026-07-14T15:32:45.000000Z;2026-07-14T15:32:45.000000Z", "firmware": "H23.01.01.10.00"}}, "streams": []});
        let c = parse_ffprobe(&v, MediaType::Video).capture.unwrap();
        assert_eq!(c.local, "2026-07-14T15:32:45");
        assert_eq!(c.source, CaptureSource::ContainerLocal);
        let v = json!({"format": {"tags": {"creation_time": "garbage;2026-07-14T19:32:45Z"}}, "streams": []});
        assert!(parse_ffprobe(&v, MediaType::Video).capture.is_some(), "first valid value wins");
        let v = json!({"format": {"tags": {"com.apple.quicktime.creationdate": "2026-07-14T15:32:45-0400;2026-07-14T15:32:45-0400"}}, "streams": []});
        assert_eq!(parse_ffprobe(&v, MediaType::Video).capture.unwrap().offset.as_deref(), Some("-04:00"));
    }

    #[test]
    fn utc_creation_time_is_an_exact_instant() {
        let v = json!({"format": {"tags": {"creation_time": "2026-07-14T19:32:45.000000Z"}}, "streams": []});
        let m = parse_ffprobe(&v, MediaType::Video);
        let c = m.capture.unwrap();
        assert_eq!(c.source, CaptureSource::ContainerUtc);
        // Re-assemble the instant and compare to the original UTC value.
        let off = timefmt::parse_offset(c.offset.as_deref().unwrap()).unwrap();
        let local = timefmt::parse_local(&c.local).unwrap();
        let instant = local - chrono::Duration::seconds(off.local_minus_utc() as i64);
        assert_eq!(instant.format("%Y-%m-%dT%H:%M:%S").to_string(), "2026-07-14T19:32:45");
    }

    #[test]
    fn audio_tags() {
        let v = json!({"format": {"duration": "201.3", "bit_rate": "320000", "tags": {"title": "Ocean", "artist": "Waves", "album": "Summer", "genre": "Ambient"}},
            "streams": [{"codec_type": "audio", "codec_name": "mp3", "sample_rate": "44100", "bit_rate": "320000"},
                        {"codec_type": "video", "codec_name": "mjpeg", "disposition": {"attached_pic": 1}}]});
        let m = parse_ffprobe(&v, MediaType::Audio);
        assert_eq!(m.title.as_deref(), Some("Ocean"));
        assert_eq!(m.codec.as_deref(), Some("mp3"));
        assert_eq!(m.sample_rate, Some(44100));
        assert!(m.width.is_none());
        assert!(m.capture.is_none());
    }
}
