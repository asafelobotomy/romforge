//! Moves and patches that never lose or misplace a file. Always on copies
//! in temp folders.

use romforge_core::home::Home;
use romforge_core::organize::OrganizeOptions;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::home::write_multi_dat;
use common::patcher::{ips, row_id};

/// A move whose copy is in place but whose original can't be removed
/// (a read-only folder) keeps its journal row, and undo then just removes
/// the copy instead of failing on the original's place being taken.
#[cfg(unix)]
#[test]
fn a_move_that_leaves_its_original_can_be_undone() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let (src, dest) = (tmp.path().join("src"), tmp.path().join("dest"));
    std::fs::create_dir_all(&src).unwrap();
    let rom = src.join("Test GB.gb");
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), &rom).unwrap();
    let mut lib = Library::open(tmp.path().join("romforge.db")).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert_eq!(plan.moves.len(), 1, "{plan:?}");
    let to = plan.moves[0].to.clone();

    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(src.join("probe"), b"").is_ok() {
        // Running as root: a read-only folder doesn't stop anything.
        std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let applied = lib.organize_apply(&plan, &dest);
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o755)).unwrap();
    let applied = applied.unwrap();
    assert_eq!((applied.moved, applied.failed.len()), (0, 1), "{applied:?}");
    assert!(applied.failed[0].1.contains("couldn't be removed"), "{applied:?}");
    assert!(applied.batch_id != 0, "the batch stays for undo");
    assert!(rom.is_file() && to.is_file());
    let undone = lib.organize_undo(None).unwrap();
    assert!(undone.failed.is_empty(), "{undone:?}");
    assert_eq!(undone.restored, 1);
    assert!(rom.is_file() && !to.exists(), "only the copy goes");
}

/// A DAT's ROM names are untrusted: a patch is refused for a game whose
/// names would climb out of the new zip.
#[test]
fn a_patch_refuses_unsafe_rom_names() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let (a, b) = (vec![3u8; 2048], vec![4u8; 4096]);
    let dat = tmp.path().join("two.dat");
    write_multi_dat(
        &dat,
        &[("Two (USA)", vec![("../evil.gb", &a[..]), ("ok.gb", &b[..])])],
    );
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::write(incoming.join("a.gb"), &a).unwrap();
    std::fs::write(incoming.join("b.gb"), &b).unwrap();
    let patch = tmp.path().join("Two fix.ips");
    std::fs::write(&patch, ips(0, b"HI")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, Some("Game Boy")).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    assert!(
        rows.iter()
            .any(|r| r.title == "Two (USA)" && r.status == RowStatus::Verified),
        "{rows:?}"
    );
    let e = lib
        .patch_plan(&row_id(&lib, "Two (USA)"), &patch, None, None)
        .unwrap_err();
    assert!(e.to_string().contains("can't go in an archive safely"), "{e}");
}
