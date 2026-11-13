//! Acceptance test: index 10,000 files, then rescan without creating duplicates.
//! Run with: cargo test -p curator-core --release --test scale -- --ignored --nocapture

mod common;
use common::*;
use curator_core::database::models::*;
use std::time::Instant;

#[test]
#[ignore = "slow: generates and indexes 10,000 files"]
fn index_ten_thousand_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("media");
    let t = Instant::now();
    curator_core::testing::make_bulk_photos(&root, 10_000).unwrap();
    println!("generated 10,000 photos in {:?}", t.elapsed());

    let rt = runtime();
    let app = open_app(tmp.path(), &rt);
    let t = Instant::now();
    app.add_source(&root).unwrap();
    // The catalog stays queryable while indexing runs (UI responsiveness).
    let mut max_query_ms = 0u128;
    while !app.active_jobs().is_empty() {
        let q = Instant::now();
        app.query_media(&MediaQuery { limit: 240, descending: true, ..Default::default() }).unwrap();
        app.stats().unwrap();
        max_query_ms = max_query_ms.max(q.elapsed().as_millis());
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("indexed (metadata + thumbnails) in {:?}; slowest page+stats query during indexing: {max_query_ms} ms", t.elapsed());
    let s = app.stats().unwrap();
    assert_eq!(s.total_photos, 10_000);
    assert_eq!(s.metadata_pending, 0);
    assert_eq!(s.thumbnails_pending, 0);
    assert_eq!(s.without_capture_date, 0);

    let t = Instant::now();
    app.rescan_source(app.sources().unwrap()[0].id).unwrap();
    wait_idle(&app);
    println!("incremental rescan in {:?}", t.elapsed());
    assert_eq!(app.stats().unwrap().total_media, 10_000, "rescan must not create duplicates");

    let t = Instant::now();
    let p = app.query_media(&MediaQuery { filter: MediaFilter { text: Some("IMG_0099".into()), camera: Some("GoPro".into()), ..Default::default() }, limit: 100, ..Default::default() }).unwrap();
    println!("search+filter in {:?} ({} hits)", t.elapsed(), p.total);
    let t = Instant::now();
    app.query_media(&MediaQuery { offset: 9_000, limit: 240, descending: true, ..Default::default() }).unwrap();
    println!("deep page (offset 9000) in {:?}", t.elapsed());
}
