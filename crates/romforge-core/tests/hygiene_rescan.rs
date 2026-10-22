//! Rescans and re-imports that must not lose or widen what is known: an
//! unreadable folder keeps its files, Scan folder keeps a folder's scope,
//! the same DAT for another system moves to it, and a batch that panics
//! leaves the library out of batch mode.

use std::path::Path;

use romforge_core::{Library, LibraryFilter, Scope, Status, systems};

mod common;
use common::fixtures;

fn copy_gb_roms(to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(fixtures().join("roms/gb")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, to.join(p.file_name().unwrap())).unwrap();
    }
}

fn statuses_under(lib: &Library, dir: &Path) -> Vec<Status> {
    let dir = std::fs::canonicalize(dir).unwrap();
    lib.files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .filter(|f| Path::new(&f.path).starts_with(&dir))
        .map(|f| f.status)
        .collect()
}

/// A subfolder the walk can't read (no permission) keeps the rows of its
/// files: a flaky disk or a permission slip doesn't forget them.
#[cfg(unix)]
#[test]
fn an_unreadable_folder_keeps_its_files() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    let inner = roms.join("inner");
    copy_gb_roms(&inner);
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let before = statuses_under(&lib, &inner);
    assert!(!before.is_empty());

    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads it anyway: nothing to check then.
    let readable = std::fs::read_dir(&inner).is_ok();
    let report = lib.scan(&roms, Scope::All);
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = report.unwrap();
    if readable {
        return;
    }
    assert!(!report.errors.is_empty(), "the folder is reported");
    assert_eq!(statuses_under(&lib, &inner), before, "its files are kept");

    // Readable again and a file gone: that one goes.
    std::fs::remove_file(inner.join("Test GB.gb")).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    assert_eq!(statuses_under(&lib, &inner).len(), before.len() - 1);
}

/// Scan folder (and its preview) on a folder scanned for one system keeps
/// that scope instead of widening it to every DAT.
#[test]
fn scan_folder_keeps_the_folder_s_scope() {
    let dir = tempfile::tempdir().unwrap();
    let gb = dir.path().join("gb");
    copy_gb_roms(&gb);
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let nes = systems::by_key("NES").unwrap().id;
    lib.scan(&gb, Scope::System(nes)).unwrap();
    let scoped = statuses_under(&lib, &gb);
    assert!(
        !scoped.is_empty() && !scoped.contains(&Status::Matched),
        "{scoped:?}"
    );

    let preview = lib.scan_preview(&gb).unwrap();
    assert!(
        preview
            .games
            .iter()
            .all(|g| g.status != romforge_core::view::RowStatus::Verified),
        "the preview keeps the scope too"
    );
    lib.scan_folder(&gb).unwrap();
    assert_eq!(statuses_under(&lib, &gb), scoped, "after Scan folder");
    // A folder inside it takes the scope of the folder holding it.
    let inner = gb.join("inner");
    copy_gb_roms(&inner);
    lib.scan_folder(&inner).unwrap();
    assert!(!statuses_under(&lib, &inner).contains(&Status::Matched));
}

/// The same DAT file imported again for another system (named by hand)
/// moves to that system; again for the same one it is unchanged.
#[test]
fn the_same_dat_for_another_system_moves() {
    let mut lib = Library::open_in_memory().unwrap();
    let dat = fixtures().join("dats/gb.dat");
    let first = lib.import_dat(&dat, None).unwrap();
    assert_eq!(first.system, Some("Game Boy"));
    let again = lib.import_dat(&dat, None).unwrap();
    assert!(again.unchanged);
    assert_eq!(again.system, Some("Game Boy"));

    let moved = lib.import_dat(&dat, Some("Game Boy Color")).unwrap();
    assert!(!moved.unchanged);
    assert_eq!(moved.system, Some("Game Boy Color"));
    assert_eq!(moved.replaced, Some(first.dat_id));
    let with_dats: Vec<String> = lib.systems_in_use().unwrap().into_iter().map(|s| s.key).collect();
    assert_eq!(with_dats, ["Game Boy Color"]);
    let same = lib.import_dat(&dat, Some("Game Boy Color")).unwrap();
    assert!(same.unchanged);
}

/// A batch whose closure panics still leaves batch mode: a DAT imported
/// afterwards identifies at once.
#[test]
fn a_panicking_batch_leaves_batch_mode() {
    let dir = tempfile::tempdir().unwrap();
    let gb = dir.path().join("gb");
    copy_gb_roms(&gb);
    let mut lib = Library::open_in_memory().unwrap();
    lib.scan(&gb, Scope::All).unwrap();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = lib.batch(|_| -> Result<(), romforge_core::Error> { panic!("in the batch") });
    }));
    assert!(crashed.is_err());
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    assert!(statuses_under(&lib, &gb).contains(&Status::Matched));
}
