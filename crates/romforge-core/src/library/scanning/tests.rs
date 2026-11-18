//! Scans committing as they go (here after every file: see
//! `store::COMMIT_EVERY`).

use crate::job::Cancel;
use crate::{Error, Library, ScanOptions, Scope};

fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn count(lib: &Library, sql: &str) -> i64 {
    lib.conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// A scan stopped after its first file keeps that file, stamped (the next
/// scan doesn't read it again) and pending its lookup, which the next
/// scan does.
#[test]
fn a_stopped_scan_keeps_its_finished_files_for_the_next() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let roms = fixtures().join("roms/gb");
    let cancel = Cancel::new();
    let opts = ScanOptions {
        jobs: 1,
        cancel: cancel.clone(),
        ..ScanOptions::default()
    };
    let r = lib.scan_with(&roms, Scope::All, &opts, |p| {
        if p.units_done == 1 {
            cancel.cancel();
        }
    });
    assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    let kept = count(&lib, "SELECT count(DISTINCT path) FROM files");
    assert!(kept >= 1, "nothing kept");
    assert_eq!(
        count(&lib, "SELECT count(*) FROM files WHERE pending = 0"),
        0,
        "not looked up yet"
    );
    assert_eq!(count(&lib, "SELECT count(*) FROM matches"), 0);

    let again = lib.scan(&roms, Scope::All).unwrap();
    assert!(again.unchanged >= kept as usize, "{again:?}");
    assert_eq!(count(&lib, "SELECT count(*) FROM files WHERE pending = 1"), 0);
    let mut full = Library::open_in_memory().unwrap();
    full.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    full.scan(&roms, Scope::All).unwrap();
    let matched = "SELECT count(*) FROM files WHERE status = 'matched'";
    assert!(count(&lib, matched) > 0);
    assert_eq!(count(&lib, matched), count(&full, matched));
}

/// Looking a file up by its content (import plans, Search) uses an index,
/// not a pass over every file.
#[test]
fn files_are_found_by_content_through_an_index() {
    let lib = Library::open_in_memory().unwrap();
    let plan: String = lib
        .conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT path FROM files WHERE sha1 = ?1 AND size = ?2",
            ["x", "1"],
            |r| r.get(3),
        )
        .unwrap();
    assert!(plan.contains("files_sha1"), "{plan}");
}
