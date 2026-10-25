//! Where emulators may come from: only their developers' own builds.

use super::*;

/// Only an emulator's official Flatpak is used: Flathub's RPCS3 (its
/// developers make none) never is, its AppImage is; Dolphin's comes from
/// its own repository, so Flathub's community build isn't used either.
#[test]
fn only_official_flatpaks_are_used() {
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
    assert_eq!(env.find(rpcs3), None, "Flathub's isn't its developers'");
    assert!(env.unofficial_flatpak(rpcs3));
    let image = apps.join("rpcs3-v0.0.43-20397_linux64.AppImage");
    program(&image);
    assert_eq!(env.find(rpcs3), Some(image.display().to_string()));

    let dolphin = known("dolphin").unwrap();
    flatpak::fake(&dir, "org.DolphinEmu.dolphin-emu", flatpak::FLATHUB_URL);
    assert_eq!(env.find(dolphin), None, "Flathub's community build");
    assert!(env.unofficial_flatpak(dolphin));
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
