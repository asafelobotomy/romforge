//! The one table of systems. Seeded into the `systems` table on every open.
//!
//! Based on remustwo's `0002_systems.sql` (MIT). `dat_names` are the DAT
//! header names (No-Intro / Redump style, without trailing `(...)` groups)
//! used to guess a DAT's system on import. `preferred_hash` is informational
//! only: matching always needs size + SHA1 or size + MD5. `extensions` decide
//! which files a scan hashes. `nointro`, `redump`, `tosec`, `hacks` and
//! `homebrew` say where `dat download` fetches the system's DATs; `cores` and
//! `emulators` what `launch` runs; `wikidata` the platform's Wikidata items;
//! `screenscraper` and `igdb` the ids those art sources use (from memory, to
//! be checked once keys are entered); `launchbox` its platform names in the
//! LaunchBox Games Database (a download the user makes, see `extdb`);
//! `ident` the kinds of game ID (`ident.rs`) its games carry; `libretro`
//! its name on libretro's servers when no DAT name gives it; `dats`
//! libretro-database's own DATs (`dat/<name>.dat`) for it; `folders` that
//! a game is a whole folder (or archive) of files, as DOS and ScummVM
//! games are, not one ROM.

mod table;
pub use table::*;
mod boxes;
pub use boxes::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct System {
    pub id: i64,
    pub key: &'static str,
    pub name: &'static str,
    pub manufacturer: &'static str,
    pub generation: i64,
    pub release_year: i64,
    pub preferred_hash: &'static str,
    pub is_disc_based: bool,
    pub is_handheld: bool,
    pub dat_names: &'static [&'static str],
    /// Lower-case file extensions (without the dot) this system's ROMs use.
    /// The scanner hashes only these, plus `.zip` and `.7z`.
    pub extensions: &'static [&'static str],
    /// Redump DAT codes (`http://redump.org/datfile/<code>/`), games first,
    /// then BIOS.
    pub redump: &'static [&'static str],
    /// No-Intro DATs in libretro-database's mirror
    /// (`metadat/no-intro/<name>.dat`); names there differ from No-Intro's in
    /// places, so they are listed rather than derived from `dat_names`.
    pub nointro: &'static [&'static str],
    /// TOSEC system names: every DAT in the TOSEC pack named
    /// `<name> - ...` is imported. Set only where No-Intro and Redump leave a
    /// gap (home computers), so consoles don't match twice.
    pub tosec: &'static [&'static str],
    /// Optional extras from libretro-database: `metadat/hacks/<name>.dat` and
    /// `metadat/homebrew/<name>.dat`.
    pub hacks: &'static [&'static str],
    pub homebrew: &'static [&'static str],
    /// RetroArch cores to try, best first (`<core>_libretro.so`).
    pub cores: &'static [&'static str],
    /// Standalone emulators to try after RetroArch (names from
    /// `launch::KNOWN_EMULATORS`).
    pub emulators: &'static [&'static str],
    /// Wikidata items of the platform (games are matched against the video
    /// games listed for it).
    pub wikidata: &'static [&'static str],
    /// RetroBIOS's ids for the system (several spellings): a BIOS file its
    /// list ties to one of these, or to none, is listed for it (milestone
    /// 20). Empty for arcade systems, whose BIOS sets MAME's DATs cover.
    pub retrobios: &'static [&'static str],
    /// The system's folders in retrokit-manuals on archive.org (manual
    /// scans named by No-Intro title; milestone 26).
    pub manuals: &'static [&'static str],
    /// ScreenScraper's system id (unverified until checked with a key).
    pub screenscraper: Option<u32>,
    /// IGDB platform ids (unverified until checked with a key).
    pub igdb: &'static [u32],
    /// Platform names in the LaunchBox Games Database (checked against its
    /// Platforms.xml), for matching games by title there.
    pub launchbox: &'static [&'static str],
    /// Kinds of game ID its games' headers or boot files give
    /// ([`crate::ident::GameId::kind`]).
    pub ident: &'static [&'static str],
    /// libretro's name for the system (thumbnails, metadata), where its
    /// No-Intro or DAT names don't give it (Redump-only systems).
    pub libretro: &'static str,
    /// The file name (without `.png`) of its picture in RetroArch's icon
    /// themes, where that isn't its libretro name (no thumbnails or
    /// metadata there, or spelt otherwise).
    pub icon: &'static str,
    /// The Commons file shown as its logo, where its Wikidata items have
    /// none or several (then the one that reads like its name here).
    pub logo: &'static str,
    /// libretro-database's own DATs (`dat/<name>.dat`, without `.dat`),
    /// imported for matching; enrich also reads their metadata.
    pub dats: &'static [&'static str],
    /// libretro-database files (`dat/<name>.dat`) read only for names and
    /// metadata, never imported as DATs (ScummVM's: its hashes are of
    /// `.scummvm` launcher files).
    pub titles: &'static [&'static str],
    /// Whether a game is a folder of files (or the archive holding it),
    /// and how import keeps it.
    pub folders: Folders,
    /// A floppy computer whose cores open a set's disks themselves, as
    /// plain files: import keeps a multi-disk set unzipped in a folder of
    /// its own, with this playlist (`m3u`, `cmd`; empty: none).
    pub loose_sets: Option<&'static str>,
    /// Arcade: games are sets of ROMs in a zip named after the set (every
    /// member hashed), kept as they are; one system per emulator family,
    /// since a set runs only in the version its DAT belongs to.
    pub arcade: bool,
    /// DATs fetched from a fixed URL (`dat download`).
    pub urls: &'static [&'static str],
}

