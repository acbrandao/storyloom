//! Phase 3: clip extraction. Real FFmpeg exports from generated videos with known capture
//! times; checks durations, keyframe-aligned lossless starts, capture-time arithmetic, embedded
//! metadata, filesystem-date policies, provenance and source immutability.

mod common;
use common::*;
use curator_core::database::models::*;
use curator_core::filesystem::renaming::ConflictPolicy;
use curator_core::filesystem::timestamps::{self, FileTimestamps};
use curator_core::media::fingerprints::hash_file;
use curator_core::tools::Tools;
use curator_core::video::exporting::{ExportRequest, FileDatePolicy};
use curator_core::video::trimming::ExportPreset;
use curator_core::Application;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

const BIRTH_MS: i64 = 1_577_934_245_000;
const MTIME_MS: i64 = 1_623_053_350_000;

struct Fx {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    clips: PathBuf,
    app: Application,
    _rt: tokio::runtime::Runtime,
}

fn fixture() -> Option<Fx> {
    let tools = Tools::discover(&[]);
    let ff = tools.ffmpeg.clone()?;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap().join("media");
    let clips = tmp.path().canonicalize().unwrap().join("clips");
    use curator_core::testing::write_video;
    // GoPro-style: local time in the UTC field, keyframe every second (30 frames at 30 fps).
    assert!(write_video(&ff, &root.join("GX010042.MP4"), 12.0, Some("2026-07-14T15:32:45.000000Z"), None, &[("firmware", "H23.01.01.10.00")], Some("GoPro AVC")).unwrap());
    // iPhone-style: local time + offset.
    assert!(write_video(&ff, &root.join("IMG_4821.MOV"), 8.0, Some("2026-03-15T19:20:00.000000Z"), Some("2026-03-15T16:20:00-0300"), &[], None).unwrap());
    // No capture metadata at all.
    assert!(write_video(&ff, &root.join("nodate.mp4"), 4.0, None, None, &[], None).unwrap());
    for f in ["GX010042.MP4", "IMG_4821.MOV", "nodate.mp4"] {
        timestamps::apply(&root.join(f), &FileTimestamps { created_ms: Some(BIRTH_MS), modified_ms: MTIME_MS }).unwrap();
    }
    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    app.add_source(&root).unwrap();
    wait_idle(&app);
    Some(Fx { _tmp: tmp, root, clips, app, _rt: rt })
}

fn id_of(app: &Application, name: &str) -> i64 {
    let ids = app.media_ids(&MediaFilter::default(), SortField::Filename, false).unwrap();
    *ids.iter().find(|i| app.media_detail(**i).unwrap().current_filename == name).unwrap()
}

fn req(fx: &Fx, ids: Vec<i64>, preset: ExportPreset, dates: FileDatePolicy) -> ExportRequest {
    ExportRequest {
        segment_ids: ids,
        destination: fx.clips.to_string_lossy().into(),
        preset,
        name_template: "{capture_date}_{capture_time}_{original_name}_{sequence}".into(),
        file_dates: dates,
        conflict: ConflictPolicy::AddSuffix,
    }
}

fn snapshot(p: &Path) -> (String, FileTimestamps) {
    (hash_file(p, &CancellationToken::new()).unwrap(), timestamps::read(p).unwrap())
}

fn duration(p: &str) -> f64 {
    let tools = Tools::discover(&[]);
    curator_core::video::trimming::source_info(tools.ffprobe.as_deref().unwrap(), Path::new(p)).unwrap().duration
}

