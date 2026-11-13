//! Timestamp-preservation regression tests: filesystem timestamps on copy/ingest,
//! source immutability, and capture-time handling.

mod common;
use common::*;
use curator_core::filesystem::operations::safe_copy;
use curator_core::filesystem::timestamps::{self, FileTimestamps};
use curator_core::media::fingerprints::hash_file;
use curator_core::media::importer::IngestLayout;
use curator_core::platform::{self, BirthTimeSupport};
use curator_core::testing::{write_jpeg, ExifSpec};
use curator_core::video::timeline::clip_capture_time;
use tokio_util::sync::CancellationToken;

// 2020-01-02T03:04:05.000Z and 2021-06-07T08:09:10.000Z
const BIRTH_MS: i64 = 1_577_934_245_000;
const MTIME_MS: i64 = 1_623_053_350_000;

fn make_aged_photo(path: &std::path::Path) -> FileTimestamps {
    let spec = ExifSpec { datetime_original: Some("2019:08:01 12:00:00".into()), offset_original: Some("+02:00".into()), ..Default::default() };
    write_jpeg(path, 320, 240, 7, Some(&spec)).unwrap();
    let ts = FileTimestamps { created_ms: Some(BIRTH_MS), modified_ms: MTIME_MS };
    let report = timestamps::apply(path, &ts).unwrap();
    if cfg!(target_os = "macos") {
        assert!(report.fully_preserved(), "could not prepare test file: {report:?}");
    }
    timestamps::read(path).unwrap()
}

#[test]
fn birth_time_can_be_set_on_this_platform() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("a.bin");
    std::fs::write(&p, b"x").unwrap();
    let r = platform::set_birth_time(&p, BIRTH_MS);
    if cfg!(target_os = "macos") {
        assert_eq!(r, BirthTimeSupport::Set);
        assert_eq!(timestamps::read(&p).unwrap().created_ms, Some(BIRTH_MS));
    } else {
        // Must be reported, never silently claimed.
        assert!(matches!(r, BirthTimeSupport::Unsupported(_) | BirthTimeSupport::Set));
    }
}

#[test]
fn safe_copy_preserves_content_and_timestamps_and_leaves_source_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src/IMG_1.JPG");
    let src_ts = make_aged_photo(&src);
    let src_hash = hash_file(&src, &CancellationToken::new()).unwrap();

    let dest = tmp.path().join("library/2019/IMG_1.JPG");
    let report = safe_copy(&src, &dest, &CancellationToken::new()).unwrap();
    assert_eq!(report.source_hash, report.destination_hash);
    assert_eq!(hash_file(&dest, &CancellationToken::new()).unwrap(), src_hash);
    assert!(report.timestamps.modified_preserved);
    let dest_ts = timestamps::read(&dest).unwrap();
    assert!((dest_ts.modified_ms - MTIME_MS).abs() < 2000);
    if cfg!(target_os = "macos") {
        assert_eq!(report.timestamps.created_preserved, Some(true), "{:?}", report.timestamps);
        assert_eq!(dest_ts.created_ms, Some(BIRTH_MS));
    }
    // Source unchanged: bytes and timestamps.
    assert_eq!(hash_file(&src, &CancellationToken::new()).unwrap(), src_hash);
    assert_eq!(timestamps::read(&src).unwrap(), src_ts);
    // No temp files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(dest.parent().unwrap()).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains("mcpartial")).collect();
    assert!(leftovers.is_empty());
}

#[test]
fn safe_copy_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("a.jpg");
    make_aged_photo(&src);
    let dest = tmp.path().join("b.jpg");
    std::fs::write(&dest, b"precious").unwrap();
    assert!(safe_copy(&src, &dest, &CancellationToken::new()).is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"precious");
    assert!(safe_copy(&src, &src, &CancellationToken::new()).is_err());
}

