//! End-to-end tests over the committed fixtures in `fixtures/`.
//!
//! The fixtures are tiny synthetic files, not real ROMs. They are written by
//! the ignored `regenerate_fixtures` test:
//!
//!     cargo test -p romforge-core --test fixtures -- --ignored regenerate
//!
//! DAT hashes are computed over the ROM *body* directly, never through the
//! header detector, so the tests check the detector rather than mirror it.
//!
//! Scanning: scopes, rescans, symlinks.

use std::path::{Path, PathBuf};

use romforge_core::{Library, LibraryFilter, ScanOptions, Scope, Status, systems};

mod common;
use common::fixture_data::*;
use common::fixture_lib::*;
use common::fixtures;

#[test]
fn dat_without_known_system_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let src = std::fs::read_to_string(fixtures().join("dats/gb.dat")).unwrap();
    let path = dir.path().join("odd.dat");
    std::fs::write(&path, src.replace("Nintendo - Game Boy", "Homebrew Things")).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let d = lib.import_dat(&path, None).unwrap();
    assert_eq!(d.system, None);

    // Scoped to NES, the system-less DAT is out of scope: no silent fallback.
    let nes = systems::by_key("NES").unwrap().id;
    lib.scan(fixtures().join("roms/gb"), Scope::System(nes)).unwrap();
    for f in lib.files(&LibraryFilter::default()).unwrap() {
        assert_ne!(f.status, Status::Matched, "{}", f.path);
    }
    // Unscoped, it matches.
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let matched = lib
        .files(&LibraryFilter {
            system_id: None,
            status: Some(Status::Matched),
        })
        .unwrap();
    assert_eq!(matched.len(), 3);
}

#[test]
fn reimport_and_rescan_are_idempotent() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let again = lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    assert!(again.unchanged);
    lib.scan(fixtures(), Scope::All).unwrap();
    let first = by_path(&lib);
    lib.scan(fixtures(), Scope::All).unwrap();
    let second = by_path(&lib);
    assert_eq!(first.len(), second.len());
    for (k, v) in &first {
        assert_eq!(v.status, second[k].status, "{k}");
    }
    assert_eq!(lib.summary().unwrap().dats.len(), 8);
}

#[test]
fn new_dat_version_replaces_old_and_keeps_scope() {
    let dir = tempfile::tempdir().unwrap();
    let src = std::fs::read_to_string(fixtures().join("dats/gb.dat")).unwrap();
    let v2 = dir.path().join("gb-v2.dat");
    std::fs::write(&v2, src.replace("<version>1</version>", "<version>2</version>")).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let old = dat_id(&lib, "Nintendo - Game Boy");
    lib.scan(fixtures().join("roms/gb"), Scope::Dat(old)).unwrap();
    let new = lib.import_dat(&v2, None).unwrap();
    assert_eq!(new.replaced, Some(old));
    assert_eq!(lib.summary().unwrap().dats.len(), 8);
    let gb = lib
        .summary()
        .unwrap()
        .dats
        .into_iter()
        .find(|d| d.dat_id == new.dat_id)
        .unwrap();
    assert_eq!(gb.version.as_deref(), Some("2"));
    assert_eq!(gb.have, 4, "folder scope moved to the new DAT");
}

#[test]
fn scanning_writes_nothing_next_to_the_roms() {
    fn listing(dir: &Path) -> Vec<(PathBuf, u64)> {
        let mut v: Vec<_> = walkdir(dir);
        v.sort();
        v
    }
    fn walkdir(dir: &Path) -> Vec<(PathBuf, u64)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                out.extend(walkdir(&p));
            } else {
                let len = p.metadata().unwrap().len();
                out.push((p, len));
            }
        }
        out
    }
    let before = listing(&fixtures());
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    lib.scan(fixtures(), Scope::All).unwrap();
    assert_eq!(before, listing(&fixtures()));
}

#[test]
fn non_rom_files_and_members_are_skipped() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    assert_eq!(report.skipped, NON_ROMS.len());
    let files = by_path(&lib);
    for (path, _) in NON_ROMS {
        assert!(!files.contains_key(*path), "{path} was hashed");
    }
    assert_eq!(lib.summary().unwrap().files_by_status[2], (Status::Unknown, 1));

    // --all-files hashes them; they are unknown, everything else is unchanged.
    let opts = ScanOptions {
        all_files: true,
        ..ScanOptions::default()
    };
    let report = lib
        .scan_with(fixtures().join("roms"), Scope::All, opts, |_| {})
        .unwrap();
    assert_eq!(report.skipped, 0);
    let files = by_path(&lib);
    for (path, _) in NON_ROMS {
        assert_eq!(files[*path].status, Status::Unknown, "{path}");
    }
    assert_eq!(files["nes/Test NES (iNES).nes"].status, Status::Matched);
}

#[test]
fn nested_symlinked_folder_is_not_followed() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(fixtures(), Scope::All).unwrap();
    let link = std::fs::canonicalize(fixtures())
        .unwrap()
        .join("roms")
        .join(LINKED_DIR.0);
    assert_eq!(report.skipped_links, [link]);
    for f in lib.files(&LibraryFilter::default()).unwrap() {
        assert!(!f.path.contains(LINKED_DIR.0), "{} followed the link", f.path);
    }
}

