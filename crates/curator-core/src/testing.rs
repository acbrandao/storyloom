//! Sample-media generation for automated tests and the sample library
//! (`cargo run -p curator-core --example make_sample_library -- <dir>`).
//! Produces real, decodable files with known capture metadata.

use image::{ImageBuffer, Rgb};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct ExifSpec {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    /// "YYYY:MM:DD HH:MM:SS"
    pub datetime_original: Option<String>,
    /// "+HH:MM"
    pub offset_original: Option<String>,
    pub subsec: Option<String>,
    pub orientation: Option<u16>,
    pub gps: Option<(f64, f64, f64)>,
    pub exposure: Option<(u32, u32)>,
    pub fnumber: Option<(u32, u32)>,
    pub iso: Option<u16>,
    pub focal: Option<(u32, u32)>,
}

const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const BYTE: u16 = 1;

struct Entry {
    tag: u16,
    typ: u16,
    count: u32,
    data: Vec<u8>,
}

fn ascii(tag: u16, s: &str) -> Entry {
    let mut d = s.as_bytes().to_vec();
    d.push(0);
    Entry { tag, typ: ASCII, count: d.len() as u32, data: d }
}
fn short(tag: u16, v: u16) -> Entry {
    Entry { tag, typ: SHORT, count: 1, data: v.to_le_bytes().to_vec() }
}
fn long(tag: u16, v: u32) -> Entry {
    Entry { tag, typ: LONG, count: 1, data: v.to_le_bytes().to_vec() }
}
fn rationals(tag: u16, v: &[(u32, u32)]) -> Entry {
    let mut d = vec![];
    for (n, den) in v {
        d.extend(n.to_le_bytes());
        d.extend(den.to_le_bytes());
    }
    Entry { tag, typ: RATIONAL, count: v.len() as u32, data: d }
}

fn ifd_size(entries: &[Entry]) -> u32 {
    let mut s = 2 + 12 * entries.len() as u32 + 4;
    for e in entries {
        if e.data.len() > 4 {
            s += (e.data.len() as u32 + 1) & !1;
        }
    }
    s
}

fn write_ifd(out: &mut Vec<u8>, entries: &mut [Entry], start: u32) {
    entries.sort_by_key(|e| e.tag);
    let mut data_off = start + 2 + 12 * entries.len() as u32 + 4;
    let mut data = vec![];
    out.extend((entries.len() as u16).to_le_bytes());
    for e in entries.iter() {
        out.extend(e.tag.to_le_bytes());
        out.extend(e.typ.to_le_bytes());
        out.extend(e.count.to_le_bytes());
        if e.data.len() <= 4 {
            let mut v = e.data.clone();
            v.resize(4, 0);
            out.extend(v);
        } else {
            out.extend(data_off.to_le_bytes());
            data.extend(&e.data);
            if e.data.len() % 2 == 1 {
                data.push(0);
            }
            data_off += (e.data.len() as u32 + 1) & !1;
        }
    }
    out.extend(0u32.to_le_bytes());
    out.extend(data);
}

fn dms(v: f64) -> [(u32, u32); 3] {
    let v = v.abs();
    let d = v.floor();
    let m = ((v - d) * 60.0).floor();
    let s = ((v - d) * 60.0 - m) * 60.0;
    [(d as u32, 1), (m as u32, 1), ((s * 1000.0).round() as u32, 1000)]
}

