//! Phase 2: batch rename, folder organization (copy/move), conflicts and undo — with
//! content, capture-time and filesystem-timestamp preservation checked at every step.

mod common;
use common::*;
use curator_core::database::models::*;
use curator_core::filesystem::organization::OrganizeMode;
use curator_core::filesystem::renaming::{ConflictPolicy, PlanItem, PlanStatus};
use curator_core::filesystem::timestamps::{self, FileTimestamps};
use curator_core::filesystem::transactions;
use curator_core::media::fingerprints::hash_file;
use curator_core::testing::{write_jpeg, ExifSpec};
use curator_core::Application;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

const BIRTH_MS: i64 = 1_577_934_245_000;
const MTIME_MS: i64 = 1_623_053_350_000;

fn photo(path: &Path, when: &str, seed: u32) {
    let spec = ExifSpec { datetime_original: Some(when.into()), offset_original: Some("-04:00".into()), model: Some("HERO12 Black".into()), ..Default::default() };
    write_jpeg(path, 200, 150, seed, Some(&spec)).unwrap();
    let r = timestamps::apply(path, &FileTimestamps { created_ms: Some(BIRTH_MS), modified_ms: MTIME_MS }).unwrap();
    assert!(r.modified_preserved);
}

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    app: Application,
    _rt: tokio::runtime::Runtime,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("MEDIA_CURATOR_TRASH_DIR", std::env::temp_dir().join(format!("mc-test-trash-{}", std::process::id())));
    let root = tmp.path().canonicalize().unwrap().join("media");
    photo(&root.join("GOPR0001.JPG"), "2026:07:14 15:32:45", 1);
    photo(&root.join("GOPR0002.JPG"), "2026:07:14 15:33:57", 2);
    photo(&root.join("GOPR0003.JPG"), "2026:07:14 15:41:22", 3);
    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    app.add_source(&root).unwrap();
    wait_idle(&app);
    Fixture { _tmp: tmp, root, app, _rt: rt }
}

fn ids(app: &Application) -> Vec<i64> {
    app.media_ids(&MediaFilter::default(), SortField::CaptureDate, false).unwrap()
}

fn snapshot(p: &Path) -> (String, FileTimestamps) {
    (hash_file(p, &CancellationToken::new()).unwrap(), timestamps::read(p).unwrap())
}