#[test]
fn symlinked_root_is_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("my-gb");
    std::os::unix::fs::symlink(fixtures().join("roms/gb"), &root).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(&root, Scope::All).unwrap();
    assert_eq!(report.hashed, 4);
    assert!(report.skipped_links.is_empty());
}

#[test]
fn symlink_loop_cannot_run_away() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixtures().join("roms/gb/Test GB.gb"),
        dir.path().join("Test GB.gb"),
    )
    .unwrap();
    std::os::unix::fs::symlink(".", dir.path().join("loop")).unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("abs-loop")).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(report.hashed, 1);
    assert_eq!(report.skipped_links.len(), 2);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn progress_reports_running_totals() {
    let mut lib = Library::open_in_memory().unwrap();
    let mut seen = Vec::new();
    let report = lib
        .scan_with(fixtures(), Scope::All, ScanOptions::default(), |p| seen.push(p))
        .unwrap();
    let last = seen.last().unwrap();
    assert_eq!(last.hashed, report.hashed);
    assert_eq!(last.bytes_hashed, report.bytes_hashed);
    assert!(seen.windows(2).all(|w| w[0].bytes_hashed <= w[1].bytes_hashed));
}

#[test]
fn parallel_and_serial_scans_agree() {
    let statuses = |jobs: usize| {
        let mut lib = Library::open_in_memory().unwrap();
        import_all(&mut lib);
        let opts = ScanOptions {
            jobs,
            ..ScanOptions::default()
        };
        let report = lib
            .scan_with(fixtures().join("roms"), Scope::All, opts, |_| {})
            .unwrap();
        let files: Vec<_> = by_path(&lib).into_iter().map(|(p, f)| (p, f.status)).collect();
        (report.hashed, report.bytes_hashed, report.skipped, files)
    };
    let serial = statuses(1);
    assert!(serial.0 > 10);
    assert_eq!(statuses(4), serial);
    assert_eq!(statuses(0), serial);
}

#[test]
fn rescans_skip_unchanged_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    for f in ["gb/Test GB.gb", "nes/Test NES 2.0.nes", "archives/Test GBA.zip"] {
        let name = std::path::Path::new(f).file_name().unwrap();
        std::fs::copy(fixtures().join("roms").join(f), dir.join(name)).unwrap();
    }
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let first = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!((first.hashed, first.unchanged), (3, 0));
    let matched = |lib: &Library| {
        lib.files(&LibraryFilter::default())
            .unwrap()
            .iter()
            .filter(|f| f.status == Status::Matched)
            .count()
    };
    assert_eq!(matched(&lib), 3);

    let again = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!(
        (again.hashed, again.unchanged),
        (0, 3),
        "nothing read the second time"
    );
    assert_eq!(matched(&lib), 3, "and nothing lost");

    // A changed file is read again; a deleted one leaves the library.
    std::fs::write(dir.join("Test NES 2.0.nes"), b"changed").unwrap();
    std::fs::remove_file(dir.join("Test GB.gb")).unwrap();
    let after = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!((after.hashed, after.unchanged), (1, 1));
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files.len(), 2);
    assert!(
        files
            .iter()
            .any(|f| f.path.ends_with("Test NES 2.0.nes") && f.status == Status::Unknown)
    );

    let all = lib
        .scan_with(
            &dir,
            Scope::All,
            ScanOptions {
                rehash: true,
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();
    assert_eq!((all.hashed, all.unchanged), (2, 0));
}

/// A file that can't be read keeps its row (a flaky disk shouldn't make it
/// forget a match), and it is read again on the next scan.
#[test]
fn unreadable_files_keep_their_rows() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), roms.join("Test GB.gb")).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let file = roms.join("Test GB.gb");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&file).is_ok() {
        return; // running as root: can't make it unreadable
    }
    let report = lib
        .scan_with(
            &roms,
            Scope::All,
            ScanOptions {
                rehash: true,
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(report.errors.len(), 1, "{report:?}");
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files.len(), 1, "the row stays");
    assert_eq!(files[0].status, Status::Matched);
    // Readable again: hashed again, not taken as unchanged.
    let again = lib.scan(&roms, Scope::All).unwrap();
    assert_eq!((again.hashed, again.unchanged), (1, 0), "{again:?}");
}

/// Identify looks up only what changed: rescanning a folder with another
/// scope (no file hashed again) still moves every status to the new scope,
/// the same as identifying everything from scratch.
#[test]
fn incremental_identify_follows_a_new_scope() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    let snes = dat_id(&lib, "Nintendo - Super Nintendo");
    let report = lib.scan(fixtures().join("roms"), Scope::Dat(snes)).unwrap();
    assert_eq!(report.hashed, 0, "nothing changed on disk");
    // Statuses and the games each file is listed as (which read the stored
    // in-scope flag of every match).
    let snapshot = |lib: &Library| -> Vec<String> {
        lib.files(&LibraryFilter::default())
            .unwrap()
            .into_iter()
            .map(|f| format!("{} {:?} {:?}", f.display_path(), f.status, f.games))
            .collect()
    };
    let after = snapshot(&lib);
    let files = by_path(&lib);
    assert_eq!(files["snes/Test SNES (copier).smc"].status, Status::Matched);
    assert_eq!(files["nes/Test NES (iNES).nes"].status, Status::WrongSystem);
    lib.identify().unwrap();
    assert_eq!(after, snapshot(&lib), "the same as looking everything up again");
}