/// Build a TIFF/EXIF blob (little-endian) for an APP1 segment.
pub fn build_exif(spec: &ExifSpec) -> Vec<u8> {
    let mut ifd0 = vec![];
    if let Some(m) = &spec.make {
        ifd0.push(ascii(0x010F, m));
    }
    if let Some(m) = &spec.model {
        ifd0.push(ascii(0x0110, m));
    }
    if let Some(o) = spec.orientation {
        ifd0.push(short(0x0112, o));
    }
    let mut exif = vec![];
    if let Some(d) = &spec.datetime_original {
        exif.push(ascii(0x9003, d));
        exif.push(ascii(0x9004, d));
    }
    if let Some(o) = &spec.offset_original {
        exif.push(ascii(0x9011, o));
    }
    if let Some(s) = &spec.subsec {
        exif.push(ascii(0x9291, s));
    }
    if let Some(e) = spec.exposure {
        exif.push(rationals(0x829A, &[e]));
    }
    if let Some(f) = spec.fnumber {
        exif.push(rationals(0x829D, &[f]));
    }
    if let Some(i) = spec.iso {
        exif.push(short(0x8827, i));
    }
    if let Some(f) = spec.focal {
        exif.push(rationals(0x920A, &[f]));
    }
    if let Some(l) = &spec.lens {
        exif.push(ascii(0xA434, l));
    }
    let mut gps = vec![];
    if let Some((lat, lon, alt)) = spec.gps {
        gps.push(ascii(0x0001, if lat < 0.0 { "S" } else { "N" }));
        gps.push(rationals(0x0002, &dms(lat)));
        gps.push(ascii(0x0003, if lon < 0.0 { "W" } else { "E" }));
        gps.push(rationals(0x0004, &dms(lon)));
        gps.push(Entry { tag: 0x0005, typ: BYTE, count: 1, data: vec![if alt < 0.0 { 1 } else { 0 }] });
        gps.push(rationals(0x0006, &[((alt.abs() * 100.0) as u32, 100)]));
    }
    // Pointers have fixed size, so compute layout with placeholders first.
    let has_exif = !exif.is_empty();
    let has_gps = !gps.is_empty();
    if has_exif {
        ifd0.push(long(0x8769, 0));
    }
    if has_gps {
        ifd0.push(long(0x8825, 0));
    }
    let ifd0_off = 8u32;
    let exif_off = ifd0_off + ifd_size(&ifd0);
    let gps_off = exif_off + if has_exif { ifd_size(&exif) } else { 0 };
    for e in ifd0.iter_mut() {
        if e.tag == 0x8769 {
            e.data = exif_off.to_le_bytes().to_vec();
        }
        if e.tag == 0x8825 {
            e.data = gps_off.to_le_bytes().to_vec();
        }
    }
    let mut out = b"II".to_vec();
    out.extend(42u16.to_le_bytes());
    out.extend(ifd0_off.to_le_bytes());
    write_ifd(&mut out, &mut ifd0, ifd0_off);
    if has_exif {
        write_ifd(&mut out, &mut exif, exif_off);
    }
    if has_gps {
        write_ifd(&mut out, &mut gps, gps_off);
    }
    out
}

/// Write a JPEG (gradient with a hue derived from `seed`) with the given EXIF.
pub fn write_jpeg(path: &Path, w: u32, h: u32, seed: u32, spec: Option<&ExifSpec>) -> std::io::Result<()> {
    let (r0, g0, b0) = ((seed * 67) % 255, (seed * 131) % 255, (seed * 29) % 255);
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
        let fx = x as f32 / w as f32;
        let fy = y as f32 / h as f32;
        // Mark the top-left corner so orientation handling is visible.
        if x < w / 8 && y < h / 8 {
            return Rgb([255, 255, 255]);
        }
        Rgb([((r0 as f32) * (1.0 - fx) + 255.0 * fx * 0.3) as u8, ((g0 as f32) * (1.0 - fy) + 120.0 * fy) as u8, (b0 as f32 * 0.8 + 50.0 * fx) as u8])
    });
    let mut jpeg = vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&img).map_err(std::io::Error::other)?;
    let mut out = jpeg[..2].to_vec(); // SOI
    if let Some(spec) = spec {
        let tiff = build_exif(spec);
        let len = (tiff.len() + 8) as u16;
        out.extend([0xFF, 0xE1]);
        out.extend(len.to_be_bytes());
        out.extend(b"Exif\0\0");
        out.extend(tiff);
    }
    out.extend(&jpeg[2..]);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, out)
}