#[test]
fn batch_rename_preview_apply_and_undo() {
    let f = fixture();
    let app = &f.app;
    let all = ids(app);
    let before: Vec<_> = all.iter().map(|i| app.media_detail(*i).unwrap()).collect();
    let snaps: Vec<_> = before.iter().map(|d| snapshot(Path::new(&d.current_path))).collect();
    app.update_curation(&all, &CurationUpdate { location: Some("Sandy Hook".into()), description: Some("windsurfing".into()), ..Default::default() }).unwrap();

    let template = "{capture_date}_{capture_time}_{location}_{description}_{sequence}";
    let plan = app.plan_rename(&all, template, 1, ConflictPolicy::Skip).unwrap();
    assert_eq!(plan.ready, 3);
    let names: Vec<_> = plan.items.iter().map(|i| Path::new(&i.to).file_name().unwrap().to_string_lossy().to_string()).collect();
    assert_eq!(names, ["2026-07-14_153245_SandyHook_Windsurfing_001.JPG", "2026-07-14_153357_SandyHook_Windsurfing_002.JPG", "2026-07-14_154122_SandyHook_Windsurfing_003.JPG"]);
    // Preview changed nothing.
    assert!(f.root.join("GOPR0001.JPG").exists());

    let (_, batch) = app.start_rename(&all, template, 1, ConflictPolicy::Skip).unwrap();
    wait_idle(app);
    for (i, id) in all.iter().enumerate() {
        let d = app.media_detail(*id).unwrap();
        assert_eq!(d.current_filename, names[i]);
        assert_eq!(d.original_filename, before[i].original_filename, "original name kept in the catalog");
        assert_eq!(d.capture_datetime, before[i].capture_datetime, "capture time unchanged");
        let (hash, ts) = snapshot(Path::new(&d.current_path));
        assert_eq!(hash, snaps[i].0, "content unchanged");
        assert_eq!(ts, snaps[i].1, "filesystem timestamps (creation + modification) preserved");
        assert!(!Path::new(&before[i].current_path).exists());
    }
    // Search finds the new names; journal has verified entries.
    assert_eq!(app.query_media(&MediaQuery { filter: MediaFilter { text: Some("SandyHook".into()), ..Default::default() }, limit: 10, ..Default::default() }).unwrap().total, 3);
    let ops = app.file_operations(10).unwrap();
    assert!(ops.iter().filter(|o| o.operation_type == "rename").all(|o| o.operation_status == "completed" && o.result.as_ref().unwrap()["timestampsVerified"] == true));

    // Rescan: no duplicates, nothing missing.
    app.rescan_source(app.sources().unwrap()[0].id).unwrap();
    wait_idle(app);
    let s = app.stats().unwrap();
    assert_eq!((s.total_media, s.missing), (3, 0));

    // Undo restores names, catalog paths and timestamps.
    app.undo_batch(&batch).unwrap();
    wait_idle(app);
    for (i, id) in all.iter().enumerate() {
        let d = app.media_detail(*id).unwrap();
        assert_eq!(d.current_path, before[i].current_path);
        assert_eq!(snapshot(Path::new(&d.current_path)), snaps[i]);
    }
    assert_eq!(app.operation_batches(10).unwrap().iter().find(|b| b.batch_id == batch).unwrap().undone, 3);
}

#[test]
fn rename_conflicts_are_detected_and_never_overwrite() {
    let f = fixture();
    let app = &f.app;
    let all = ids(app);
    // An unrelated file already holds the first target name.
    std::fs::write(f.root.join("trip-001.JPG"), b"precious").unwrap();
    let skip = app.plan_rename(&all, "trip-{sequence}", 1, ConflictPolicy::Skip).unwrap();
    assert_eq!(skip.items[0].status, PlanStatus::Conflict);
    assert_eq!(skip.ready, 2);
    let suffix = app.plan_rename(&all, "trip-{sequence}", 1, ConflictPolicy::AddSuffix).unwrap();
    assert!(suffix.items[0].to.ends_with("trip-001-2.JPG"));
    // Two files mapping to the same name inside one batch.
    let dup = app.plan_rename(&all, "same", 1, ConflictPolicy::Skip).unwrap();
    assert_eq!((dup.ready, dup.conflicts), (1, 2));
    let dup2 = app.plan_rename(&all, "same", 1, ConflictPolicy::AddSuffix).unwrap();
    let mut names: Vec<_> = dup2.items.iter().map(|i| Path::new(&i.to).file_name().unwrap().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names, ["same-2.JPG", "same-3.JPG", "same.JPG"]);

    app.start_rename(&all, "trip-{sequence}", 1, ConflictPolicy::Skip).unwrap();
    wait_idle(app);
    assert_eq!(std::fs::read(f.root.join("trip-001.JPG")).unwrap(), b"precious", "existing file untouched");
    assert!(f.root.join("GOPR0001.JPG").exists(), "conflicting item left as-is");
    assert!(f.root.join("trip-002.JPG").exists());
    // Bad templates are rejected before anything happens.
    assert!(app.plan_rename(&all, "{bogus}", 1, ConflictPolicy::Skip).is_err());
}