/// How a system's games are stored (`System::folders`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folders {
    /// One ROM, disc or disk set per game.
    No,
    /// A folder of files, imported as one zip (DOSBox Pure runs it).
    Zip,
    /// A folder of files, imported as a folder (ScummVM reads no archives).
    Kept,
}

#[allow(clippy::too_many_arguments)]
const fn s(
    id: i64,
    key: &'static str,
    name: &'static str,
    manufacturer: &'static str,
    generation: i64,
    release_year: i64,
    preferred_hash: &'static str,
    disc: u8,
    handheld: u8,
    dat_names: &'static [&'static str],
    extensions: &'static [&'static str],
) -> System {
    System {
        id,
        key,
        name,
        manufacturer,
        generation,
        release_year,
        preferred_hash,
        is_disc_based: disc == 1,
        is_handheld: handheld == 1,
        dat_names,
        extensions,
        redump: &[],
        nointro: &[],
        tosec: &[],
        hacks: &[],
        homebrew: &[],
        cores: &[],
        emulators: &[],
        wikidata: &[],
        retrobios: &[],
        manuals: &[],
        screenscraper: None,
        igdb: &[],
        launchbox: &[],
        ident: &[],
        libretro: "",
        icon: "",
        logo: "",
        dats: &[],
        titles: &[],
        folders: Folders::No,
        loose_sets: None,
        arcade: false,
        urls: &[],
    }
}

