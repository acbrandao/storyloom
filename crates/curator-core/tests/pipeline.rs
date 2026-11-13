//! End-to-end tests of indexing, metadata, curation, search, persistence and statistics
//! against the generated sample library.

mod common;
use common::*;
use curator_core::database::models::*;
use curator_core::tools::Tools;
use std::path::Path;

fn find(app: &curator_core::Application, name: &str) -> MediaDetail {
    let q = MediaQuery { filter: MediaFilter { text: Some(name.into()), ..Default::default() }, limit: 50, ..Default::default() };
    let page = app.query_media(&q).unwrap();
    let item = page.items.iter().find(|i| i.filename == name).unwrap_or_else(|| panic!("{name} not found by search"));
    app.media_detail(item.id).unwrap()
}

fn all(app: &curator_core::Application, f: MediaFilter) -> MediaPage {
    app.query_media(&MediaQuery { filter: f, limit: 5000, ..Default::default() }).unwrap()
}

#[test]
fn full_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let media_root = tmp.path().join("media");
    let tools = Tools::discover(&[]);
    let has_ff = tools.ffmpeg.is_some() && tools.ffprobe.is_some();
    curator_core::testing::make_sample_library(&media_root, tools.ffmpeg.as_deref()).unwrap();
    // Snapshot every source file (bytes + timestamps) to prove indexing is read-only.
    let before = snapshot_tree(&media_root);

    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    let job = app.add_source(&media_root).unwrap();
    wait_idle(&app);
    let jobs = app.recent_jobs(5).unwrap();
    let j = jobs.iter().find(|x| x.id == job).unwrap();
    assert_eq!(j.status, "completed", "{:?}", j);

    // ---- type detection & counts ----
    let s = app.stats().unwrap();
    assert_eq!(s.total_photos, 40, "36 camera photos + screenshot + wallpaper + duplicate + truncated");
    if has_ff {
        assert_eq!(s.total_videos, 4);
        assert_eq!(s.total_audio, 3);
    }
    assert_eq!(s.total_media, s.total_photos + s.total_videos + s.total_audio);
    let disk_bytes: i64 = before.iter().filter(|(p, ..)| curator_core::media::media_types::classify(
        Path::new(p).extension().unwrap_or_default().to_str().unwrap()).is_some()).map(|(_, len, ..)| *len as i64).sum();
    assert_eq!(s.total_bytes, disk_bytes, "storage total must equal the sum of indexed file sizes");
    let by_type_total: i64 = s.by_type.iter().map(|t| t.bytes).sum();
    assert_eq!(by_type_total, s.total_bytes);
    assert_eq!(s.metadata_pending, 0);
    assert_eq!(s.duplicate_groups, 1, "IMG_0001 and its copy");

    // Problems recorded, not fatal.
    let issues = app.job_issues(job).unwrap();
    assert!(issues.iter().any(|i| i.path.ends_with("empty.mp4") && i.issue == "empty"));
    assert!(issues.iter().any(|i| i.path.ends_with("notes.xyz") && i.issue == "unsupported"));

    // ---- capture times ----
    let canon = find(&app, "DSC_0019.JPG"); // Sandy Hook, i=0, Canon writes no offset
    assert_eq!(canon.capture_datetime.as_deref(), Some("2026-07-14T09:10:00.250"));
    assert_eq!(canon.capture_timezone, None);
    assert_eq!(canon.capture_time_source.as_deref(), Some("exif_original"));
    assert_eq!(canon.camera_model.as_deref(), Some("Canon EOS R6m2"));
    assert!(canon.gps_latitude.unwrap() > 40.0);
    let iphone = find(&app, "IMG_0001.JPG");
    assert_eq!(iphone.capture_datetime.as_deref(), Some("2026-03-02T09:10:00.250"));
    assert_eq!(iphone.capture_timezone.as_deref(), Some("-03:00"));
    let rotated = find(&app, "IMG_0004.JPG");
    assert_eq!(rotated.orientation, Some(6));
    let shot = find(&app, "Screenshot 2026-07-14 at 15.32.45.png");
    assert_eq!(shot.capture_datetime.as_deref(), Some("2026-07-14T15:32:45"));
    assert_eq!(shot.capture_time_source.as_deref(), Some("filename"));
    let wall = find(&app, "wallpaper.jpg");
    assert_eq!(wall.capture_datetime, None, "filesystem dates must not become capture dates");
    assert!(wall.sort_datetime.is_some(), "still sortable provisionally");

    if has_ff {
        let gopro = find(&app, "GX010042.MP4");
        assert_eq!(gopro.capture_datetime.as_deref(), Some("2026-07-14T15:32:45"));
        assert_eq!(gopro.capture_time_source.as_deref(), Some("container_local"));
        assert_eq!(gopro.camera_make.as_deref(), Some("GoPro"));
        assert!((gopro.duration.unwrap() - 12.0).abs() < 0.2);
        assert_eq!(gopro.codec.as_deref(), Some("h264"));
        assert_eq!((gopro.width, gopro.height), (Some(640), Some(360)));
        let mov = find(&app, "IMG_4821.MOV");
        assert_eq!(mov.capture_datetime.as_deref(), Some("2026-03-15T16:20:00"));
        assert_eq!(mov.capture_timezone.as_deref(), Some("-03:00"));
        assert_eq!(mov.camera_model.as_deref(), Some("iPhone 16 Pro"));
        assert!(mov.gps_latitude.unwrap() < -22.0);
        let pxl = find(&app, "PXL_20260309_141512345.mp4");
        assert_eq!(pxl.capture_datetime.as_deref(), Some("2026-03-09T14:15:12.345"));
        let song = find(&app, "Ocean Breeze.mp3");
        assert_eq!(song.title.as_deref(), Some("Ocean Breeze"));
        assert_eq!(song.artist.as_deref(), Some("Studio Waves"));
        assert!(song.duration.unwrap() > 5.0);
        assert!(app.thumbnail(gopro.id).unwrap().is_some(), "video thumbnail");
    }

    // Thumbnails were generated, and the broken file is flagged rather than crashing the job.
    assert!(app.thumbnail(canon.id).unwrap().is_some());
    let truncated = find(&app, "truncated.jpg");
    assert!(truncated.metadata_status == "error" || truncated.thumbnail_status == "error", "{truncated:?}");

    // ---- rescan creates no duplicates ----
    let n_before = s.total_media;
    app.rescan_source(app.sources().unwrap()[0].id).unwrap();
    wait_idle(&app);
    assert_eq!(app.stats().unwrap().total_media, n_before);

    // ---- curation, tags, collections ----
    let photos = all(&app, MediaFilter { media_types: vec![MediaType::Photo], ..Default::default() });
    let ids: Vec<i64> = photos.items.iter().take(5).map(|i| i.id).collect();
    app.update_curation(&ids[..2], &CurationUpdate { rating: Some(5), ..Default::default() }).unwrap();
    app.update_curation(&ids[2..3], &CurationUpdate { rating: Some(3), selection_status: Some("selected".into()), ..Default::default() }).unwrap();
    app.update_curation(&ids[3..4], &CurationUpdate { selection_status: Some("rejected".into()), favorite: Some(true), color_label: Some("red".into()), ..Default::default() }).unwrap();
    app.update_curation(&ids[4..5], &CurationUpdate { notes: Some("Great light at sunset".into()), ..Default::default() }).unwrap();
    assert!(app.update_curation(&ids[..1], &CurationUpdate { rating: Some(9), ..Default::default() }).is_err());
    // A rating does not imply selection:
    assert_eq!(app.media_detail(ids[0]).unwrap().selection_status, "none");

    let windsurf = app.create_tag("Activities/Windsurfing", None).unwrap();
    let activities = app.tags().unwrap().into_iter().find(|t| t.name == "Activities").unwrap();
    assert_eq!(app.tags().unwrap().iter().find(|t| t.id == windsurf).unwrap().path, "Activities / Windsurfing");
    let brazil = app.create_tag("Locations/Brazil", None).unwrap();
    app.assign_tag(&ids[..3], windsurf).unwrap();
    app.assign_tag(&ids[1..2], brazil).unwrap();
    let coll = app.create_collection("Best of 2026", None).unwrap();
    app.add_to_collection(coll, &ids[..4]).unwrap();
    app.add_to_collection(coll, &ids[..4]).unwrap(); // idempotent
    let smart = app.create_smart_collection("Five stars", &MediaFilter { min_rating: Some(5), ..Default::default() }).unwrap();

    // ---- filters ----
    assert_eq!(all(&app, MediaFilter { min_rating: Some(5), ..Default::default() }).total, 2);
    assert_eq!(all(&app, MediaFilter { min_rating: Some(3), ..Default::default() }).total, 3);
    assert_eq!(all(&app, MediaFilter { selection_status: Some("selected".into()), ..Default::default() }).total, 1);
    assert_eq!(all(&app, MediaFilter { tag_ids: vec![activities.id], ..Default::default() }).total, 3, "parent tag includes children");
    assert_eq!(all(&app, MediaFilter { tag_ids: vec![windsurf, brazil], ..Default::default() }).total, 1);
    assert_eq!(all(&app, MediaFilter { collection_id: Some(coll), ..Default::default() }).total, 4);
    assert_eq!(all(&app, MediaFilter { collection_id: Some(smart), ..Default::default() }).total, 2);
    assert_eq!(all(&app, MediaFilter { favorite_only: true, color_label: Some("red".into()), ..Default::default() }).total, 1);
    assert_eq!(all(&app, MediaFilter { text: Some("great light".into()), ..Default::default() }).total, 1, "notes are searchable");
    assert!(all(&app, MediaFilter { text: Some("windsurf".into()), ..Default::default() }).total >= 3, "tags and folders searchable");
    assert_eq!(all(&app, MediaFilter { camera: Some("ILCE".into()), ..Default::default() }).total, 6);
    assert_eq!(all(&app, MediaFilter { date_from: Some("2026-07-14".into()), date_to: Some("2026-07-14".into()), media_types: vec![MediaType::Photo], ..Default::default() }).total, 7, "6 Sandy Hook photos + screenshot");
    let unrated_before = all(&app, MediaFilter { unrated_only: true, ..Default::default() }).total;
    assert_eq!(unrated_before, n_before - 3);
    if has_ff {
        assert_eq!(all(&app, MediaFilter { media_types: vec![MediaType::Video], min_duration: Some(10.0), max_duration: Some(60.0), ..Default::default() }).total, 2);
        assert_eq!(all(&app, MediaFilter { text: Some("gopro".into()), ..Default::default() }).total, 1);
    }
    // Paging is stable and complete.
    let mut seen = std::collections::HashSet::new();
    for off in (0..n_before).step_by(10) {
        let p = app.query_media(&MediaQuery { offset: off, limit: 10, descending: true, ..Default::default() }).unwrap();
        for it in p.items {
            assert!(seen.insert(it.id));
        }
    }
    assert_eq!(seen.len() as i64, n_before);

    // ---- user date correction kept separate from the original ----
    app.set_capture_correction(wall.id, Some("2019-12-25T08:00:00"), Some("+01:00")).unwrap();
    let w2 = app.media_detail(wall.id).unwrap();
    assert_eq!(w2.capture_datetime, None);
    assert_eq!(w2.user_capture_datetime.as_deref(), Some("2019-12-25T08:00:00"));
    assert_eq!(w2.sort_datetime.as_deref(), Some("2019-12-25T08:00:00"));
    assert!(app.set_capture_correction(wall.id, Some("not a date"), None).is_err());

    // ---- persistence across restart ----
    drop(app);
    let app = open_app(tmp.path(), &rt);
    assert_eq!(app.stats().unwrap().total_five_star, 2);
    assert_eq!(app.media_detail(ids[3]).unwrap().color_label.as_deref(), Some("red"));
    assert_eq!(app.media_detail(ids[0]).unwrap().tags.len(), 1);
    assert_eq!(app.collections().unwrap().iter().find(|c| c.id == coll).unwrap().media_count, 4);
    assert_eq!(app.collections().unwrap().iter().find(|c| c.id == smart).unwrap().media_count, 2);

    // ---- missing files are flagged, never deleted from the catalog ----
    std::fs::remove_file(media_root.join("Backup/IMG_0001 copy.JPG")).unwrap();
    app.rescan_source(app.sources().unwrap()[0].id).unwrap();
    wait_idle(&app);
    let s2 = app.stats().unwrap();
    assert_eq!(s2.total_media, n_before);
    assert_eq!(s2.missing, 1);

    // ---- backup ----
    let b = app.backup_database().unwrap();
    assert!(b.exists());
    assert_eq!(app.integrity_check().unwrap(), "ok");

    // ---- the source tree was never modified (except the file this test deleted) ----
    let after = snapshot_tree(&media_root);
    for (p, len, mtime, hash) in &before {
        if p.ends_with("IMG_0001 copy.JPG") {
            continue;
        }
        let a = after.iter().find(|x| &x.0 == p).unwrap_or_else(|| panic!("{p} vanished"));
        assert_eq!((len, mtime, hash), (&a.1, &a.2, &a.3), "{p} was modified");
    }
    // Removing a source only removes catalog records.
    let src = app.sources().unwrap()[0].id;
    app.remove_source(src).unwrap();
    assert_eq!(app.stats().unwrap().total_media, 0);
    assert!(media_root.join("2026/Windsurfing/Sandy-Hook/Photos/DSC_0019.JPG").exists());
}

fn snapshot_tree(root: &Path) -> Vec<(String, u64, i64, String)> {
    let token = tokio_util::sync::CancellationToken::new();
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let m = e.metadata().unwrap();
            let ts = curator_core::filesystem::timestamps::from_metadata(&m);
            let h = curator_core::media::fingerprints::hash_file(e.path(), &token).unwrap();
            (e.path().to_string_lossy().to_string(), m.len(), ts.modified_ms, h)
        })
        .collect()
}
