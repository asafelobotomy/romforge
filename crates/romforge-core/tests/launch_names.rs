//! Launching by name: a game is a title on a system.
#![cfg(unix)]

use romforge_core::hash::hash_stream;
use romforge_core::launch::Env;
use romforge_core::{Library, Scope};

/// One DAT with a game called `title` whose one ROM is `data`.
fn dat(dir: &std::path::Path, system: &str, title: &str, data: &[u8]) -> std::path::PathBuf {
    let h = hash_stream(data, data.len() as u64).unwrap().raw;
    let p = dir.join(format!("{}.dat", h.sha1));
    std::fs::write(
        &p,
        format!(
            r#"<datafile><header><name>{system}</name></header><game name="{title}"><rom name="{title}.bin" size="{}" sha1="{}"/></game></datafile>"#,
            h.size, h.sha1
        ),
    )
    .unwrap();
    p
}

#[test]
fn the_same_title_on_two_systems_is_two_games() {
    let tmp = tempfile::tempdir().unwrap();
    let roms = tmp.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    let (a, b) = (vec![1u8; 2048], vec![2u8; 4096]);
    std::fs::write(roms.join("a.bin"), &a).unwrap();
    std::fs::write(roms.join("b.bin"), &b).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dat(tmp.path(), "Nintendo - Game Boy", "Same Game", &a), None)
        .unwrap();
    lib.import_dat(
        dat(tmp.path(), "Sega - Mega Drive - Genesis", "Same Game", &b),
        None,
    )
    .unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let err = lib
        .launch_plan(&Env::default(), "Same Game", None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("several games"), "{err}");
    assert!(err.contains("Same Game (") && err.contains(" | "), "{err}");
}

/// A file identical in two regions' games (an audio track) doesn't make
/// the other region's game, which you don't have, one Play offers.
#[test]
fn a_shared_track_does_not_offer_a_game_you_do_not_have() {
    let tmp = tempfile::tempdir().unwrap();
    let roms = tmp.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    let (data, other, shared) = (vec![1u8; 2048], vec![2u8; 2048], vec![3u8; 4096]);
    std::fs::write(roms.join("Disc (USA) (Track 1).bin"), &data).unwrap();
    std::fs::write(roms.join("Disc (USA) (Track 2).bin"), &shared).unwrap();
    let rom = |name: &str, d: &[u8]| {
        let h = hash_stream(d, d.len() as u64).unwrap().raw;
        format!(r#"<rom name="{name}" size="{}" sha1="{}"/>"#, h.size, h.sha1)
    };
    let game = |region: &str, first: &[u8]| {
        format!(
            r#"<game name="Disc ({region})">{}{}</game>"#,
            rom(&format!("Disc ({region}) (Track 1).bin"), first),
            rom(&format!("Disc ({region}) (Track 2).bin"), &shared)
        )
    };
    let p = tmp.path().join("ps1.dat");
    std::fs::write(
        &p,
        format!(
            "<datafile><header><name>Sony - PlayStation</name></header>{}{}</datafile>",
            game("USA", &data),
            game("Europe", &other)
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&p, None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let err = lib
        .launch_plan(&Env::default(), "Disc", None)
        .unwrap_err()
        .to_string();
    assert!(!err.contains("several games"), "{err}");
}
