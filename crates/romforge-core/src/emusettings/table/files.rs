//! Where each emulator keeps the curated settings (its files, from
//! romforge's config folder for it), and table shorthand for its keys.

use super::{Format, Key, Words, k};

pub(super) const TF: Words = Words::Bool("true", "false");
pub(super) const PY: Words = Words::Bool("True", "False");
pub(super) const YN: Words = Words::Bool("yes", "no");
pub(super) const ONE: Words = Words::Bool("1", "0");
pub(super) const SAME: Words = Words::Same;

// Each emulator's files: the one list (`launch/owned/files.rs`).
pub(super) use crate::launch::files::*;

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
    k("retroarch", RETROARCH, key, Format::RetroArch, SAME)
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
