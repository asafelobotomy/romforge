//! A patch goes only on the ROM the plan saw. Always on copies in temp
//! folders.

use romforge_core::home::Home;
use romforge_core::view::Relation;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::patcher::*;

/// An IPS has no check of its own: a ROM changed since the plan is
/// refused, and nothing is written.
#[test]
fn an_ips_on_a_rom_changed_since_the_plan_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let rom = incoming.join("Test GB.gb");
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), &rom).unwrap();
    let patch = tmp.path().join("Test GB English.ips");
    std::fs::write(&patch, ips(0, b"HELLO")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let base = row_id(&lib, "Test GB");
    let plan = lib
        .patch_plan(&base, &patch, Some(Relation::Translation), Some("English"))
        .unwrap();

    let mut data = std::fs::read(&rom).unwrap();
    data[100] ^= 0xff;
    std::fs::write(&rom, &data).unwrap();
    let err = lib.patch_apply(&plan).unwrap_err().to_string();
    assert!(err.contains("changed since the plan"), "{err}");
    assert!(!plan.target.exists(), "nothing written");
    assert!(lib.cleanup_batches().unwrap().is_empty(), "no batch left");
}
