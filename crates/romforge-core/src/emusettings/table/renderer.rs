//! The renderer each emulator draws with (sources as in `video.rs`).

use super::files::*;
use super::{Key, Words, concat};

pub(super) const RENDERERS: &[(&str, &str)] = &[
    ("auto", "Automatic"),
    ("vulkan", "Vulkan"),
    ("opengl", "OpenGL"),
    ("software", "Software"),
];

pub(super) const RENDERER: [Key; 21] = concat(&[
    &[
        ini(
            "duckstation",
            DS,
            "[GPU]Renderer",
            Words::Map(&[
                ("auto", "Automatic"),
                ("vulkan", "Vulkan"),
                ("opengl", "OpenGL"),
                ("software", "Software"),
            ]),
        ),
        ini(
            "pcsx2",
            PCSX2,
            "[EmuCore/GS]Renderer",
            Words::Map(&[
                ("auto", "-1"),
                ("vulkan", "14"),
                ("opengl", "12"),
                ("software", "13"),
            ]),
        ),
        ini(
            "dolphin",
            DOLPHIN,
            "[Core]GFXBackend",
            Words::Map(&[
                ("vulkan", "Vulkan"),
                ("opengl", "OGL"),
                ("software", "Software Renderer"),
            ]),
        ),
        ini(
            "ppsspp",
            PPSSPP,
            "[Graphics]GraphicsBackend",
            Words::Map(&[("vulkan", "3 (VULKAN)"), ("opengl", "0 (OPENGL)")]),
        ),
        ini(
            "ppsspp",
            PPSSPP,
            "[Graphics]SoftwareRenderer",
            Words::Map(&[("vulkan", "False"), ("opengl", "False"), ("software", "True")]),
        ),
        ini(
            "flycast",
            FLYCAST,
            "[config]pvr.rend",
            Words::Map(&[("opengl", "0"), ("vulkan", "4")]),
        ),
        toml(
            "melonds",
            MELONDS,
            "[3D]Renderer",
            Words::Map(&[("software", "0"), ("opengl", "1")]),
        ),
        ini(
            "mgba",
            MGBA_QT,
            "[General]displayDriver",
            Words::Map(&[("software", "0"), ("opengl", "1")]),
        ),
        toml(
            "xemu",
            XEMU,
            "[display]renderer",
            Words::Map(&[("opengl", "\"OPENGL\""), ("vulkan", "\"VULKAN\"")]),
        ),
        rpcs3(
            "Video/Renderer",
            Words::Map(&[("vulkan", "Vulkan"), ("opengl", "OpenGL")]),
        ),
        cemu("Graphic/api", Words::Map(&[("opengl", "0"), ("vulkan", "1")])),
        spaced(
            "mednafen",
            MEDNAFEN,
            "video.driver",
            Words::Map(&[("auto", "default"), ("opengl", "opengl"), ("software", "softfb")]),
        ),
        ini(
            "dosbox-staging",
            STAGING,
            "[sdl]output",
            Words::Map(&[("opengl", "opengl"), ("software", "texture")]),
        ),
        ini(
            "dosbox-x",
            DOSBOX_X,
            "[sdl]output",
            Words::Map(&[("auto", "default"), ("opengl", "opengl"), ("software", "surface")]),
        ),
        ini(
            "scummvm",
            SCUMMVM,
            "[scummvm]gfx_mode",
            Words::Map(&[
                ("auto", "default"),
                ("opengl", "opengl"),
                ("software", "surfacesdl"),
            ]),
        ),
        // Its 3D games' renderer (`--renderer`).
        ini(
            "scummvm",
            SCUMMVM,
            "[scummvm]renderer",
            Words::Map(&[("opengl", "opengl_shaders"), ("software", "software")]),
        ),
        spaced(
            "mame",
            MAME,
            "video",
            Words::Map(&[
                ("auto", "auto"),
                ("vulkan", "bgfx"),
                ("opengl", "opengl"),
                ("software", "soft"),
            ]),
        ),
        spaced("mame", MAME, "bgfx_backend", Words::Map(&[("vulkan", "vulkan")])),
    ],
    &citra!(
        "[Renderer]graphics_api",
        Words::Map(&[("software", "0"), ("opengl", "1"), ("vulkan", "2")])
    ),
]);
