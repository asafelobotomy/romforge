//! Milestone 2 end to end, always on copies in a temp folder.

use std::path::Path;

use romforge_core::organize::OrganizeOptions;
use romforge_core::{LibraryFilter, Scope, Status};

mod common;
use common::fixtures;
use common::organize::*;

#[test]
fn plan_apply_rescan_undo() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    let dest = tmp.path().join("sorted");
    copy_tree(&fixtures().join("roms"), &src);
    let before = listing(&src);

    let mut lib = library_with_fixture_dats();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert_eq!(listing(&src), before, "planning moves nothing");
    assert!(!dest.exists());

    let rel = |p: &Path| p.strip_prefix(&dest).unwrap().to_string_lossy().into_owned();
    let targets: Vec<String> = plan.moves.iter().map(|m| rel(&m.to)).collect();
    assert!(
        targets.contains(&"NES/Test NES (iNES).nes".to_owned()),
        "{targets:?}"
    );
    assert!(
        targets.contains(&"Game Boy Advance/Test GBA.zip".to_owned()),
        "{targets:?}"
    );
    let reasons: Vec<&str> = plan.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert!(
        reasons.iter().any(|r| r.starts_with("not matched (unknown")),
        "{reasons:?}"
    );
    assert!(
        reasons.iter().any(|r| r.starts_with("not matched (duplicate")),
        "{reasons:?}"
    );
    assert!(
        reasons.iter().any(|r| r.contains("different games")),
        "Mixed.7z holds several: {reasons:?}"
    );

    let applied = lib.organize_apply(&plan, &dest).unwrap();
    assert_eq!(applied.moved, plan.moves.len());
    assert!(applied.failed.is_empty(), "{:?}", applied.failed);
    for m in &plan.moves {
        assert!(!m.from.exists() && m.to.exists(), "{m:?}");
    }
    // The library follows the files, and a rescan of the new tree agrees.
    let matched_before = lib
        .files(&LibraryFilter {
            status: Some(Status::Matched),
            ..LibraryFilter::default()
        })
        .unwrap()
        .len();
    lib.scan(&dest, Scope::All).unwrap();
    let in_dest = lib
        .files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .filter(|f| {
            f.path
                .starts_with(&dest.canonicalize().unwrap().to_string_lossy().into_owned())
        })
        .filter(|f| f.status == Status::Matched)
        .count();
    assert!(in_dest > 0 && in_dest <= matched_before);
    // Nothing left to do the second time round.
    let again = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert!(again.moves.is_empty(), "{:?}", again.moves);

    let undone = lib.organize_undo(None).unwrap();
    assert_eq!(undone.restored, applied.moved);
    assert!(undone.failed.is_empty(), "{:?}", undone.failed);
    assert_eq!(listing(&src), before, "undo restores the tree exactly");
    assert!(listing(&dest).is_empty(), "and leaves no files behind");
    assert!(lib.organize_undo(None).is_err(), "nothing left to undo");
    let history = lib.organize_batches().unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].undone_at.is_some());
}

#[test]
fn never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    let dest = tmp.path().join("sorted");
    copy_tree(&fixtures().join("roms/nes"), &src);
    let mut lib = library_with_fixture_dats();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    let first = &plan.moves[0];
    std::fs::create_dir_all(first.to.parent().unwrap()).unwrap();
    std::fs::write(&first.to, b"someone else's file").unwrap();

    // Planned again: the occupied target is skipped.
    let replanned = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert!(replanned.moves.iter().all(|m| m.to != first.to));
    assert!(
        replanned
            .skipped
            .iter()
            .any(|s| s.reason.contains("already exists"))
    );
    // Applying the stale plan fails that move instead of clobbering.
    let applied = lib.organize_apply(&plan, &dest).unwrap();
    assert_eq!(applied.failed.len(), 1);
    assert_eq!(std::fs::read(&first.to).unwrap(), b"someone else's file");
    assert!(first.from.exists());
}

