//! Picture settings, for every emulator that has them, each key in its
//! own file's words. Read in the source of: DuckStation (2026-10), PCSX2
//! 2.8.2, Dolphin 2609a, PPSSPP 1.20.4, Flycast 2.7, mGBA 0.10.5, melonDS
//! 1.1, xemu 0.8.139, RPCS3 0.0.43, Cemu 2.6, Azahar 2126.2 (Lime3DS and
//! Citra the same), Mednafen 1.32.1, DOSBox Staging 0.83.0, DOSBox-X
//! 2026.10.01, ScummVM 2026.4.0 and MAME 0.289.

use super::files::*;
use super::renderer::{RENDERER, RENDERERS};
use super::{Key, Setting, Words, choice, concat, config};

const SCALES: &[(&str, &str)] = &[
    ("1", "Native"),
    ("2", "2×"),
    ("3", "3×"),
    ("4", "4×"),
    ("5", "5×"),
    ("6", "6×"),
    ("8", "8×"),
];

const RESOLUTION: [Key; 12] = concat(&[
    &[
        ini("duckstation", DS, "[GPU]ResolutionScale", SAME),
        ini("pcsx2", PCSX2, "[EmuCore/GS]upscale_multiplier", SAME),
        ini("dolphin", GFX, "[Settings]InternalResolution", SAME),
        ini("ppsspp", PPSSPP, "[Graphics]InternalResolution", SAME),
        ini("flycast", FLYCAST, "[config]rend.Resolution", Words::Times(480)),
        toml("melonds", MELONDS, "[3D.GL]ScaleFactor", SAME),
        ini("mgba", MGBA, "[ports.qt]videoScale", SAME),
        toml("xemu", XEMU, "[display.quality]surface_scale", SAME),
        rpcs3("Video/Resolution Scale", Words::Times(100)),
    ],
    &citra!("[Renderer]resolution_factor", SAME),
]);

const VSYNC: [Key; 19] = concat(&[
    &[
        ra("video_vsync"),
        ini("duckstation", DS, "[Display]VSync", TF),
        ini("pcsx2", PCSX2, "[EmuCore/GS]VsyncEnable", TF),
        ini("dolphin", GFX, "[Hardware]VSync", PY),
        ini("ppsspp", PPSSPP, "[Graphics]VerticalSync", PY),
        ini("flycast", FLYCAST, "[config]rend.vsync", YN),
        toml("melonds", MELONDS, "[Screen]VSync", TF),
        ini("mgba", MGBA, "[ports.qt]videoSync", ONE),
        toml("xemu", XEMU, "[display.window]vsync", TF),
        rpcs3("Video/VSync Mode", Words::Bool("Full", "Disabled")),
        cemu("Graphic/VSync", ONE),
        spaced("mednafen", MEDNAFEN, "video.glvsync", ONE),
        ini("dosbox-staging", STAGING, "[sdl]vsync", Words::Bool("on", "off")),
        ini("dosbox-x", DOSBOX_X, "[vsync]vsyncmode", Words::Bool("on", "off")),
        ini("scummvm", SCUMMVM, "[scummvm]vsync", TF),
        spaced("mame", MAME, "waitvsync", ONE),
    ],
    &citra!("[Renderer]use_vsync", TF),
]);

const SHOW_FPS: &[Key] = &[
    ra("fps_show"),
    ini("duckstation", DS, "[Display]ShowFPS", TF),
    ini("pcsx2", PCSX2, "[EmuCore/GS]OsdShowFPS", TF),
    ini("dolphin", GFX, "[Settings]ShowFPS", PY),
    ini(
        "ppsspp",
        PPSSPP,
        "[Graphics]iShowStatusFlags",
        Words::Bool("2", "0"),
    ),
    ini("flycast", FLYCAST, "[config]rend.ShowFPS", YN),
    ini("mgba", MGBA, "[ports.qt]showFps", ONE),
    rpcs3("Video/Performance Overlay/Enabled", TF),
    cemu("Graphic/Overlay/Position", ONE),
    cemu("Graphic/Overlay/FPS", TF),
    spaced("mednafen", MEDNAFEN, "fps.autoenable", ONE),
    ini("dosbox-x", DOSBOX_X, "[sdl]showdetails", TF),
    ini("scummvm", SCUMMVM, "[scummvm]show_fps", TF),
];

const ASPECTS: &[(&str, &str)] = &[
    ("auto", "As the game sets it"),
    ("4:3", "4:3"),
    ("16:9", "16:9"),
    ("stretch", "Stretch to fill"),
];

