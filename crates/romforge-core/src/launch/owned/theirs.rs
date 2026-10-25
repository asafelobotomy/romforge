//! Where your own game data is, for the emulators that keep it outside
//! romforge's config (RPCS3's devices, Cemu's MLC, xemu's disk and EEPROM,
//! the 3DS emulators' NAND and SD card), read as `gamedata.rs` and
//! `azahar.rs` always have: their config points at it, and "move it in"
//! moves it from there. Once moved in, a `.romforge-moved-in` file in the
//! emulator's romforge config folder says to use RomForge's copy instead.

use std::path::{Path, PathBuf};

use super::existing::UserDirs;

/// The file that says an emulator's data was moved into RomForge.
pub const MOVED_IN: &str = ".romforge-moved-in";

/// Whether `dir` (an emulator's romforge config folder) has had its data
/// moved in.
pub fn moved_in(dir: &Path) -> bool {
    dir.join(MOVED_IN).is_file()
}

/// The first of `candidates` that exists.
pub(super) fn first(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|p| p.exists())
}

/// `rel` in the user's XDG folder and in the Flatpak `id`'s.
pub(super) fn usual(user: &UserDirs, id: &str, data: bool, rel: &str) -> [PathBuf; 2] {
    let (base, sub) = if data {
        (&user.xdg.data, "data")
    } else {
        (&user.xdg.config, "config")
    };
    [
        base.join(rel),
        user.home.join(".var/app").join(id).join(sub).join(rel),
    ]
}

/// `key<sep> value` on a line, quotes taken off.
pub(super) fn value_of(line: &str, key: &str, sep: char) -> Option<String> {
    let (k, v) = line.split_once(sep)?;
    let unquote = |s: &str| s.trim().trim_matches(|c| c == '"' || c == '\'').to_owned();
    (unquote(k) == key).then(|| unquote(v))
}

/// RPCS3's devices romforge points it at.
pub const RPCS3_DEVICES: &[&str] = &[
    "dev_hdd0",
    "dev_hdd1",
    "dev_flash",
    "dev_flash2",
    "dev_flash3",
    "games",
];

/// Your RPCS3's devices that exist: (device, folder), from its `vfs.yml`
/// (`$(EmulatorDir)` and each device's own entry), else beside its config.
pub fn rpcs3_devices(user: &UserDirs) -> Vec<(&'static str, PathBuf)> {
    let Some(theirs) = first(usual(user, "net.rpcs3.RPCS3", false, "rpcs3")) else {
        return Vec::new();
    };
    let vfs = std::fs::read_to_string(theirs.join("vfs.yml")).unwrap_or_default();
    let custom = |key: &str| {
        vfs.lines()
            .find_map(|l| value_of(l, key, ':'))
            .filter(|v| !v.is_empty())
    };
    let emu_dir = custom("$(EmulatorDir)").unwrap_or_else(|| format!("{}/", theirs.display()));
    RPCS3_DEVICES
        .iter()
        .filter_map(|dev| {
            let path = custom(&format!("/{dev}/"))
                .map(|v| v.replace("$(EmulatorDir)", &emu_dir))
                .unwrap_or_else(|| format!("{emu_dir}{dev}/"));
            let path = PathBuf::from(path.trim_end_matches('/'));
            path.exists().then_some((*dev, path))
        })
        .collect()
}

/// Your Cemu's MLC: where its own `settings.xml` says, else its usual one.
pub fn cemu_mlc(user: &UserDirs) -> Option<PathBuf> {
    let own = first(usual(user, "info.cemu.Cemu", false, super::files::CEMU))
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|x| {
            Some(
                x.split_once("<mlc_path>")?
                    .1
                    .split_once("</mlc_path>")?
                    .0
                    .trim()
                    .to_owned(),
            )
        })
        .filter(|m| !m.is_empty() && Path::new(m).exists())
        .map(PathBuf::from);
    own.or_else(|| first(usual(user, "info.cemu.Cemu", true, "Cemu/mlc01")))
}

/// Your xemu's `key` file (`hdd_path`, `eeprom_path`) from its `xemu.toml`,
/// when it exists.
pub fn xemu_file(user: &UserDirs, key: &str) -> Option<PathBuf> {
    let toml = first(usual(user, "app.xemu.xemu", true, "xemu/xemu/xemu.toml"))
        .and_then(|f| std::fs::read_to_string(f).ok())?;
    toml.lines()
        .find_map(|l| value_of(l, key, '='))
        .filter(|v| !v.is_empty() && Path::new(v).exists())
        .map(PathBuf::from)
}

/// Your NAND and SD card for one of Citra's line: the folder holding
/// `nand/` of the first of `reads` (its own and older lines' folders) in
/// XDG or any of the line's Flatpaks.
pub fn citra_storage(user: &UserDirs, reads: &[&str]) -> Option<PathBuf> {
    const FLATPAKS: &[&str] = &[
        "org.azahar_emu.Azahar",
        "io.github.lime3ds.Lime3DS",
        "org.citra_emu.citra",
    ];
    reads.iter().find_map(|d| {
        FLATPAKS
            .iter()
            .find_map(|id| first(usual(user, id, true, d)).filter(|f| f.join("nand").is_dir()))
    })
}