impl System {
    /// A colour for the platform's splash and badges (stage 2, step 6): a
    /// plain colour by maker, no logo or trademark; themes may override it.
    pub fn accent(&self) -> &'static str {
        match self.manufacturer {
            "Nintendo" => "#d1293d",
            "Sega" => "#1f5fbf",
            "Sony" => "#3b4a6b",
            "Microsoft" => "#2f8f3a",
            "Atari" => "#c8501e",
            "NEC" => "#e06c1f",
            "SNK" => "#b38b1d",
            "Commodore" => "#5a6fa8",
            "Bandai" => "#6a3fa0",
            "ScummVM" => "#3e8f5b",
            _ => "#5c5c6a",
        }
    }

    const fn redump(mut self, codes: &'static [&'static str]) -> Self {
        self.redump = codes;
        self
    }

    const fn nointro(mut self, names: &'static [&'static str]) -> Self {
        self.nointro = names;
        self
    }

    const fn tosec(mut self, names: &'static [&'static str]) -> Self {
        self.tosec = names;
        self
    }

    const fn hacks(mut self, names: &'static [&'static str]) -> Self {
        self.hacks = names;
        self
    }

    const fn homebrew(mut self, names: &'static [&'static str]) -> Self {
        self.homebrew = names;
        self
    }

    const fn cores(mut self, names: &'static [&'static str]) -> Self {
        self.cores = names;
        self
    }

    const fn emulators(mut self, names: &'static [&'static str]) -> Self {
        self.emulators = names;
        self
    }

    const fn wikidata(mut self, items: &'static [&'static str]) -> Self {
        self.wikidata = items;
        self
    }

    const fn retrobios(mut self, ids: &'static [&'static str]) -> Self {
        self.retrobios = ids;
        self
    }

    const fn manuals(mut self, folders: &'static [&'static str]) -> Self {
        self.manuals = folders;
        self
    }

    const fn ss(mut self, id: u32) -> Self {
        self.screenscraper = Some(id);
        self
    }

    const fn igdb(mut self, ids: &'static [u32]) -> Self {
        self.igdb = ids;
        self
    }

    const fn launchbox(mut self, names: &'static [&'static str]) -> Self {
        self.launchbox = names;
        self
    }

    const fn ident(mut self, kinds: &'static [&'static str]) -> Self {
        self.ident = kinds;
        self
    }

    const fn libretro(mut self, name: &'static str) -> Self {
        self.libretro = name;
        self
    }

    const fn icon(mut self, name: &'static str) -> Self {
        self.icon = name;
        self
    }

    const fn logo(mut self, file: &'static str) -> Self {
        self.logo = file;
        self
    }

    const fn dats(mut self, names: &'static [&'static str]) -> Self {
        self.dats = names;
        self
    }

    const fn titles(mut self, names: &'static [&'static str]) -> Self {
        self.titles = names;
        self
    }

    const fn arcade(mut self) -> Self {
        self.arcade = true;
        self
    }

    const fn urls(mut self, urls: &'static [&'static str]) -> Self {
        self.urls = urls;
        self
    }

    const fn loose_sets(mut self, playlist: &'static str) -> Self {
        self.loose_sets = Some(playlist);
        self
    }

    const fn folders(mut self, how: Folders) -> Self {
        self.folders = how;
        self
    }
}

/// Find a system by its key (case-insensitive), e.g. `NES` or `game boy`.
pub fn by_key(key: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.key.eq_ignore_ascii_case(key.trim()))
}

/// Whether any system uses this extension (case-insensitive, no dot).
pub fn is_rom_extension(ext: &str) -> bool {
    SYSTEMS
        .iter()
        .any(|s| s.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// The one system using this extension (case-insensitive, no dot); `None`
/// when none or several do (`.bin`). A hint about an unknown file, never
/// a match (house rule 8).
pub fn by_extension(ext: &str) -> Option<&'static System> {
    let mut it = SYSTEMS
        .iter()
        .filter(|s| s.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)));
    let one = it.next()?;
    it.next().is_none().then_some(one)
}

pub fn by_id(id: i64) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.id == id)
}

/// Systems whose games carry a game ID of `kind` ("snes", "ps1", ...).
pub fn by_ident(kind: &str) -> impl Iterator<Item = &'static System> {
    SYSTEMS.iter().filter(move |s| s.ident.contains(&kind))
}

/// Guess a system from a DAT header name such as
/// `Nintendo - Nintendo Entertainment System (Headered) (Parent-Clone)`.
/// Trailing `(...)` groups are ignored and the rest must match exactly, so
/// `Nintendo - Game Boy Color` never matches `Nintendo - Game Boy`.
pub fn from_dat_name(dat_name: &str) -> Option<&'static System> {
    let base = strip_paren_suffixes(dat_name);
    SYSTEMS
        .iter()
        .find(|s| s.dat_names.iter().any(|n| n.eq_ignore_ascii_case(base)))
}

fn strip_paren_suffixes(mut s: &str) -> &str {
    loop {
        s = s.trim_end();
        if s.ends_with(')')
            && let Some(open) = s.rfind(" (")
        {
            s = &s[..open];
            continue;
        }
        return s;
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod box_tests;
