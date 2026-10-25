//! What romforge applies when it starts an emulator, and RetroArch's own
//! settings (`retroarch.cfg`, checked in RetroArch 1.22.2).

use super::{Format, Key, Kind, Setting, Words, config, k};

const fn ra(key: &'static str) -> Key {
    k(
        "retroarch",
        crate::launch::files::RETROARCH,
        key,
        Format::RetroArch,
        Words::Same,
    )
}

pub(super) const LAUNCH: &[Setting] = &[
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
        keys: &[ra("video_driver")],
    },
    config(
        "smooth",
        "Smooth picture",
        "Bilinear filtering: softer pixels instead of sharp ones.",
        &[ra("video_smooth")],
    ),
    config(
        "integer_scale",
        "Integer scaling",
        "Scale only by whole numbers, so every pixel is the same size (borders around the picture).",
        &[ra("video_scale_integer")],
    ),
    config(
        "rewind",
        "Rewind",
        "Keep the last moments of play to step back through (uses memory and some speed).",
        &[ra("rewind_enable")],
    ),
    config(
        "run_ahead",
        "Run-ahead",
        "Run a frame ahead to cut input lag (needs a faster computer).",
        &[ra("run_ahead_enabled")],
    ),
    config(
        "auto_state",
        "Resume where you left off",
        "Save a state when the game closes and load it when it starts again.",
        &[ra("savestate_auto_save"), ra("savestate_auto_load")],
    ),
];