#[test]
fn ingest_copies_by_capture_date_with_provenance_and_timestamps() {
    let tmp = tempfile::tempdir().unwrap();
    let card = tmp.path().join("card/DCIM/100CANON");
    let photo = card.join("IMG_1.JPG");
    let src_ts = make_aged_photo(&photo);
    let nodate = card.join("no_metadata.jpg");
    write_jpeg(&nodate, 100, 100, 3, None).unwrap();
    let library = tmp.path().join("Library");

    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    let plan = app.plan_ingest(&tmp.path().join("card"), &library, IngestLayout::ByCaptureDate, None).unwrap();
    assert_eq!(plan.to_copy, 2);
    let dated = plan.items.iter().find(|i| i.source.ends_with("IMG_1.JPG")).unwrap();
    assert!(dated.destination.ends_with("Library/2019/2019-08-01/IMG_1.JPG"), "{}", dated.destination);
    let undated = plan.items.iter().find(|i| i.source.ends_with("no_metadata.jpg")).unwrap();
    assert!(undated.destination.contains("Unknown Date"), "export date must not be used: {}", undated.destination);

    app.start_ingest(plan).unwrap();
    wait_idle(&app);
    let dest = library.join("2019/2019-08-01/IMG_1.JPG");
    assert!(dest.exists());
    assert_eq!(timestamps::read(&photo).unwrap(), src_ts, "source untouched");
    let dts = timestamps::read(&dest).unwrap();
    assert!((dts.modified_ms - src_ts.modified_ms).abs() < 2000);
    if cfg!(target_os = "macos") {
        assert_eq!(dts.created_ms, src_ts.created_ms);
    }
    // Catalog: capture time from EXIF, original path points at the card.
    let ids = app.media_ids(&Default::default(), Default::default(), false).unwrap();
    let details: Vec<_> = ids.iter().map(|i| app.media_detail(*i).unwrap()).collect();
    let d = details.iter().find(|d| d.current_path.ends_with("2019-08-01/IMG_1.JPG")).unwrap();
    assert_eq!(d.capture_datetime.as_deref(), Some("2019-08-01T12:00:00"));
    assert_eq!(d.capture_timezone.as_deref(), Some("+02:00"));
    assert!(d.original_path.ends_with("card/DCIM/100CANON/IMG_1.JPG"), "{}", d.original_path);
    // Journal records the verified copy.
    let ops = app.file_operations(10).unwrap();
    assert_eq!(ops.iter().filter(|o| o.operation_type == "copy" && o.operation_status == "completed").count(), 2);

    // Re-running the same ingest copies nothing new.
    let plan2 = app.plan_ingest(&tmp.path().join("card"), &library, IngestLayout::ByCaptureDate, None).unwrap();
    assert_eq!(plan2.to_copy, 0);
    assert_eq!(plan2.already_present, 2);
}

#[test]
fn ingest_refuses_nested_library() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("photos");
    make_aged_photo(&src.join("a.jpg"));
    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    assert!(app.plan_ingest(&src, &src.join("library"), IngestLayout::ByCaptureDate, None).is_err() || !src.join("library").exists());
    std::fs::create_dir_all(src.join("library")).unwrap();
    assert!(app.plan_ingest(&src, &src.join("library"), IngestLayout::ByCaptureDate, None).is_err());
}

#[test]
fn clip_capture_time_is_source_plus_in_point() {
    // The spec's example.
    let c = clip_capture_time("2026-07-14T15:32:45", None, 270.0).unwrap();
    assert_eq!(c.local, "2026-07-14T15:37:15");
    // Multiple clips from one source.
    for (in_pt, expect) in [(72.0, "2026-07-14T15:33:57"), (270.0, "2026-07-14T15:37:15"), (545.0, "2026-07-14T15:41:50")] {
        assert_eq!(clip_capture_time("2026-07-14T15:32:45", Some("-04:00"), in_pt).unwrap().local, expect);
    }
    // Missing source capture time → no capture time (never the export date).
    assert!(clip_capture_time("", None, 10.0).is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn heic_uses_container_orientation_not_exif_tag() {
    // A JPEG with EXIF orientation 6, converted to HEIC by macOS ImageIO.
    let tmp = tempfile::tempdir().unwrap();
    let jpg = tmp.path().join("r.jpg");
    let spec = ExifSpec { datetime_original: Some("2026:03:02 12:31:39".into()), orientation: Some(6), ..Default::default() };
    write_jpeg(&jpg, 1200, 800, 4, Some(&spec)).unwrap();
    let heic = tmp.path().join("r.heic");
    let ok = std::process::Command::new("/usr/bin/sips").args(["-s", "format", "heic"]).arg(&jpg).arg("--out").arg(&heic).output().unwrap().status.success();
    if !ok {
        return; // HEIC encoding unavailable on this machine
    }
    let tools = curator_core::tools::Tools::discover(&[]);
    let m = curator_core::media::metadata::extract(&heic, curator_core::database::models::MediaType::Photo, &tools).unwrap();
    assert_eq!(m.orientation, None, "EXIF orientation inside HEIF must not be applied");
    let (w, h) = curator_core::platform::display_dimensions(&heic).unwrap();
    assert_eq!((m.width, m.height), (Some(w), Some(h)), "catalog stores what macOS displays");
    assert_eq!(m.capture.unwrap().local, "2026-03-02T12:31:39");
}
