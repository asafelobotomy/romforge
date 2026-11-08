//! Where emulators come from: their developers' own builds first; a
//! Flatpak made by others only when nothing official is installed.

use super::*;

/// An emulator's official build wins: RPCS3's AppImage over Flathub's
/// RPCS3 (its developers make no Flatpak), Dolphin's own Flatpak over
/// Flathub's community one. With nothing official, the one installed is
/// used and said to be others'.
#[test]
fn official_builds_come_first_and_others_flatpaks_last() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("flatpak");
    flatpak::fake(&dir, "net.rpcs3.RPCS3", flatpak::FLATHUB_URL);
    let apps = t.path().join("apps");
    std::fs::create_dir_all(&apps).unwrap();
    let env = Env {
        flatpak_dirs: vec![dir.clone()],
        emulator_dirs: vec![apps.clone()],
        ..Default::default()
    };
    let rpcs3 = known("rpcs3").unwrap();
    assert_eq!(
        env.find(rpcs3).as_deref(),
        Some("flatpak run net.rpcs3.RPCS3"),
        "nothing official: Flathub's, said so"
    );
    assert!(env.unofficial_flatpak(rpcs3) && env.runs_others_flatpak(rpcs3));
    let image = apps.join("rpcs3-v0.0.43-20397_linux64.AppImage");
    program(&image);
    assert_eq!(
        env.find(rpcs3),
        Some(image.display().to_string()),
        "its AppImage first"
    );
    assert!(!env.runs_others_flatpak(rpcs3));

    let dolphin = known("dolphin").unwrap();
    flatpak::fake(&dir, "org.DolphinEmu.dolphin-emu", flatpak::FLATHUB_URL);
    assert!(
        env.runs_others_flatpak(dolphin),
        "Flathub's community build, as nothing else is"
    );
    let t2 = tempfile::tempdir().unwrap();
    let own = t2.path().join("flatpak");
    flatpak::fake(
        &own,
        "org.DolphinEmu.dolphin-emu",
        "https://flatpak.dolphin-emu.org/releases/",
    );
    let env = Env {
        flatpak_dirs: vec![own],
        ..Default::default()
    };
    assert_eq!(
        env.find(dolphin).as_deref(),
        Some("flatpak run org.DolphinEmu.dolphin-emu")
    );
    assert!(!env.unofficial_flatpak(dolphin));
}

/// MAME makes no Linux build: Flathub's is the fallback, after a MAME of
/// your own; Mednafen (source only, not on Flathub) has none.
#[test]
fn an_emulator_without_a_linux_build_falls_back_to_the_most_used_one() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("flatpak");
    flatpak::fake(&dir, "org.mamedev.MAME", flatpak::FLATHUB_URL);
    let mut env = Env {
        flatpak_dirs: vec![dir],
        ..Default::default()
    };
    let mame = known("mame").unwrap();
    assert_eq!(env.find(mame).as_deref(), Some("flatpak run org.mamedev.MAME"));
    assert!(env.runs_fallback(mame) && !env.unofficial_flatpak(mame));
    assert!(env.runs_fallback(known("mame-fm7").unwrap()), "its machines too");
    let bin = t.path().join("bin");
    program(&bin.join("mame"));
    env.path = vec![bin.clone()];
    assert_eq!(
        env.find(mame),
        Some(bin.join("mame").display().to_string()),
        "yours first"
    );
    assert!(!env.runs_fallback(mame));
    assert_eq!(known("mednafen").unwrap().flatpak_from, FlatpakFrom::Flathub);
    assert_eq!(known("mednafen").unwrap().flatpak, None);
}
