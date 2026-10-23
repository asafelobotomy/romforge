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
    /// As its makers write it, for the app.
    pub label: &'static str,
    /// Another entry whose settings it uses (MAME's machines are MAME).
    pub shares: Option<&'static str>,
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
    /// How to run it from a config folder of romforge's (`{config}`, and
    /// `{system}` the game's system; milestone 18, 1.3). Used only for an
    /// emulator romforge writes a config for (`launch/owned.rs`).
    pub config: &'static [&'static str],
    /// Where it finds BIOS files, when the RomForge folder is there:
    /// `{bios}` is `Bios/<settings name>/` (milestone 20). For emulators
    /// that take the folder on the command line rather than in a config
    /// file of romforge's (`launch/owned/standalone.rs`).
    pub bios: &'static [&'static str],
    /// For MAME's machines: the machine it starts, whose ROM set (`<machine>.zip`)
    /// is the BIOS it needs.
    pub machine: Option<&'static str>,
    /// Opens a ROM inside a zip/7z itself.
    pub archives: bool,
}

const fn emu(
    name: &'static str,
    label: &'static str,
    programs: &'static [&'static str],
    flatpak: &'static str,
    args: &'static [&'static str],
    archives: bool,
) -> KnownEmulator {
    KnownEmulator {
        name,
        label,
        shares: None,
        programs,
        flatpak: Some(flatpak),
        appimage: None,
        args,
        fullscreen: &[],
        config: &[],
        bios: &[],
        machine: None,
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
    const fn bios(mut self, flags: &'static [&'static str]) -> Self {
        self.bios = flags;
        self
    }
    const fn machine(mut self, name: &'static str) -> Self {
        self.machine = Some(name);
        self
    }
    const fn appimage(mut self, prefix: &'static str) -> Self {
        self.appimage = Some(prefix);
        self
    }
    const fn no_flatpak(mut self) -> Self {
        self.flatpak = None;
        self
    }
    const fn shares(mut self, name: &'static str) -> Self {
        self.shares = Some(name);
        self
    }
    /// The entry whose settings apply to it: [`KnownEmulator::shares`],
    /// else itself.
    pub fn settings_name(&self) -> &'static str {
        self.shares.unwrap_or(self.name)
    }
}

pub const RETROARCH_FLATPAK: &str = "org.libretro.RetroArch";

// Flatpak ids are checked on Flathub (flathub.org/api/v2/appstream/<id>,
// 2026-10-19); romforge offers to install them from there (1.4).

/// RetroArch; each system's cores come from its `cores` column.
/// Checked with `retroarch --help` (1.22.2).
pub const RETROARCH: KnownEmulator = emu(
    "retroarch",
    "RetroArch",
    &["retroarch"],
    RETROARCH_FLATPAK,
    &["-L", "{core}", "{rom}"],
    true,
)
.fullscreen(&["-f"])
.config(&[
    "--config",
    "{config}/retroarch.cfg",
    "--appendconfig",
    "{config}/systems/{system}.cfg",
]);

