//! The one table of Emulator Settings (house rule 4). A setting is applied
//! when romforge starts an emulator (fullscreen, program, arguments), or
//! written into romforge's own copy of the emulator's config, under the
//! keys listed for it (`keys`); an emulator without such a key doesn't
//! show the setting. Config keys are checked in each emulator's own
//! config (RetroArch 1.22.2).

use crate::launch::KnownEmulator;

/// What a setting holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `true` or `false`.
    Bool,
    /// One of `choices`.
    Choice,
    /// A program on this computer.
    Path,
    /// Command-line arguments, split at spaces.
    Args,
}

/// One setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: Kind,
    /// It can be set for every emulator at once.
    pub global: bool,
    /// The value when nothing is set; empty: whatever the emulator's own
    /// config says.
    pub default: &'static str,
    /// For [`Kind::Choice`]: value and label.
    pub choices: &'static [(&'static str, &'static str)],
    /// Where it is written: (emulator, key in its config). Every key listed
    /// for an emulator gets the value.
    pub keys: &'static [(&'static str, &'static str)],
}

const fn config(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    global: bool,
    keys: &'static [(&'static str, &'static str)],
) -> Setting {
    Setting {
        key,
        label,
        hint,
        kind: Kind::Bool,
        global,
        default: "",
        choices: &[],
        keys,
    }
}

pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "hotkeys",
        label: "Game pad hotkeys",
        hint: "romforge's own hotkeys while a game runs (Select+Start quits; the Hotkeys tab lists them all). The emulators' own pad hotkeys stay off.",
        kind: Kind::Bool,
        global: true,
        default: "true",
        choices: &[],
        keys: &[],
    },
    Setting {
        key: "fullscreen",
        label: "Fullscreen",
        hint: "Start games fullscreen, without the emulator's own window and menus.",
        kind: Kind::Bool,
        global: true,
        default: "true",
        choices: &[],
        keys: &[],
    },
    Setting {
        key: "program",
        label: "Program",
        hint: "Start this program instead of the copy romforge finds.",
        kind: Kind::Path,
        global: false,
        default: "",
        choices: &[],
        keys: &[],
    },
    Setting {
        key: "args",
        label: "Extra arguments",
        hint: "Added before the game's own arguments, split at spaces (no quoting).",
        kind: Kind::Args,
        global: false,
        default: "",
        choices: &[],
        keys: &[],
    },
    Setting {
        key: "video_driver",
        label: "Video driver",
        hint: "How the game is drawn. Vulkan suits most current graphics cards; OpenGL works almost everywhere.",
        kind: Kind::Choice,
        global: false,
        default: "",
        choices: &[("gl", "OpenGL"), ("glcore", "OpenGL core"), ("vulkan", "Vulkan")],
        keys: &[("retroarch", "video_driver")],
    },
    config(
        "vsync",
        "VSync",
        "Wait for the screen's refresh: no tearing, a little more input lag.",
        true,
        &[("retroarch", "video_vsync")],
    ),
    config(
        "smooth",
        "Smooth picture",
        "Bilinear filtering: softer pixels instead of sharp ones.",
        true,
        &[("retroarch", "video_smooth")],
    ),
    config(
        "integer_scale",
        "Integer scaling",
        "Scale only by whole numbers, so every pixel is the same size (borders around the picture).",
        true,
        &[("retroarch", "video_scale_integer")],
    ),
    config(
        "rewind",
        "Rewind",
        "Keep the last moments of play to step back through (uses memory and some speed).",
        true,
        &[("retroarch", "rewind_enable")],
    ),
    config(
        "run_ahead",
        "Run-ahead",
        "Run a frame ahead to cut input lag (needs a faster computer).",
        true,
        &[("retroarch", "run_ahead_enabled")],
    ),
    config(
        "auto_state",
        "Resume where you left off",
        "Save a state when the game closes and load it when it starts again.",
        true,
        &[
            ("retroarch", "savestate_auto_save"),
            ("retroarch", "savestate_auto_load"),
        ],
    ),
    config(
        "show_fps",
        "Show FPS",
        "Show frames per second over the game.",
        true,
        &[("retroarch", "fps_show")],
    ),
];

pub(super) fn setting(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.key == key)
}

/// Whether `s` means anything for `e`.
pub(super) fn applies(s: &Setting, e: &KnownEmulator) -> bool {
    match s.key {
        "fullscreen" => !e.fullscreen.is_empty(),
        "program" | "args" => true,
        _ => s.keys.iter().any(|(name, _)| *name == e.settings_name()),
    }
}