const ASPECT: &[Key] = &[
    ini(
        "duckstation",
        DS,
        "[Display]AspectRatio",
        Words::Map(&[
            ("auto", "Auto (Game Native)"),
            ("4:3", "4:3"),
            ("16:9", "16:9"),
            ("stretch", "Stretch To Fill"),
        ]),
    ),
    ini(
        "pcsx2",
        PCSX2,
        "[EmuCore/GS]AspectRatio",
        Words::Map(&[
            ("auto", "Auto 4:3/3:2"),
            ("4:3", "4:3"),
            ("16:9", "16:9"),
            ("stretch", "Stretch"),
        ]),
    ),
    ini(
        "dolphin",
        GFX,
        "[Settings]AspectRatio",
        Words::Map(&[("auto", "0"), ("4:3", "2"), ("16:9", "1"), ("stretch", "3")]),
    ),
    toml(
        "xemu",
        XEMU,
        "[display.ui]aspect_ratio",
        Words::Map(&[("auto", "\"auto\""), ("4:3", "\"4x3\""), ("16:9", "\"16x9\"")]),
    ),
    toml(
        "xemu",
        XEMU,
        "[display.ui]fit",
        Words::Map(&[
            ("auto", "\"scale\""),
            ("4:3", "\"scale\""),
            ("16:9", "\"scale\""),
            ("stretch", "\"stretch\""),
        ]),
    ),
    rpcs3(
        "Video/Aspect ratio",
        Words::Map(&[("4:3", "4:3"), ("16:9", "16:9")]),
    ),
    rpcs3(
        "Video/Stretch To Display Area",
        Words::Map(&[("4:3", "false"), ("16:9", "false"), ("stretch", "true")]),
    ),
    cemu(
        "Graphic/FullscreenScaling",
        Words::Map(&[("auto", "0"), ("stretch", "1")]),
    ),
    ini(
        "dosbox-staging",
        STAGING,
        "[render]aspect",
        Words::Map(&[("auto", "auto"), ("4:3", "on"), ("stretch", "stretch")]),
    ),
    ini(
        "dosbox-x",
        DOSBOX_X,
        "[render]aspect",
        Words::Map(&[("4:3", "true"), ("stretch", "false")]),
    ),
    ini(
        "scummvm",
        SCUMMVM,
        "[scummvm]aspect_ratio",
        Words::Map(&[("4:3", "true"), ("stretch", "false")]),
    ),
    ini(
        "scummvm",
        SCUMMVM,
        "[scummvm]stretch_mode",
        Words::Map(&[("auto", "fit"), ("4:3", "fit"), ("stretch", "stretch")]),
    ),
    spaced(
        "mame",
        MAME,
        "keepaspect",
        Words::Map(&[("auto", "1"), ("4:3", "1"), ("stretch", "0")]),
    ),
];

const WIDESCREEN: &[Key] = &[
    ini("duckstation", DS, "[GPU]WidescreenHack", TF),
    ini("pcsx2", PCSX2, "[EmuCore]EnableWideScreenPatches", TF),
    ini("dolphin", GFX, "[Settings]wideScreenHack", PY),
    ini("flycast", FLYCAST, "[config]rend.WideScreen", YN),
];

const LAYOUTS: &[(&str, &str)] = &[
    ("stacked", "One above the other"),
    ("side", "Side by side"),
    ("large", "One large, one small"),
    ("hybrid", "Hybrid"),
    ("single", "One screen"),
];

const SCREEN_LAYOUT: [Key; 4] = concat(&[
    &[toml(
        "melonds",
        MELONDS,
        "[Instance0.Window0]ScreenLayout",
        Words::Map(&[("stacked", "1"), ("side", "2"), ("hybrid", "3")]),
    )],
    &citra!(
        "[Layout]layout_option",
        Words::Map(&[
            ("stacked", "0"),
            ("single", "1"),
            ("large", "2"),
            ("side", "3"),
            ("hybrid", "5")
        ])
    ),
]);

pub(super) const VIDEO: &[Setting] = &[
    choice(
        "renderer",
        "Renderer",
        "How the game is drawn. Vulkan suits most current graphics cards; OpenGL works almost everywhere; software is slowest but most exact.",
        RENDERERS,
        &RENDERER,
    ),
    choice(
        "resolution",
        "Internal resolution",
        "Draw 3D games at a multiple of the console's own resolution: sharper, and harder on the graphics card.",
        SCALES,
        &RESOLUTION,
    ),
    config(
        "vsync",
        "VSync",
        "Wait for the screen's refresh: no tearing, a little more input lag.",
        &VSYNC,
    ),
    config(
        "show_fps",
        "Show frame rate",
        "Show frames per second while playing.",
        SHOW_FPS,
    ),
    choice(
        "aspect",
        "Aspect ratio",
        "The picture's shape on screen.",
        ASPECTS,
        ASPECT,
    ),
    config(
        "widescreen",
        "Widescreen hack",
        "Draw more of the scene to the sides so 4:3 games fill a 16:9 screen (some games show glitches).",
        WIDESCREEN,
    ),
    choice(
        "screen_layout",
        "Screen layout",
        "Where a DS or 3DS game's two screens go.",
        LAYOUTS,
        &SCREEN_LAYOUT,
    ),
];