/// Generate a test video with FFmpeg. `creation_time` is the container UTC field;
/// `apple_creationdate` the QuickTime local+offset field. Returns false if FFmpeg is missing.
pub fn write_video(
    ffmpeg: &Path,
    path: &Path,
    seconds: f64,
    creation_time: Option<&str>,
    apple_creationdate: Option<&str>,
    extra: &[(&str, &str)],
    handler: Option<&str>,
) -> std::io::Result<bool> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("testsrc2=size=640x360:rate=30:duration={seconds}")]);
    cmd.args(["-f", "lavfi", "-i", &format!("sine=frequency=440:duration={seconds}")]);
    cmd.args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "30", "-c:a", "aac", "-shortest"]);
    if let Some(ct) = creation_time {
        cmd.args(["-metadata", &format!("creation_time={ct}")]);
    }
    if let Some(cd) = apple_creationdate {
        cmd.args(["-metadata", &format!("com.apple.quicktime.creationdate={cd}")]);
    }
    for (k, v) in extra {
        cmd.args(["-metadata", &format!("{k}={v}")]);
    }
    if let Some(h) = handler {
        cmd.args(["-metadata:s:v:0", &format!("handler_name={h}")]);
    }
    cmd.args(["-movflags", "use_metadata_tags"]).arg(path);
    Ok(cmd.status()?.success())
}

pub fn write_audio(ffmpeg: &Path, path: &Path, seconds: f64, tags: &[(&str, &str)]) -> std::io::Result<bool> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("sine=frequency=330:duration={seconds}")]);
    for (k, v) in tags {
        cmd.args(["-metadata", &format!("{k}={v}")]);
    }
    cmd.arg(path);
    Ok(cmd.status()?.success())
}