// The console emulators' flags were read from their source at the
// release named (2026-10-19). One with no `config` has no flag for a data
// folder: a portable marker next to the program or an environment
// variable instead (milestone 18, 1.3).
pub const KNOWN_EMULATORS: &[KnownEmulator] = &[
    // Rolling release of 2026-10-13 (qthost.cpp). `-nogui` quits when the
    // game ends; no data-folder flag (`portable.txt`).
    emu(
        "duckstation",
        "DuckStation",
        &["duckstation-qt", "duckstation"],
        "org.duckstation.DuckStation",
        &["--", "{rom}"],
        false,
    )
    .fullscreen(&["-fullscreen", "-nogui"]),
    // v2.8.2 (QtHost.cpp). `-datapath` keeps its data in `PCSX2/` inside
    // the folder given.
    emu(
        "pcsx2",
        "PCSX2",
        &["pcsx2-qt", "pcsx2"],
        "net.pcsx2.PCSX2",
        &["--", "{rom}"],
        false,
    )
    .fullscreen(&["-fullscreen", "-nogui"])
    .config(&["-datapath", "{config}"]),
    // 2609a (CommandLineParse.cpp): no fullscreen flag, a config override
    // instead. `-u` is ignored under Flatpak, which always uses XDG.
    emu(
        "dolphin",
        "Dolphin",
        &["dolphin-emu"],
        "org.DolphinEmu.dolphin-emu",
        &["-b", "-e", "{rom}"],
        false,
    )
    .fullscreen(&["-C", "Dolphin.Display.Fullscreen=True"])
    .config(&["-u", "{config}"]),
    // v1.20.4 (NativeApp.cpp). It reads an ISO inside a zip but not a 7z,
    // so archives stay off.
    emu(
        "ppsspp",
        "PPSSPP",
        &["PPSSPPSDL", "ppsspp"],
        "org.ppsspp.PPSSPP",
        &["{rom}"],
        false,
    )
    .fullscreen(&["--fullscreen", "--escape-exit"]),
    // v2.7 (cfg/cl.cpp).
    emu(
        "flycast",
        "Flycast",
        &["flycast"],
        "org.flycast.Flycast",
        &["{rom}"],
        false,
    )
    .fullscreen(&["-config", "window:fullscreen=yes"]),
    // 0.10.5 (commandline.c).
    emu(
        "mgba",
        "mGBA",
        &["mgba-qt", "mgba"],
        "io.mgba.mGBA",
        &["{rom}"],
        true,
    )
    .fullscreen(&["-f"]),
    // 1.1 (CLI.cpp); archives need a build with archive support.
    emu(
        "melonds",
        "melonDS",
        &["melonDS"],
        "net.kuribo64.melonDS",
        &["{rom}"],
        true,
    )
    .fullscreen(&["-f"]),
    // v0.8.137 (vl.c, xemu.c).
    emu(
        "xemu",
        "xemu",
        &["xemu"],
        "app.xemu.xemu",
        &["-dvd_path", "{rom}"],
        false,
    )
    .fullscreen(&["-full-screen"])
    .config(&["-config_path", "{config}/xemu.toml"]),
    // v0.0.43 (rpcs3.cpp): `--fullscreen` works only with `--no-gui`.
    emu(
        "rpcs3",
        "RPCS3",
        &["rpcs3"],
        "net.rpcs3.RPCS3",
        &["--no-gui", "{rom}"],
        false,
    )
    .fullscreen(&["--fullscreen"]),
    // 1.32.1: any setting is a flag; its folder is `MEDNAFEN_HOME`. Not on
    // Flathub (checked 2026-10-19).
    emu("mednafen", "Mednafen", &["mednafen"], "", &["{rom}"], true)
        .no_flatpak()
        .fullscreen(&["-video.fs", "1"]),
    // v2.6 (LaunchSettings.cpp); `-m` moves only the MLC folder.
    emu(
        "cemu",
        "Cemu",
        &["cemu", "Cemu"],
        "info.cemu.Cemu",
        &["-g", "{rom}"],
        false,
    )
    .fullscreen(&["-f"]),
    // A folder is mounted as C:, a program run from its folder; images
    // and archives need the core. Checked with `--help` (0.83).
    emu(
        "dosbox-staging",
        "DOSBox Staging",
        &["dosbox-staging", "dosbox"],
        "io.github.dosbox-staging",
        &["{rom}"],
        false,
    )
    .fullscreen(&["--fullscreen"])
    .config(&["--noprimaryconf", "--conf", "{config}/dosbox-staging.conf"])
    // 0.83's `[mt32] romdir`, `[soundcanvas] soundcanvas_rom_dir` and
    // `[fluidsynth] soundfont_dir`, by `--set`.
    .bios(&[
        "--set",
        "romdir={bios}/mt32-roms",
        "--set",
        "soundcanvas_rom_dir={bios}/soundcanvas-roms",
        "--set",
        "soundfont_dir={bios}/soundfonts",
    ]),
    // Checked with `-help` (2026.10.01).
    emu(
        "dosbox-x",
        "DOSBox-X",
        &["dosbox-x"],
        "com.dosbox_x.DOSBox-X",
        &["{rom}"],
        false,
    )
    .fullscreen(&["-fullscreen"])
    .config(&["-conf", "{config}/dosbox-x.conf"])
    // `[midi] mt32.romdir` (dosbox-x.reference.conf), by `-set`.
    .bios(&["-set", "midi mt32.romdir={bios}"]),
    // ScummVM finds the game in the folder itself. Checked with `--help`
    // (2026.3.0).
    emu(
        "scummvm",
        "ScummVM",
        &["scummvm"],
        "org.scummvm.ScummVM",
        &["--path={rom}", "--auto-detect"],
        false,
    )
    .fullscreen(&["-f"])
    .config(&["--config={config}/scummvm.ini"])
    // Engine data (`--extrapath`), laid out as RetroBIOS gives it.
    .bios(&["--extrapath={bios}/scummvm/extra"]),
    // Arcade sets by name, from the folder holding them (and their parents
    // and BIOS sets). Fullscreen is MAME's default; `-nowindow` overrides
    // an ini that says otherwise. Checked with `-showconfig` (0.289).
    emu(
        "mame",
        "MAME",
        &["mame"],
        "org.mamedev.MAME",
        &["-rompath", "{dir}", "{set}"],
        true,
    )
    .fullscreen(&["-nowindow"])
    .config(&["-inipath", "{config}"]),
    // MAME needs the machine's own ROM set in its rompath: `Bios/mame/`.
    emu(
        "mame-fm7",
        "MAME: FM-7",
        &["mame"],
        "org.mamedev.MAME",
        &["fm7", "-flop1", "{rom}"],
        false,
    )
    .appimage("mame")
    .shares("mame")
    .machine("fm7")
    .fullscreen(&["-nowindow"])
    .config(&["-inipath", "{config}"])
    .bios(&["-rompath", "{bios}"]),
    emu(
        "mame-fmtowns",
        "MAME: FM Towns",
        &["mame"],
        "org.mamedev.MAME",
        &["fmtowns", "-cdrom", "{rom}"],
        false,
    )
    .appimage("mame")
    .shares("mame")
    .machine("fmtowns")
    .fullscreen(&["-nowindow"])
    .config(&["-inipath", "{config}"])
    .bios(&["-rompath", "{bios}"]),
];

/// The entry named `name` (`retroarch` included).
pub fn known(name: &str) -> Option<&'static KnownEmulator> {
    std::iter::once(&RETROARCH)
        .chain(KNOWN_EMULATORS)
        .find(|e| e.name.eq_ignore_ascii_case(name))
}
