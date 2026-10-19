//! Milestone 2 end to end, always on copies in a temp folder.
//!
//! Loose discs.

use romforge_core::organize::OrganizeOptions;
use romforge_core::{Library, Scope};

mod common;
use common::organize::*;

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

/// A GDI disc set is held back like a cue one: its tracks never move
/// without their sheet.
#[test]
fn loose_gdi_tracks_wait_for_their_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    std::fs::create_dir(&src).unwrap();
    let track = vec![7u8; 4096];
    let gdi = b"1\n1 0 4 2352 \"Disc (Track 1).bin\" 0\n".to_vec();
    let dat = tmp.path().join("dc.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sega - Dreamcast</name></header><game name="Disc (USA)">{}{}</game></datafile>"#,
            rom("Disc (USA) (Track 1).bin", &track),
            rom("Disc (USA).gdi", &gdi)
        ),
    )
    .unwrap();
    std::fs::write(src.join("track01.bin"), &track).unwrap();
    std::fs::write(
        src.join("disc.gdi"),
        b"2\n1 0 4 2352 track01.bin 0\n2 600 0 2352 track02.raw 0\n",
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let plan = lib
        .organize_plan(&OrganizeOptions::new(&src, tmp.path().join("sorted")))
        .unwrap();
    assert!(
        plan.moves.is_empty(),
        "renaming the track breaks the sheet: {plan:?}"
    );
}