#[test]
fn exact_multi_segment_export_with_timestamps_and_provenance() {
    let Some(fx) = fixture() else { return };
    let app = &fx.app;
    let gopro = id_of(app, "GX010042.MP4");
    let src_path = fx.root.join("GX010042.MP4");
    let before = snapshot(&src_path);
    let s1 = app.create_segment(gopro, 1.0, 3.5, None).unwrap();
    let s2 = app.create_segment(gopro, 4.5, 7.25, Some("Jump")).unwrap();
    let s3 = app.create_segment(gopro, 9.0, 11.0, None).unwrap();

    let plan = app.plan_clip_export(&req(&fx, vec![s3, s1, s2], ExportPreset::Exact, FileDatePolicy::SourceFile)).unwrap();
    assert_eq!(plan.ready, 3);
    let caps: Vec<_> = plan.items.iter().map(|i| i.capture_local.clone().unwrap()).collect();
    assert_eq!(caps, ["2026-07-14T15:32:46", "2026-07-14T15:32:49.500", "2026-07-14T15:32:54"], "source capture + in-point, chronological");
    assert!(plan.items.iter().all(|i| i.capture_offset.is_none()), "no time zone invented for GoPro local time");
    assert!(plan.items[0].to.ends_with("2026-07-14_153246_GX010042_001.mp4"), "{}", plan.items[0].to);

    app.start_clip_export(&req(&fx, vec![s1, s2, s3], ExportPreset::Exact, FileDatePolicy::SourceFile)).unwrap();
    wait_idle(app);
    let segs = app.segments(gopro).unwrap();
    assert_eq!(segs.iter().filter(|s| s.exported_path.is_some()).count(), 3);
    for (s, (want_dur, want_cap)) in segs.iter().zip([(2.5, "2026-07-14T15:32:46"), (2.75, "2026-07-14T15:32:49.500"), (2.0, "2026-07-14T15:32:54")]) {
        let path = s.exported_path.as_deref().unwrap();
        assert!((duration(path) - want_dur).abs() < 0.15, "{path}: {} vs {want_dur}", duration(path));
        assert_eq!(s.processing_method.as_deref(), Some("reencode"));
        assert_eq!(s.actual_in_point, Some(s.in_point), "exact cut starts where requested");
        assert_eq!(s.calculated_capture_datetime.as_deref(), Some(want_cap));
        let v = s.verification.as_ref().unwrap();
        assert_eq!(v["durationVerified"], true);
        assert_eq!(v["captureEmbeddedVerified"], true, "{v}");
        assert_eq!(v["sourceUnchanged"], true);
        // Filesystem dates follow the source file (default policy).
        let ts = timestamps::read(Path::new(path)).unwrap();
        assert!((ts.modified_ms - MTIME_MS).abs() < 2000);
        if cfg!(target_os = "macos") {
            assert_eq!(ts.created_ms, Some(BIRTH_MS));
        }
        // Catalog record of the clip: authoritative capture time + provenance.
        let clip = app.media_detail(s.output_media_id.expect("clip catalogued")).unwrap();
        assert_eq!(clip.capture_datetime.as_deref(), Some(want_cap));
        assert_eq!(clip.capture_time_source.as_deref(), Some("derived_clip"));
        let origin = clip.clip_origin.unwrap();
        assert_eq!((origin.source_media_id, origin.in_point), (gopro, s.in_point));
    }
    assert_eq!(segs[1].name.as_deref(), Some("Jump"));
    assert_eq!(app.media_detail(gopro).unwrap().derived_segments, 3);
    assert_eq!(snapshot(&src_path), before, "source video unchanged (content + file dates)");

    // Rescans keep the calculated capture times and create no duplicates.
    let n = app.stats().unwrap().total_media;
    for s in app.sources().unwrap() {
        app.rescan_source(s.id).unwrap();
        wait_idle(app);
    }
    assert_eq!(app.stats().unwrap().total_media, n);
    let clip = app.media_detail(segs[0].output_media_id.unwrap()).unwrap();
    assert_eq!(clip.capture_datetime.as_deref(), Some("2026-07-14T15:32:46"));

    // Exported clips are a permanent record: their range can't change and they can't be deleted.
    assert!(app.delete_segment(segs[0].id).is_err());
    assert!(app.update_segment(segs[0].id, 0.5, 3.5, None).is_err());
    assert!(app.update_segment(segs[0].id, 1.0, 3.5, Some("Renamed label")).is_ok());
    // Removing the source folder would orphan the clips: refused.
    assert!(app.remove_source(app.sources().unwrap().iter().find(|s| s.path.ends_with("media")).unwrap().id).is_err());
}

#[test]
fn lossless_export_starts_on_keyframe_and_dates_follow_actual_start() {
    let Some(fx) = fixture() else { return };
    let app = &fx.app;
    let gopro = id_of(app, "GX010042.MP4");
    let seg = app.create_segment(gopro, 4.5, 7.75, None).unwrap();
    let plan = app.plan_clip_export(&req(&fx, vec![seg], ExportPreset::Lossless, FileDatePolicy::SourceFile)).unwrap();
    let item = &plan.items[0];
    assert!((item.actual_in - 4.0).abs() < 0.05, "keyframe every second → starts at 4.0, got {}", item.actual_in);
    assert_eq!(item.capture_local.as_deref(), Some("2026-07-14T15:32:49"), "capture = source + actual (keyframe) start");
    assert!(item.notes.iter().any(|n| n.contains("earlier")), "{:?}", item.notes);
    assert!(item.to.ends_with(".mp4"));
    app.start_clip_export(&req(&fx, vec![seg], ExportPreset::Lossless, FileDatePolicy::SourceFile)).unwrap();
    wait_idle(app);
    let s = &app.segments(gopro).unwrap()[0];
    assert_eq!(s.processing_method.as_deref(), Some("stream_copy"));
    let d = duration(s.exported_path.as_deref().unwrap());
    assert!((d - 3.75).abs() < 0.6, "duration {d}");
    // Export the same segment again (another version): a second provenance row is created.
    app.start_clip_export(&req(&fx, vec![seg], ExportPreset::Exact, FileDatePolicy::SourceFile)).unwrap();
    wait_idle(app);
    let all = app.segments(gopro).unwrap();
    assert_eq!(all.iter().filter(|s| s.exported_path.is_some()).count(), 2);
    let paths: std::collections::HashSet<_> = all.iter().filter_map(|s| s.exported_path.clone()).collect();
    assert_eq!(paths.len(), 2, "versions never overwrite each other");
}

