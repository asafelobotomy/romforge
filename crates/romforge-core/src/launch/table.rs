//! The one table of emulators romforge starts (house rule 4): RetroArch and
//! the standalone emulators, how to find each and how to start it.
//!
//! Flags are filled in only once checked, against `--help` on a real
//! install or against the emulator's source; an empty `fullscreen` or
//! `config` means not checked yet, not "has none".

/// An emulator romforge knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownEmulator {
    pub name: &'static str,
    /// Executable names to look for on `PATH` and in emulator folders.
    pub programs: &'static [&'static str],
    pub flatpak: Option<&'static str>,
    /// How its AppImage's file name starts (any case), when it isn't
    /// `name`: `<appimage>*.AppImage`.
    pub appimage: Option<&'static str>,
    /// Arguments for a game: `{rom}` the file, `{dir}` its folder, `{set}`
    /// an arcade set's name, `{core}` a RetroArch core.
    pub args: &'static [&'static str],
    /// Put before `args` on every start: fullscreen, without the
    /// emulator's own UI.
    pub fullscreen: &'static [&'static str],
    /// How to run it from a config folder of romforge's (`{config}`,
    /// milestone 18, 1.3).
    pub config: &'static [&'static str],
    /// Opens a ROM inside a zip/7z itself.
    pub archives: bool,
}

const fn emu(
    name: &'static str,
    programs: &'static [&'static str],
    flatpak: &'static str,
    args: &'static [&'static str],
    archives: bool,
) -> KnownEmulator {
    KnownEmulator {
        name,
        programs,
        flatpak: Some(flatpak),
        appimage: None,
        args,
        fullscreen: &[],
        config: &[],
        archives,
    }
}

impl KnownEmulator {
    const fn fullscreen(mut self, flags: &'static [&'static str]) -> Self {
        self.fullscreen = flags;
        self
    }
    const fn config(mut self, flags: &'static [&'static str]) -> Self {
        self.config = flags;
        self
    }
    const fn appimage(mut self, prefix: &'static str) -> Self {
        self.appimage = Some(prefix);
        self
    }
}

pub const RETROARCH_FLATPAK: &str = "org.libretro.RetroArch";

/// RetroArch; each system's cores come from its `cores` column.
/// Checked with `retroarch --help` (1.22.2).
pub const RETROARCH: KnownEmulator = emu(
    "retroarch",
    &["retroarch"],
    RETROARCH_FLATPAK,
    &["-L", "{core}", "{rom}"],
    true,
)
.fullscreen(&["-f"])
.config(&["--config", "{config}/retroarch.cfg"]);

pub const KNOWN_EMULATORS: &[KnownEmulator] = &[
    emu(
        "duckstation",
        &["duckstation-qt", "duckstation"],
        "org.duckstation.DuckStation",
        &["{rom}"],
        false,
    ),
    emu(
        "pcsx2",
        &["pcsx2-qt", "pcsx2"],
        "net.pcsx2.PCSX2",
        &["{rom}"],
        false,
    ),
    emu(
        "dolphin",
        &["dolphin-emu"],
        "org.DolphinEmu.dolphin-emu",
        &["-b", "-e", "{rom}"],
        false,
    ),
    emu(
        "ppsspp",
        &["PPSSPPSDL", "ppsspp"],
        "org.ppsspp.PPSSPP",
        &["{rom}"],
        false,
    ),
    emu("flycast", &["flycast"], "org.flycast.Flycast", &["{rom}"], false),
    emu("mgba", &["mgba-qt", "mgba"], "io.mgba.mGBA", &["{rom}"], true),
    emu("melonds", &["melonDS"], "net.kuribo64.melonDS", &["{rom}"], true),
    emu("xemu", &["xemu"], "app.xemu.xemu", &["-dvd_path", "{rom}"], false),
    emu(
        "rpcs3",
        &["rpcs3"],
        "net.rpcs3.RPCS3",
        &["--no-gui", "{rom}"],
        false,
    ),
    emu("mednafen", &["mednafen"], "com.github.mednafen", &["{rom}"], true),
    emu(
        "cemu",
        &["cemu", "Cemu"],
        "info.cemu.Cemu",
        &["-g", "{rom}"],
        false,
    ),
    // A folder is mounted as C:, a program run from its folder; images
    // and archives need the core. Checked with `--help` (0.83).
    emu(
        "dosbox-staging",
        &["dosbox-staging", "dosbox"],
        "io.github.dosbox-staging",
        &["{rom}"],
        false,
    )
    .fullscreen(&["--fullscreen"])
    .config(&["--noprimaryconf", "--conf", "{config}/dosbox-staging.conf"]),
    // Checked with `-help` (2026.10.01).
    emu(
        "dosbox-x",
        &["dosbox-x"],
        "com.dosbox_x.DOSBox-X",
        &["{rom}"],
        false,
    )
    .fullscreen(&["-fullscreen"])
    .config(&["-conf", "{config}/dosbox-x.conf"]),
    // ScummVM finds the game in the folder itself. Checked with `--help`
    // (2026.3.0).
    emu(
        "scummvm",
        &["scummvm"],
        "org.scummvm.ScummVM",
        &["--path={rom}", "--auto-detect"],
        false,
    )
    .fullscreen(&["-f"])
    .config(&["--config={config}/scummvm.ini"]),
    // Arcade sets by name, from the folder holding them (and their parents
    // and BIOS sets). Fullscreen is MAME's default; `-nowindow` overrides
    // an ini that says otherwise. Checked with `-showconfig` (0.289).
    emu(
        "mame",
        &["mame"],
        "org.mamedev.MAME",
        &["-rompath", "{dir}", "{set}"],
        true,
    )
    .fullscreen(&["-nowindow"])
    .config(&["-inipath", "{config}"]),
    // MAME needs the machine's own ROMs in its rompath.
    emu(
        "mame-fm7",
        &["mame"],
        "org.mamedev.MAME",
        &["fm7", "-flop1", "{rom}"],
        false,
    )
    .appimage("mame")
    .fullscreen(&["-nowindow"])
    .config(&["-inipath", "{config}"]),
    emu(
        "mame-fmtowns",
        &["mame"],
        "org.mamedev.MAME",
        &["fmtowns", "-cdrom", "{rom}"],
        false,
    )
    .appimage("mame")
    .fullscreen(&["-nowindow"])
    .config(&["-inipath", "{config}"]),
];

/// The entry named `name` (`retroarch` included).
pub fn known(name: &str) -> Option<&'static KnownEmulator> {
    std::iter::once(&RETROARCH)
        .chain(KNOWN_EMULATORS)
        .find(|e| e.name.eq_ignore_ascii_case(name))
}
