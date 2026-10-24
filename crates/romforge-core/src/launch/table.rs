//! The one table of emulators romforge starts (house rule 4): RetroArch and
//! the standalone emulators, how to find each and how to start it.
//!
//! Flags are filled in only once checked, against `--help` on a real
//! install or against the emulator's source; an empty `fullscreen` or
//! `config` means not checked yet, not "has none".

mod known;
pub use known::KnownEmulator;
use known::emu;

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
    .fullscreen(&["--fullscreen"])
    // Its Flatpak asks on every start whether to run an "unofficial build"
    // (`gui_application.cpp`, whenever `FLATPAK_ID` is set; no setting).
    // The official AppImage doesn't: it is preferred, and offered.
    .official_appimage("RPCS3/rpcs3-binaries-linux", "_linux64.AppImage")
    .flatpak_note(
        "RPCS3's Flatpak asks on every start whether to run an unofficial build (its developers don't support Flatpak builds). The official AppImage doesn't ask; romforge uses it instead once it is downloaded.",
    ),
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
    // 2126.2 (citra_qt.cpp): the game is the last argument, `-f` full
    // screen; a CIA is installed (`-i`), never started, so Play refuses
    // one (`launch/start.rs`). Decrypted games only.
    emu(
        "azahar",
        "Azahar",
        &["azahar", "azahar-qt"],
        "org.azahar_emu.Azahar",
        &["{rom}"],
        false,
    )
    .fullscreen(&["-f"]),
    // Azahar's predecessors, discontinued; both play encrypted games with
    // `aes_keys.txt` (Azahar removed that in dc1ebb63, 2025-02-27), and
    // neither reads Z3DS. Lime3DS 2119.1 (lime3ds-archive, lime_qt.cpp):
    // one `lime3ds` program, the game last, `-f`.
    emu(
        "lime3ds",
        "Lime3DS",
        &["lime3ds"],
        "io.github.lime3ds.Lime3DS",
        &["{rom}"],
        false,
    )
    .fullscreen(&["-f"])
    .cannot_open(&["zcci", "zcia", "zcxi", "z3dsx"])
    .discontinued("2119.1 (2024-11-09)"),
    // Citra's last build (2024-03-04; its repository was taken down, read
    // from citra-mirror/citra, main.cpp): `citra-qt -f -g <file>`. Its
    // files are Lime3DS's (RetroBIOS's `citra` is the core's layout).
    emu(
        "citra",
        "Citra",
        &["citra-qt"],
        "org.citra_emu.citra",
        &["-g", "{rom}"],
        false,
    )
    .appimage("citra-qt")
    .fullscreen(&["-f"])
    .cannot_open(&["zcci", "zcia", "zcxi", "z3dsx"])
    .discontinued("its last build (2024-03-04)")
    .firmware("lime3ds"),
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
