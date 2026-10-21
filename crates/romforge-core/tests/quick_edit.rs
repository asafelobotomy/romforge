//! Quick edits on their own connection beside a long job's
//! ([`Library::edit`]): they wait out a short save, and say "busy" rather
//! than "database is locked" when a scan holds the library.

use std::time::{Duration, Instant};

use romforge_core::{Error, Library};

/// A library on disk, and a second plain connection to it standing in for
/// the long job's.
fn library() -> (tempfile::TempDir, std::path::PathBuf, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    drop(Library::open(&path).unwrap());
    let job = rusqlite::Connection::open(&path).unwrap();
    job.busy_timeout(Duration::from_secs(10)).unwrap();
    (dir, path, job)
}

fn flip_empty_media(path: &std::path::Path) -> Result<(), Error> {
    Library::edit(path, |lib| {
        let mut s = lib.settings()?;
        s.show_empty_media = true;
        lib.save_settings(&s).map(drop)
    })
}

fn empty_media(path: &std::path::Path) -> bool {
    Library::open(path).unwrap().settings().unwrap().show_empty_media
}

/// Enrich saves game by game: an edit made meanwhile waits for that save,
/// then goes through.
#[test]
fn an_edit_waits_out_a_short_save() {
    let (_dir, path, job) = library();
    job.execute_batch("BEGIN IMMEDIATE").unwrap();
    let edit = std::thread::spawn({
        let path = path.clone();
        move || flip_empty_media(&path)
    });
    std::thread::sleep(Duration::from_millis(300));
    job.execute_batch("COMMIT").unwrap();
    edit.join().unwrap().unwrap();
    assert!(empty_media(&path));
}

/// A scan holds its write for the whole run: the edit gives up soon, says
/// why, and changes nothing; once the scan ends it goes through.
#[test]
fn an_edit_beside_a_scan_says_busy() {
    let (_dir, path, job) = library();
    job.execute_batch("BEGIN IMMEDIATE").unwrap();
    let t = Instant::now();
    let err = flip_empty_media(&path).unwrap_err();
    assert!(matches!(err, Error::Busy), "{err:?}");
    assert!(err.to_string().starts_with("busy:"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(8), "{:?}", t.elapsed());
    job.execute_batch("ROLLBACK").unwrap();
    assert!(!empty_media(&path));
    flip_empty_media(&path).unwrap();
    assert!(empty_media(&path));
}
