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

/// An archive with nothing to hash in it isn't read again by the next scan
/// (its stamp is kept apart, with no file row), until the DATs change.
#[test]
fn an_archive_with_no_rom_is_read_once() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(dir.path().join("notes.zip")).unwrap());
    zip.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"no rom here").unwrap();
    zip.finish().unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let first = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(first.unchanged, 0);
    assert_eq!(count(&lib, "SELECT count(*) FROM scan_stamps"), 1);
    assert_eq!(count(&lib, "SELECT count(*) FROM files"), 0);
    let again = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(again.unchanged, 1, "{again:?}");

    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    assert_eq!(count(&lib, "SELECT count(*) FROM scan_stamps"), 0);
    let after = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(after.unchanged, 0, "read again: it may be an arcade set now");

    std::fs::remove_file(dir.path().join("notes.zip")).unwrap();
    lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(count(&lib, "SELECT count(*) FROM scan_stamps"), 0);
}

/// Asked to stop, a read ends inside the file instead of at its end.
#[test]
fn a_stop_ends_a_read_partway() {
    let cancel = Cancel::new();
    let bytes = vec![7u8; 4 << 20];
    let ok = crate::job::watching(&cancel, || {
        crate::hash::hash_stream(&bytes[..], bytes.len() as u64)
    });
    assert!(ok.is_ok());
    cancel.cancel();
    let stopped = crate::job::watching(&cancel, || {
        crate::hash::hash_stream(&bytes[..], bytes.len() as u64)
    });
    assert!(stopped.is_err());
    // Outside a scan's read, the flag isn't seen.
    assert!(crate::hash::hash_stream(&bytes[..], bytes.len() as u64).is_ok());
}
