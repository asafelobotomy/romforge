//! BIOS files you already have elsewhere on this computer, in the usual
//! places only: RetroArch's system folder, EmuDeck's and RetroDECK's BIOS
//! folders, and the standalone emulators' own BIOS folders, each also in
//! its Flatpak's folder. They are offered as an import into `Bios/` (the
//! same plan as a drop); nothing here is ever changed by looking.

use std::path::PathBuf;

use crate::launch::existing::UserDirs;

/// Where a BIOS folder (or file) usually is.
enum Base {
    /// Under `$XDG_CONFIG_HOME` (and the Flatpak's `config/`).
    Config,
    /// Under `$XDG_DATA_HOME` (and the Flatpak's `data/`).
    Data,
    /// Under the home folder.
    Home,
}

/// (what it is, where, path, Flatpak id). RetroArch's system folder is its
/// default `system_directory`; DuckStation's `bios/` its data folder's;
/// PCSX2's `bios/` its config folder's; the 3DS emulators' `sysdata/` their
/// data folder's (milestone 24); Cemu's keys its data folder's; Dolphin's
/// GameCube IPLs its `GC/`.
const PLACES: &[(&str, Base, &str, Option<&str>)] = &[
    (
        "RetroArch",
        Base::Config,
        "retroarch/system",
        Some("org.libretro.RetroArch"),
    ),
    ("EmuDeck", Base::Home, "Emulation/bios", None),
    ("RetroDECK", Base::Home, "retrodeck/bios", None),
    (
        "DuckStation",
        Base::Data,
        "duckstation/bios",
        Some("org.duckstation.DuckStation"),
    ),
    ("PCSX2", Base::Config, "PCSX2/bios", Some("net.pcsx2.PCSX2")),
    (
        "Azahar",
        Base::Data,
        "azahar-emu/sysdata",
        Some("org.azahar_emu.Azahar"),
    ),
    (
        "Lime3DS",
        Base::Data,
        "lime3ds-emu/sysdata",
        Some("io.github.lime3ds.Lime3DS"),
    ),
    (
        "Citra",
        Base::Data,
        "citra-emu/sysdata",
        Some("org.citra_emu.citra"),
    ),
    ("Cemu", Base::Data, "Cemu/keys.txt", Some("info.cemu.Cemu")),
    ("Cemu", Base::Data, "Cemu/otp.bin", Some("info.cemu.Cemu")),
    ("Cemu", Base::Data, "Cemu/seeprom.bin", Some("info.cemu.Cemu")),
    (
        "Dolphin",
        Base::Data,
        "dolphin-emu/GC",
        Some("org.DolphinEmu.dolphin-emu"),
    ),
];

/// The usual BIOS places that exist for `user`, as (what it is, path).
pub fn usual_places(user: &UserDirs) -> Vec<(&'static str, PathBuf)> {
    let mut out = Vec::new();
    for (what, base, rel, flatpak) in PLACES {
        let (native, sub) = match base {
            Base::Config => (user.xdg.config.join(rel), Some("config")),
            Base::Data => (user.xdg.data.join(rel), Some("data")),
            Base::Home => (user.home.join(rel), None),
        };
        let boxed = flatpak
            .zip(sub)
            .map(|(id, sub)| user.home.join(".var/app").join(id).join(sub).join(rel));
        for p in std::iter::once(native).chain(boxed) {
            if p.exists() && !out.iter().any(|(_, q)| *q == p) {
                out.push((*what, p));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_usual_places_that_exist() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("u");
        let user = UserDirs {
            home: home.clone(),
            xdg: crate::launch::existing::Xdg {
                config: home.join(".config"),
                data: home.join(".local/share"),
            },
        };
        for d in [
            ".config/retroarch/system",
            "Emulation/bios",
            ".var/app/org.duckstation.DuckStation/data/duckstation/bios",
        ] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        std::fs::create_dir_all(home.join(".local/share/Cemu")).unwrap();
        std::fs::write(home.join(".local/share/Cemu/keys.txt"), "k").unwrap();
        let found: Vec<(&str, PathBuf)> = usual_places(&user);
        let rel: Vec<(&str, String)> = found
            .iter()
            .map(|(w, p)| (*w, p.strip_prefix(&home).unwrap().display().to_string()))
            .collect();
        assert_eq!(
            rel,
            [
                ("RetroArch", ".config/retroarch/system".to_owned()),
                ("EmuDeck", "Emulation/bios".to_owned()),
                (
                    "DuckStation",
                    ".var/app/org.duckstation.DuckStation/data/duckstation/bios".to_owned()
                ),
                ("Cemu", ".local/share/Cemu/keys.txt".to_owned()),
            ]
        );
    }
}