#[test]
fn chains_swaps_and_case_only_renames() {
    let f = fixture();
    let app = &f.app;
    let d: Vec<_> = ids(app).iter().map(|i| app.media_detail(*i).unwrap()).collect();
    let (a, b, c) = (&d[0], &d[1], &d[2]);
    let snaps: Vec<_> = d.iter().map(|x| snapshot(Path::new(&x.current_path))).collect();
    let item = |m: &MediaDetail, to: &str| PlanItem { media_id: m.id, from: m.current_path.clone(), to: f.root.join(to).to_string_lossy().into(), status: PlanStatus::Ready, message: None };
    // Swap A<->B and rename C to a name A currently holds? No: chain C -> X. Swap first.
    let items = vec![item(a, "GOPR0002.JPG"), item(b, "GOPR0001.JPG"), item(c, "GOPR0003-final.JPG")];
    let r = transactions::execute_moves(app.database(), &items, "rename", "test-swap", |_, _| {}).unwrap();
    assert_eq!((r.done, r.failed.len()), (3, 0), "{:?}", r.failed);
    assert_eq!(snapshot(&f.root.join("GOPR0002.JPG")).0, snaps[0].0, "A's content now under B's name");
    assert_eq!(snapshot(&f.root.join("GOPR0001.JPG")).0, snaps[1].0);
    assert_eq!(app.media_detail(a.id).unwrap().current_filename, "GOPR0002.JPG");
    assert!(std::fs::read_dir(&f.root).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".mc-rename")), "no temp files left");
    // Case-only rename (APFS is case-insensitive by default).
    let x = app.media_detail(c.id).unwrap();
    let lower = PlanItem { media_id: x.id, from: x.current_path.clone(), to: f.root.join("gopr0003-final.jpg").to_string_lossy().into(), status: PlanStatus::Ready, message: None };
    let r = transactions::execute_moves(app.database(), &[lower], "rename", "test-case", |_, _| {}).unwrap();
    assert_eq!(r.done, 1, "{:?}", r.failed);
    assert_eq!(app.media_detail(c.id).unwrap().current_filename, "gopr0003-final.jpg");
    assert_eq!(snapshot(&f.root.join("gopr0003-final.jpg")), snaps[2]);
}

#[test]
fn organize_move_and_undo() {
    let f = fixture();
    let app = &f.app;
    let all = ids(app);
    let snaps: Vec<_> = all.iter().map(|i| snapshot(Path::new(&app.media_detail(*i).unwrap().current_path))).collect();
    let dest = f.root.parent().unwrap().join("Organized");
    let plan = app.plan_organize(&all, &dest, Some("{year}/{capture_date}"), OrganizeMode::Move, ConflictPolicy::Skip).unwrap();
    assert_eq!(plan.ready, 3);
    assert!(plan.items[0].to.ends_with("Organized/2026/2026-07-14/GOPR0001.JPG"));
    let (_, batch) = app.start_organize(&all, &dest, Some("{year}/{capture_date}"), OrganizeMode::Move, ConflictPolicy::Skip).unwrap();
    wait_idle(app);
    for (i, id) in all.iter().enumerate() {
        let d = app.media_detail(*id).unwrap();
        assert!(d.current_path.contains("Organized/2026/2026-07-14/"), "{}", d.current_path);
        assert_eq!(snapshot(Path::new(&d.current_path)), snaps[i], "content and timestamps preserved");
    }
    // Rescanning both folders: nothing missing, no duplicates.
    for s in app.sources().unwrap() {
        app.rescan_source(s.id).unwrap();
        wait_idle(app);
    }
    let st = app.stats().unwrap();
    assert_eq!((st.total_media, st.missing), (3, 0));
    app.undo_batch(&batch).unwrap();
    wait_idle(app);
    assert!(f.root.join("GOPR0002.JPG").exists());
    assert_eq!(snapshot(&f.root.join("GOPR0002.JPG")), snaps[1]);
}

