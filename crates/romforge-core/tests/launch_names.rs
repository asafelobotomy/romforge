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
