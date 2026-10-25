//! Where each emulator keeps the curated settings (its files, from
//! romforge's config folder for it), and table shorthand for its keys.

use super::{Format, Key, Words, k};

pub(super) const TF: Words = Words::Bool("true", "false");
pub(super) const PY: Words = Words::Bool("True", "False");
pub(super) const YN: Words = Words::Bool("yes", "no");
pub(super) const ONE: Words = Words::Bool("1", "0");
pub(super) const SAME: Words = Words::Same;

// Each emulator's files, from romforge's config folder for it.
pub(super) const DS: &str = "duckstation/settings.ini";
pub(super) const PCSX2: &str = "PCSX2/inis/PCSX2.ini";
pub(super) const DOLPHIN: &str = "Config/Dolphin.ini";
pub(super) const GFX: &str = "Config/GFX.ini";
pub(super) const PPSSPP: &str = "ppsspp/PSP/SYSTEM/ppsspp.ini";
pub(super) const FLYCAST: &str = "flycast/emu.cfg";
pub(super) const MGBA: &str = "mgba/config.ini";
pub(super) const MGBA_QT: &str = "mgba/qt.ini";
pub(super) const MELONDS: &str = "melonDS/melonDS.toml";
pub(super) const XEMU: &str = "xemu.toml";
pub(super) const RPCS3: &str = "rpcs3/config.yml";
pub(super) const CEMU: &str = "Cemu/settings.xml";
pub(super) const MEDNAFEN: &str = "mednafen.cfg";
pub(super) const STAGING: &str = "dosbox-staging.conf";
pub(super) const DOSBOX_X: &str = "dosbox-x.conf";
pub(super) const SCUMMVM: &str = "scummvm.ini";
pub(super) const MAME: &str = "mame.ini";

pub(super) const fn ini(e: &'static str, file: &'static str, key: &'static str, w: Words) -> Key {
    k(e, file, key, Format::Ini, w)
}
pub(super) const fn toml(e: &'static str, file: &'static str, key: &'static str, w: Words) -> Key {
    k(e, file, key, Format::Toml, w)
}
pub(super) const fn spaced(e: &'static str, file: &'static str, key: &'static str, w: Words) -> Key {
    k(e, file, key, Format::Spaced, w)
}
pub(super) const fn rpcs3(key: &'static str, w: Words) -> Key {
    k("rpcs3", RPCS3, key, Format::NestedYaml, w)
}
pub(super) const fn cemu(key: &'static str, w: Words) -> Key {
    k("cemu", CEMU, key, Format::Xml, w)
}
pub(super) const fn ra(key: &'static str) -> Key {
    k("retroarch", "retroarch.cfg", key, Format::RetroArch, SAME)
}

/// The Citra line's `qt-config.ini`, the same keys in each.
macro_rules! citra_keys {
    ($key:literal, $w:expr) => {
        [
            super::k(
                "azahar",
                "azahar-emu/qt-config.ini",
                $key,
                $crate::launch::Format::QtIni,
                $w,
            ),
            super::k(
                "lime3ds",
                "lime3ds-emu/qt-config.ini",
                $key,
                $crate::launch::Format::QtIni,
                $w,
            ),
            super::k(
                "citra",
                "citra-emu/qt-config.ini",
                $key,
                $crate::launch::Format::QtIni,
                $w,
            ),
        ]
    };
}

pub(super) use citra_keys as citra;