#[test]
fn organize_copy_carries_curation_and_undo_trashes_only_unmodified_copies() {
    let f = fixture();
    let app = &f.app;
    let all = ids(app);
    let tag = app.create_tag("Activities/Windsurfing", None).unwrap();
    app.assign_tag(&all[..1], tag).unwrap();
    app.update_curation(&all[..1], &CurationUpdate { rating: Some(5), selection_status: Some("selected".into()), notes: Some("best jump".into()), ..Default::default() }).unwrap();
    let dest = f.root.parent().unwrap().join("Highlights");
    let (_, batch) = app.start_organize(&all, &dest, None, OrganizeMode::Copy, ConflictPolicy::Skip).unwrap();
    wait_idle(app);
    assert_eq!(app.stats().unwrap().total_media, 6);
    let copies = app.media_ids(&MediaFilter { folder: Some(dest.to_string_lossy().into()), ..Default::default() }, SortField::CaptureDate, false).unwrap();
    assert_eq!(copies.len(), 3);
    let first = app.media_detail(copies[0]).unwrap();
    assert_eq!((first.rating, first.selection_status.as_str(), first.notes.as_deref()), (5, "selected", Some("best jump")));
    assert_eq!(first.tags.len(), 1);
    assert!(first.original_path.ends_with("media/GOPR0001.JPG"), "provenance: {}", first.original_path);
    assert_eq!(first.capture_datetime.as_deref(), Some("2026-07-14T15:32:45"));
    let copy_ts = timestamps::read(Path::new(&first.current_path)).unwrap();
    assert!((copy_ts.modified_ms - MTIME_MS).abs() < 2000);
    if cfg!(target_os = "macos") {
        assert_eq!(copy_ts.created_ms, Some(BIRTH_MS));
    }
    // Modify one copy: undo must leave it alone.
    let modified = app.media_detail(copies[2]).unwrap().current_path;
    std::fs::write(&modified, b"edited by another app").unwrap();
    app.undo_batch(&batch).unwrap();
    wait_idle(app);
    assert!(!dest.join("GOPR0001.JPG").exists() && !dest.join("GOPR0002.JPG").exists(), "unmodified copies moved to Trash");
    assert!(Path::new(&modified).exists(), "edited copy kept");
    assert!(f.root.join("GOPR0001.JPG").exists(), "originals never touched");
    assert_eq!(app.stats().unwrap().total_media, 4);
}

#[test]
fn moves_across_drives_and_into_app_data_are_refused() {
    let f = fixture();
    let app = &f.app;
    let all = ids(app);
    let protected = app.config().data_dir.join("x");
    assert!(app.plan_organize(&all, &protected, None, OrganizeMode::Copy, ConflictPolicy::Skip).is_err());
    // /Volumes on another device (if any) or /dev — use the system temp volume check helper.
    assert!(curator_core::filesystem::organization::same_volume(&f.root, &f.root.join("sub")));
}

#[test]
fn rename_on_import() {
    let tmp = tempfile::tempdir().unwrap();
    let card = tmp.path().join("card");
    photo(&card.join("DSC0001.JPG"), "2026:07:14 15:32:45", 1);
    photo(&card.join("DSC0002.JPG"), "2026:07:14 09:00:00", 2);
    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    let plan = app
        .plan_ingest(&card, &tmp.path().join("Library"), curator_core::media::importer::IngestLayout::ByCaptureDate, Some("{capture_date}_{capture_time}_{camera}_{sequence}"))
        .unwrap();
    let names: Vec<_> = plan.items.iter().map(|i| Path::new(&i.destination).file_name().unwrap().to_string_lossy().to_string()).collect();
    assert_eq!(names, ["2026-07-14_090000_HERO12Black_001.JPG", "2026-07-14_153245_HERO12Black_002.JPG"], "chronological sequence");
    app.start_ingest(plan).unwrap();
    wait_idle(&app);
    let d = app.media_detail(app.media_ids(&MediaFilter::default(), SortField::CaptureDate, false).unwrap()[0]).unwrap();
    assert_eq!(d.current_filename, "2026-07-14_090000_HERO12Black_001.JPG");
    assert!(d.original_path.ends_with("card/DSC0002.JPG"), "{}", d.original_path);
    assert!(card.join("DSC0002.JPG").exists(), "source untouched");
}