#[test]
fn bookkeeping_after_moves() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    let dest = tmp.path().join("sorted");
    copy_tree(&fixtures().join("roms/nes"), &src);
    std::fs::create_dir(&dest).unwrap();
    let mut lib = library_with_fixture_dats();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    let applied = lib.organize_apply(&plan, &dest).unwrap();
    assert!(applied.moved > 0);

    // Rescanning the folder the files came from keeps them in the library.
    lib.scan(&src, Scope::All).unwrap();
    let moved = lib
        .files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .filter(|f| {
            f.path
                .starts_with(&*dest.canonicalize().unwrap().to_string_lossy())
        })
        .count();
    assert_eq!(moved, applied.moved);

    // An apply with nothing to move leaves no batch for undo to land on.
    let again = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert!(again.moves.is_empty());
    assert_eq!(lib.organize_apply(&again, &dest).unwrap().batch_id, 0);
    assert_eq!(lib.organize_batches().unwrap().len(), 1);

    // Undo restores the real batch and leaves the destination folder (which
    // existed before) in place.
    let undone = lib.organize_undo(None).unwrap();
    assert_eq!(undone.batch_id, applied.batch_id);
    assert_eq!(undone.restored, applied.moved);
    assert!(dest.is_dir(), "the destination itself is never removed");
    assert!(
        std::fs::read_dir(&dest).unwrap().next().is_none(),
        "but its emptied folders are"
    );
}

#[test]
fn only_files_under_the_source_folder_move() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    copy_tree(&fixtures().join("roms/nes"), &a);
    copy_tree(&fixtures().join("roms/snes"), &b);
    let mut lib = library_with_fixture_dats();
    lib.scan(&a, Scope::All).unwrap();
    lib.scan(&b, Scope::All).unwrap();
    let plan = lib
        .organize_plan(&OrganizeOptions::new(&a, tmp.path().join("out")))
        .unwrap();
    assert!(!plan.moves.is_empty());
    let a = a.canonicalize().unwrap();
    assert!(plan.moves.iter().all(|m| m.from.starts_with(&a)), "{plan:?}");
    assert!(plan.skipped.iter().all(|s| s.path.starts_with(&a)));
    assert!(
        lib.organize_plan(&OrganizeOptions::new(tmp.path().join("nope"), tmp.path()))
            .is_err()
    );
}

/// A move that fails leaves nothing in the journal (it is recorded before
/// the move and dropped when the move fails): the file is where it was and
/// there is nothing to undo.
#[test]
fn failed_moves_leave_no_journal_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    copy_tree(&fixtures().join("roms/gb"), &src);
    let mut lib = library_with_fixture_dats();
    lib.scan(&src, Scope::All).unwrap();
    // The destination is a file, so no folder can be made in it.
    let dest = tmp.path().join("not-a-folder");
    std::fs::write(&dest, b"x").unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert!(!plan.moves.is_empty());
    let applied = lib.organize_apply(&plan, &dest).unwrap();
    assert_eq!(applied.moved, 0);
    assert_eq!(applied.failed.len(), plan.moves.len());
    for m in &plan.moves {
        assert!(m.from.exists(), "{} stayed", m.from.display());
    }
    // No move happened, so no batch was kept.
    let err = lib.organize_undo(None).unwrap_err().to_string();
    assert!(err.contains("nothing to undo"), "{err}");
}

/// A symlinked ROM is scanned and matched, but never moved: moving the
/// link would leave the file it points to behind.
#[cfg(unix)]
#[test]
fn symlinked_files_stay() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("elsewhere");
    let src = tmp.path().join("roms");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::create_dir_all(&src).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), real.join("Test GB.gb")).unwrap();
    std::os::unix::fs::symlink(real.join("Test GB.gb"), src.join("Test GB.gb")).unwrap();
    let mut lib = library_with_fixture_dats();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib
        .organize_plan(&OrganizeOptions::new(&src, tmp.path().join("sorted")))
        .unwrap();
    assert!(plan.moves.is_empty(), "{:?}", plan.moves);
    assert!(
        plan.skipped.iter().any(|s| s.reason == "a symbolic link"),
        "{:?}",
        plan.skipped
    );
}