#[test]
fn timezone_and_clip_capture_file_dates() {
    let Some(fx) = fixture() else { return };
    let app = &fx.app;
    let iphone = id_of(app, "IMG_4821.MOV");
    let seg = app.create_segment(iphone, 2.0, 5.0, None).unwrap();
    app.start_clip_export(&req(&fx, vec![seg], ExportPreset::Exact, FileDatePolicy::ClipCapture)).unwrap();
    wait_idle(app);
    let s = &app.segments(iphone).unwrap()[0];
    assert_eq!(s.calculated_capture_datetime.as_deref(), Some("2026-03-15T16:20:02"));
    assert_eq!(s.calculated_capture_timezone.as_deref(), Some("-03:00"), "offset carried over");
    let path = s.exported_path.as_deref().unwrap();
    assert!(path.ends_with(".mov"), "QuickTime source → QuickTime clip");
    // Embedded metadata read back by an independent tool.
    let tools = Tools::discover(&[]);
    let probe = curator_core::video::probe::probe(tools.ffprobe.as_deref().unwrap(), Path::new(path)).unwrap();
    assert_eq!(probe.pointer("/format/tags/com.apple.quicktime.creationdate").and_then(|v| v.as_str()), Some("2026-03-15T16:20:02-0300"));
    assert!(probe.pointer("/format/tags/creation_time").and_then(|v| v.as_str()).unwrap().starts_with("2026-03-15T19:20:02"), "UTC instant");
    // File dates = clip capture instant (19:20:02Z).
    let want = chrono::DateTime::parse_from_rfc3339("2026-03-15T19:20:02Z").unwrap().timestamp_millis();
    let ts = timestamps::read(Path::new(path)).unwrap();
    assert!((ts.modified_ms - want).abs() < 2000);
    if cfg!(target_os = "macos") {
        assert!((ts.created_ms.unwrap() - want).abs() < 2000);
    }
}

#[test]
fn missing_capture_metadata_is_never_replaced_by_export_date() {
    let Some(fx) = fixture() else { return };
    let app = &fx.app;
    let nodate = id_of(app, "nodate.mp4");
    assert_eq!(app.media_detail(nodate).unwrap().capture_datetime, None);
    let seg = app.create_segment(nodate, 1.0, 2.0, None).unwrap();
    let plan = app.plan_clip_export(&req(&fx, vec![seg], ExportPreset::Exact, FileDatePolicy::SourceFile)).unwrap();
    assert!(plan.items[0].capture_local.is_none());
    assert!(plan.items[0].to.contains("undated"), "{}", plan.items[0].to);
    app.start_clip_export(&req(&fx, vec![seg], ExportPreset::Exact, FileDatePolicy::SourceFile)).unwrap();
    wait_idle(app);
    let s = &app.segments(nodate).unwrap()[0];
    let clip = app.media_detail(s.output_media_id.unwrap()).unwrap();
    assert_eq!(clip.capture_datetime, None, "no capture date invented");
    let tools = Tools::discover(&[]);
    let probe = curator_core::video::probe::probe(tools.ffprobe.as_deref().unwrap(), Path::new(s.exported_path.as_deref().unwrap())).unwrap();
    assert!(probe.pointer("/format/tags/creation_time").is_none(), "no creation_time written: {probe}");
}

#[test]
fn segment_validation() {
    let Some(fx) = fixture() else { return };
    let app = &fx.app;
    let gopro = id_of(app, "GX010042.MP4");
    assert!(app.create_segment(gopro, 5.0, 4.0, None).is_err(), "out before in");
    assert!(app.create_segment(gopro, 5.0, 5.05, None).is_err(), "too short");
    assert!(app.create_segment(gopro, 5.0, 30.0, None).is_err(), "past the end");
    assert!(app.create_segment(gopro, -1.0, 2.0, None).is_err());
    let draft = app.create_segment(gopro, 1.0, 2.0, None).unwrap();
    app.update_segment(draft, 1.5, 2.5, Some("x")).unwrap();
    app.delete_segment(draft).unwrap();
    assert!(app.segments(gopro).unwrap().is_empty());
    assert!(app.filmstrip(gopro).unwrap().exists(), "timeline filmstrip");
}
