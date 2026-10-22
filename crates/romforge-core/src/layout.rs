//! Every name romforge gives the files and folders it writes, in one place:
//! what one module writes is what another skips (house rule 6: the scan
//! never takes romforge's own files for ROMs).

use std::ffi::OsStr;
use std::path::PathBuf;

/// romforge's folder under each XDG base (config, data, cache).
pub const APP_DIR: &str = "romforge";
/// The library's database.
pub const DB_FILE: &str = "romforge.db";
/// In romforge's config folder: where the RomForge folder is.
pub const POINTER_FILE: &str = "location";

/// The RomForge folder (milestone 13): always named this, in whatever
/// parent is chosen; the marker file at its top says it is one.
pub const HOME_DIR: &str = "RomForge";
pub const HOME_MARKER: &str = ".romforge";
pub const GAMES_DIR: &str = "Games";
pub const MEDIA_DIR: &str = "Media";
pub const DATABASES_DIR: &str = "Databases";
/// Emulators and cores you fetched or put there (milestone 18).
pub const EMULATORS_DIR: &str = "Emulators";
/// In `Emulators/`: romforge's own copy of each emulator's config, one
/// folder per emulator (milestone 18, 1.3).
pub const EMULATOR_CONFIG_DIR: &str = "config";
/// BIOS files, laid out per emulator (`Bios/RetroArch/` as RetroArch's
/// system folder; milestone 20).
pub const BIOS_DIR: &str = "Bios";
/// What games save, one folder per system: SRAM and memory cards, save
/// states, screenshots.
pub const SAVES_DIR: &str = "Saves";
pub const STATES_DIR: &str = "States";
pub const SCREENSHOTS_DIR: &str = "Screenshots";
/// Fullscreen themes the user adds, one folder each (stage 2, step 4).
pub const THEMES_DIR: &str = "Themes";

/// Without a RomForge folder: downloaded databases next to the library
/// database, and art in romforge's cache folder.
pub const LOOSE_DATABASES_DIR: &str = "databases";
pub const LOOSE_ART_DIR: &str = "art";
pub const LOOSE_THEMES_DIR: &str = "themes";

/// Scratch space for imports and cleanup (7z tracks are unpacked here).
pub const STAGING_DIR: &str = ".romforge-staging";
/// Originals kept for undo.
pub const TRASH_DIR: &str = ".romforge-trash";

/// Art and details that go with a game: a folder inside its archive (or
/// beside its files), and its details as Markdown.
pub const EXTRAS_DIR: &str = "romforge";
pub const DETAILS_SUFFIX: &str = ".romforge.md";

/// ScummVM's detection list and libretro's ScummVM titles, kept with the
/// downloaded databases (milestone 14).
pub const SCUMMVM_LIST: &str = "scummvm-detection.dat";
pub const SCUMMVM_TITLES: &str = "scummvm-titles.dat";

/// A file being written beside its final place, renamed when complete.
pub fn part(name: &str) -> String {
    format!(".{name}.romforge-part")
}

/// A sidecar folder being written, renamed when complete.
pub fn side(name: &str) -> String {
    format!(".{name}.romforge-side")
}

/// Folders romforge keeps for itself (staging, trash): never scanned or
/// searched for games.
pub fn is_own_dir(name: &OsStr) -> bool {
    name == TRASH_DIR || name == STAGING_DIR
}

/// A path (or archive member) that is a game's art or details, not ROM
/// data.
pub fn is_extra(name: &str) -> bool {
    name.ends_with(DETAILS_SUFFIX)
        || name
            .split('/')
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case(EXTRAS_DIR) && name.contains('/'))
}

/// romforge's folder under an XDG base: `$<var>/romforge`, else
/// `$HOME/<fallback>/romforge` (`var` must be absolute to count).
pub(crate) fn xdg(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
        .map(|base| base.join(APP_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extras_and_own_folders() {
        assert!(is_extra("romforge/boxart.png"));
        assert!(is_extra("Game (USA).romforge.md"));
        assert!(is_extra("Sets/Game.romforge.md"));
        assert!(!is_extra("romforge"), "a file named romforge is a file");
        assert!(!is_extra("Game.md"), "a Mega Drive ROM");
        assert!(!is_extra("data/romforge/x"), "only at the top");
        assert!(is_own_dir(OsStr::new(TRASH_DIR)) && is_own_dir(OsStr::new(STAGING_DIR)));
        assert_eq!(part("a.zip"), ".a.zip.romforge-part");
    }
}