/// A small but realistic library: phones, GoPro, drone, DSLR, audio, duplicates, broken files.
/// Returns the list of created files.
pub fn make_sample_library(root: &Path, ffmpeg: Option<&Path>) -> std::io::Result<Vec<PathBuf>> {
    let mut created = vec![];
    let trips: &[(&str, &str, &str, (f64, f64, f64), &str)] = &[
        ("2026/Argentina-Brazil/Buenos-Aires", "2026:03:02", "-03:00", (-34.6037, -58.3816, 25.0), "Apple|iPhone 16 Pro"),
        ("2026/Argentina-Brazil/Florianopolis", "2026:03:09", "-03:00", (-27.5954, -48.5480, 3.0), "Apple|iPhone 16 Pro"),
        ("2026/Argentina-Brazil/Rio-de-Janeiro", "2026:03:15", "-03:00", (-22.9068, -43.1729, 10.0), "SONY|ILCE-7M4"),
        ("2026/Windsurfing/Sandy-Hook", "2026:07:14", "-04:00", (40.4329, -73.9876, 2.0), "Canon|Canon EOS R6m2"),
        ("2026/Windsurfing/Bonaire", "2026:05:20", "-04:00", (12.1443, -68.2655, 1.0), "FUJIFILM|X-T5"),
        ("2025/Family/Birthdays", "2025:11:08", "-05:00", (40.7128, -74.0060, 15.0), "Google|Pixel 9"),
    ];
    let mut seed = 1;
    for (dir, date, offset, gps, cam) in trips {
        let (make, model) = cam.split_once('|').unwrap();
        for i in 0..6 {
            let minute = 10 + i * 7;
            let hour = 9 + i;
            let spec = ExifSpec {
                make: Some(make.into()),
                model: Some(model.into()),
                lens: Some("24-70mm F2.8".into()),
                datetime_original: Some(format!("{date} {hour:02}:{minute:02}:{:02}", (i * 13) % 60)),
                offset_original: if make == "Canon" { None } else { Some(offset.to_string()) },
                subsec: if i % 2 == 0 { Some("250".into()) } else { None },
                orientation: Some(if i == 3 { 6 } else { 1 }),
                gps: if make == "SONY" { None } else { Some((gps.0 + i as f64 * 0.001, gps.1 + i as f64 * 0.001, gps.2)) },
                exposure: Some((1, 250 * (i as u32 + 1))),
                fnumber: Some((28 + i as u32 * 10, 10)),
                iso: Some(100 * (i as u16 + 1)),
                focal: Some((24 + i as u32 * 8, 1)),
            };
            let name = format!("{}_{:04}.JPG", if make == "Apple" { "IMG" } else { "DSC" }, seed);
            let p = root.join(dir).join("Photos").join(name);
            let (w, h) = if i == 3 { (1200, 800) } else if i % 2 == 0 { (1600, 1067) } else { (1067, 1600) };
            write_jpeg(&p, w, h, seed, Some(&spec))?;
            created.push(p);
            seed += 1;
        }
    }
    // Photo without EXIF but with a dated filename, and one with no date information at all.
    let p = root.join("Downloads/Screenshot 2026-07-14 at 15.32.45.png");
    std::fs::create_dir_all(p.parent().unwrap())?;
    ImageBuffer::<Rgb<u8>, Vec<u8>>::from_fn(800, 500, |x, _| Rgb([(x % 255) as u8, 90, 160])).save(&p).map_err(std::io::Error::other)?;
    created.push(p);
    let p = root.join("Downloads/wallpaper.jpg");
    write_jpeg(&p, 1280, 720, 99, None)?;
    created.push(p);

    // Exact duplicate of an existing photo in another folder.
    let first = created[0].clone();
    let dup = root.join("Backup/IMG_0001 copy.JPG");
    std::fs::create_dir_all(dup.parent().unwrap())?;
    std::fs::copy(&first, &dup)?;
    created.push(dup);

    // Broken inputs: truncated JPEG, empty file, unsupported type.
    let broken = root.join("Problems/truncated.jpg");
    std::fs::create_dir_all(broken.parent().unwrap())?;
    std::fs::write(&broken, &std::fs::read(&first)?[..200])?;
    created.push(broken);
    std::fs::write(root.join("Problems/empty.mp4"), b"")?;
    std::fs::write(root.join("Problems/notes.xyz"), b"not media")?;

    if let Some(ff) = ffmpeg {
        let v = root.join("2026/Windsurfing/Sandy-Hook/Videos/GX010042.MP4");
        if write_video(ff, &v, 12.0, Some("2026-07-14T15:32:45.000000Z"), None, &[("firmware", "H23.01.01.10.00")], Some("GoPro AVC"))? {
            created.push(v);
        }
        let v = root.join("2026/Argentina-Brazil/Rio-de-Janeiro/Videos/IMG_4821.MOV");
        if write_video(ff, &v, 8.0, Some("2026-03-15T19:20:00.000000Z"), Some("2026-03-15T16:20:00-0300"), &[("com.apple.quicktime.make", "Apple"), ("com.apple.quicktime.model", "iPhone 16 Pro"), ("com.apple.quicktime.location.ISO6709", "-22.9519-043.2105+700.000/")], None)? {
            created.push(v);
        }
        let v = root.join("2026/Windsurfing/Bonaire/Videos/DJI_0007.MP4");
        if write_video(ff, &v, 20.0, Some("2026-05-20T18:05:10.000000Z"), None, &[("encoder", "DJI Mini4 Pro")], None)? {
            created.push(v);
        }
        let v = root.join("2026/Argentina-Brazil/Florianopolis/Videos/PXL_20260309_141512345.mp4");
        if write_video(ff, &v, 5.0, None, None, &[], None)? {
            created.push(v);
        }
        for (name, title, artist) in [("Ocean Breeze.mp3", "Ocean Breeze", "Studio Waves"), ("Sunset Drive.m4a", "Sunset Drive", "The Coastals"), ("Field Recording.wav", "Field Recording", "")] {
            let a = root.join("Music").join(name);
            let mut tags = vec![("title", title), ("album", "Summer Sessions"), ("genre", "Ambient")];
            if !artist.is_empty() {
                tags.push(("artist", artist));
            }
            if write_audio(ff, &a, 6.0, &tags)? {
                created.push(a);
            }
        }
    }
    Ok(created)
}

/// Many small photos for load testing (`count` files spread over folders and days).
pub fn make_bulk_photos(root: &Path, count: usize) -> std::io::Result<()> {
    // Encode one template per orientation, then vary only EXIF bytes (fast).
    for i in 0..count {
        let day = 1 + (i / 200) % 28;
        let month = 1 + (i / (200 * 28)) % 12;
        let spec = ExifSpec {
            make: Some(["Apple", "GoPro", "SONY", "Canon"][i % 4].into()),
            model: Some(["iPhone 15", "HERO12 Black", "ILCE-7M4", "Canon EOS R5"][i % 4].into()),
            datetime_original: Some(format!("2025:{month:02}:{day:02} {:02}:{:02}:{:02}", 8 + (i % 12), i % 60, (i * 7) % 60)),
            ..Default::default()
        };
        let p = root.join(format!("bulk/2025-{month:02}/day-{day:02}/IMG_{i:06}.jpg"));
        write_jpeg(&p, 160, 120, i as u32, Some(&spec))?;
    }
    Ok(())
}
