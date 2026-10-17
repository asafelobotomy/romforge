//! Milestone 2 end to end, always on copies in a temp folder.

use std::path::{Path, PathBuf};

use romforge_core::hash::hash_stream;
use romforge_core::organize::OrganizeOptions;
use romforge_core::{Library, LibraryFilter, Scope, Status};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let ty = e.file_type().unwrap();
        let dst = to.join(e.file_name());
        if ty.is_dir() {
            copy_tree(&e.path(), &dst);
        } else if ty.is_file() {
            std::fs::copy(e.path(), dst).unwrap();
        }
    }
}

/// (relative path, size) of every file under `dir`.
fn listing(dir: &Path) -> Vec<(String, u64)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                out.push((rel, e.metadata().unwrap().len()));
            }
        }
    }
    let mut out = Vec::new();
    if dir.exists() {
        walk(dir, dir, &mut out);
    }
    out.sort();
    out
}

fn library_with_fixture_dats() -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib
}

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

fn digest(d: &[u8]) -> romforge_core::hash::Digest {
    hash_stream(d, d.len() as u64).unwrap().raw
}

fn rom(name: &str, d: &[u8]) -> String {
    let h = digest(d);
    format!(
        r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
        h.size, h.crc32, h.md5, h.sha1
    )
}

#[test]
fn one_game_one_rom_keeps_the_best_version() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    std::fs::create_dir(&src).unwrap();
    let versions = [
        "Puzzle (Japan)",
        "Puzzle (USA)",
        "Puzzle (USA) (Rev 1)",
        "Puzzle (Europe)",
        "Puzzle (USA) (Beta)",
        "Other (Japan)",
    ];
    let mut games = String::new();
    for (i, name) in versions.iter().enumerate() {
        let data = vec![i as u8 + 1; 2048];
        games += &format!(
            r#"<game name="{name}">{}</game>"#,
            rom(&format!("{name}.gb"), &data)
        );
        std::fs::write(src.join(format!("{i}.gb")), &data).unwrap();
    }
    let dat = tmp.path().join("gb.dat");
    std::fs::write(
        &dat,
        format!(r#"<datafile><header><name>Nintendo - Game Boy</name></header>{games}</datafile>"#),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&src, Scope::All).unwrap();

    let mut opts = OrganizeOptions::new(&src, tmp.path().join("sorted"));
    opts.one_g1r = true;
    let names = |lib: &Library, opts: &OrganizeOptions| {
        let mut v: Vec<String> = lib
            .organize_plan(opts)
            .unwrap()
            .moves
            .into_iter()
            .map(|m| m.game)
            .collect();
        v.sort();
        v
    };
    assert_eq!(names(&lib, &opts), ["Other (Japan)", "Puzzle (USA) (Rev 1)"]);
    opts.regions = vec!["Europe".into()];
    assert_eq!(names(&lib, &opts), ["Other (Japan)", "Puzzle (Europe)"]);
    opts.one_g1r = false;
    assert_eq!(names(&lib, &opts).len(), versions.len());
}

#[test]
fn loose_disc_moves_only_as_a_complete_set() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    std::fs::create_dir(&src).unwrap();
    let track = vec![9u8; 4096];
    let cue = b"FILE \"Disc (Track 1).bin\" BINARY\n".to_vec();
    let dat = tmp.path().join("psx.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Disc (USA)">{}{}</game></datafile>"#,
            rom("Disc (USA).bin", &track),
            rom("Disc (USA).cue", &cue)
        ),
    )
    .unwrap();
    std::fs::write(src.join("disc.bin"), &track).unwrap();
    std::fs::write(src.join("disc.cue"), b"FILE \"disc.bin\" BINARY\r\n").unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let dest = tmp.path().join("sorted");
    let rel = |plan: &romforge_core::organize::Plan| {
        let mut v: Vec<String> = plan
            .moves
            .iter()
            .map(|m| m.to.strip_prefix(&dest).unwrap().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    // The user's own cue (not the DAT's) lists exactly the matched track: the
    // set moves as is, names kept, so that cue still works.
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert_eq!(
        rel(&plan),
        [
            "PlayStation/Disc (USA)/disc.bin",
            "PlayStation/Disc (USA)/disc.cue"
        ]
    );

    // A cue that also names a file that isn't a matched track: nothing moves.
    std::fs::write(
        src.join("disc.cue"),
        b"FILE \"disc.bin\" BINARY\nFILE \"other.bin\" BINARY\n",
    )
    .unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    assert!(
        plan.moves.is_empty(),
        "renaming the bin would break the user's cue: {plan:?}"
    );
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.reason.contains("break the cue sheet"))
    );

    // With the DAT's own cue sheet, the set moves into a game folder.
    std::fs::write(src.join("disc.cue"), &cue).unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib.organize_plan(&OrganizeOptions::new(&src, &dest)).unwrap();
    let mut to: Vec<String> = plan
        .moves
        .iter()
        .map(|m| m.to.strip_prefix(&dest).unwrap().to_string_lossy().into_owned())
        .collect();
    to.sort();
    assert_eq!(
        to,
        [
            "PlayStation/Disc (USA)/Disc (USA).bin",
            "PlayStation/Disc (USA)/Disc (USA).cue"
        ]
    );
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
fn one_g1r_counts_what_is_already_organized() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let usa = vec![1u8; 2048];
    let jpn = vec![2u8; 2048];
    std::fs::write(a.join("usa.gb"), &usa).unwrap();
    std::fs::write(b.join("jpn.gb"), &jpn).unwrap();
    let dat = tmp.path().join("gb.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - Game Boy</name></header><game name="Puzzle (USA)">{}</game><game name="Puzzle (Japan)">{}</game></datafile>"#,
            rom("Puzzle (USA).gb", &usa),
            rom("Puzzle (Japan).gb", &jpn)
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    let dest = tmp.path().join("sorted");
    let mut opts = OrganizeOptions::new(tmp.path(), &dest);
    opts.one_g1r = true;

    lib.scan(&a, Scope::All).unwrap();
    let plan = lib.organize_plan(&opts).unwrap();
    lib.organize_apply(&plan, &dest).unwrap();
    lib.scan(&dest, Scope::All).unwrap();
    // Later the Japanese version turns up: USA is already there, so it stays.
    lib.scan(&b, Scope::All).unwrap();
    let plan = lib.organize_plan(&opts).unwrap();
    assert!(plan.moves.is_empty(), "{:?}", plan.moves);
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.reason == "1G1R: keeping Puzzle (USA)")
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
