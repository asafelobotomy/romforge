//! Opening a file that isn't in the library (from a file manager).
#![cfg(unix)]

use romforge_core::Library;
use romforge_core::launch::Env;

#[test]
fn a_file_outside_the_library_is_started_by_its_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = tmp.path().join("Some Game.sfc");
    std::fs::write(&rom, vec![1u8; 2048]).unwrap();
    let lib = Library::open_in_memory().unwrap();
    // No emulator is installed in the test environment: the plan fails on
    // that, having understood the file (a SNES game), not on "no matched file".
    match lib.launch_plan(&Env::default(), rom.to_str().unwrap(), None) {
        Ok(spec) => assert_eq!((spec.game.as_str(), spec.system.as_str()), ("Some Game", "SNES")),
        Err(e) => assert!(!e.to_string().contains("no matched file"), "{e}"),
    }
}

#[test]
fn a_shared_extension_is_not_guessed() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = tmp.path().join("disc.bin");
    std::fs::write(&rom, vec![1u8; 2048]).unwrap();
    let lib = Library::open_in_memory().unwrap();
    let err = lib
        .launch_plan(&Env::default(), rom.to_str().unwrap(), None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no matched file"), "{err}");
}
