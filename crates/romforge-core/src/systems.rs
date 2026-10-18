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

/// How a system's games are stored ([`System::folders`]).
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
        screenscraper: None,
        igdb: &[],
        launchbox: &[],
        ident: &[],
        libretro: "",
        dats: &[],
        titles: &[],
        folders: Folders::No,
        loose_sets: None,
        arcade: false,
        urls: &[],
    }
}

impl System {
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

#[rustfmt::skip]
pub const SYSTEMS: &[System] = &[
    s(1, "NES", "Nintendo Entertainment System", "Nintendo", 3, 1983, "CRC32", 0, 0, &["Nintendo - Nintendo Entertainment System"], &["nes", "unf", "unif"]).nointro(&["Nintendo - Nintendo Entertainment System"]).hacks(&["Nintendo - Nintendo Entertainment System"]).homebrew(&["Nintendo - Nintendo Entertainment System"]).cores(&["mesen", "fceumm", "nestopia"]).wikidata(&["Q172742"]).ss(3).igdb(&[18]).launchbox(&["Nintendo Entertainment System"]),
    s(2, "SNES", "Super Nintendo Entertainment System", "Nintendo", 4, 1990, "CRC32", 0, 0, &["Nintendo - Super Nintendo Entertainment System"], &["sfc", "smc", "swc", "fig", "bs", "st"]).nointro(&["Nintendo - Super Nintendo Entertainment System", "Nintendo - Satellaview", "Nintendo - Sufami Turbo"]).hacks(&["Nintendo - Super Nintendo Entertainment System"]).homebrew(&["Nintendo - Super Nintendo Entertainment System"]).cores(&["snes9x", "bsnes"]).wikidata(&["Q183259"]).ss(4).igdb(&[19]).launchbox(&["Super Nintendo Entertainment System"]).ident(&["snes"]),
    s(3, "N64", "Nintendo 64", "Nintendo", 5, 1996, "CRC32", 0, 0, &["Nintendo - Nintendo 64"], &["n64", "z64", "v64"]).nointro(&["Nintendo - Nintendo 64", "Nintendo - Nintendo 64DD"]).hacks(&["Nintendo - Nintendo 64"]).cores(&["mupen64plus_next", "parallel_n64"]).wikidata(&["Q184839"]).ss(14).igdb(&[4]).launchbox(&["Nintendo 64"]).ident(&["n64"]),
    s(4, "GameCube", "Nintendo GameCube", "Nintendo", 6, 2001, "MD5", 1, 0, &[], &["iso", "gcm", "gcz", "rvz", "ciso"]).redump(&["gc", "gc-bios"]).homebrew(&["Nintendo - GameCube"]).cores(&["dolphin"]).emulators(&["dolphin"]).wikidata(&["Q182172"]).ss(13).igdb(&[21]).launchbox(&["Nintendo GameCube"]).ident(&["gc"]).libretro("Nintendo - GameCube"),
    s(5, "Wii", "Nintendo Wii", "Nintendo", 7, 2006, "MD5", 1, 0, &[], &["iso", "wbfs", "rvz", "wia"]).redump(&["wii"]).cores(&["dolphin"]).emulators(&["dolphin"]).wikidata(&["Q8079"]).ss(16).igdb(&[5]).launchbox(&["Nintendo Wii"]).ident(&["wii"]).libretro("Nintendo - Wii"),
    s(6, "Game Boy", "Nintendo Game Boy", "Nintendo", 4, 1989, "CRC32", 0, 1, &["Nintendo - Game Boy"], &["gb", "sgb"]).nointro(&["Nintendo - Game Boy"]).hacks(&["Nintendo - Game Boy"]).homebrew(&["Nintendo - Game Boy"]).cores(&["gambatte", "sameboy", "mgba"]).emulators(&["mgba"]).wikidata(&["Q186437"]).ss(9).igdb(&[33]).launchbox(&["Nintendo Game Boy"]).ident(&["gb"]),
    s(7, "Game Boy Color", "Nintendo Game Boy Color", "Nintendo", 5, 1998, "CRC32", 0, 1, &["Nintendo - Game Boy Color"], &["gbc"]).nointro(&["Nintendo - Game Boy Color"]).hacks(&["Nintendo - Game Boy Color"]).cores(&["gambatte", "sameboy", "mgba"]).emulators(&["mgba"]).wikidata(&["Q203992"]).ss(10).igdb(&[22]).launchbox(&["Nintendo Game Boy Color"]).ident(&["gb"]),
    s(8, "Game Boy Advance", "Nintendo Game Boy Advance", "Nintendo", 6, 2001, "CRC32", 0, 1, &["Nintendo - Game Boy Advance"], &["gba", "agb"]).nointro(&["Nintendo - Game Boy Advance", "Nintendo - e-Reader"]).hacks(&["Nintendo - Game Boy Advance"]).homebrew(&["Nintendo - Game Boy Advance"]).cores(&["mgba", "vbam"]).emulators(&["mgba"]).wikidata(&["Q188642"]).ss(12).igdb(&[24]).launchbox(&["Nintendo Game Boy Advance"]).ident(&["gba"]),
    s(9, "Nintendo DS", "Nintendo DS", "Nintendo", 7, 2004, "CRC32", 0, 1, &["Nintendo - Nintendo DS"], &["nds", "dsi"]).nointro(&["Nintendo - Nintendo DS", "Nintendo - Nintendo DSi", "Nintendo - Nintendo DS (Download Play)"]).hacks(&["Nintendo - Nintendo DS"]).cores(&["melonds", "desmume"]).emulators(&["melonds"]).wikidata(&["Q170323"]).ss(15).igdb(&[20]).launchbox(&["Nintendo DS"]).ident(&["nds"]),
    s(10, "Genesis", "Sega Genesis / Mega Drive", "Sega", 4, 1988, "CRC32", 0, 0, &["Sega - Mega Drive - Genesis"], &["md", "gen", "smd", "bin"]).nointro(&["Sega - Mega Drive - Genesis"]).hacks(&["Sega - Mega Drive - Genesis"]).homebrew(&["Sega - Mega Drive - Genesis"]).cores(&["genesis_plus_gx", "picodrive"]).wikidata(&["Q10676"]).ss(1).igdb(&[29]).launchbox(&["Sega Genesis"]).ident(&["md"]),
    s(11, "Master System", "Sega Master System", "Sega", 3, 1985, "CRC32", 0, 0, &["Sega - Master System - Mark III"], &["sms"]).nointro(&["Sega - Master System - Mark III"]).hacks(&["Sega - Master System - Mark III"]).cores(&["genesis_plus_gx", "picodrive"]).wikidata(&["Q209868"]).ss(2).igdb(&[64]).launchbox(&["Sega Master System"]),
    s(12, "Saturn", "Sega Saturn", "Sega", 5, 1994, "MD5", 1, 0, &["Sega - Saturn"], &["cue", "bin", "iso", "chd", "ccd", "img", "mds"]).redump(&["ss"]).hacks(&["Sega - Saturn"]).cores(&["mednafen_saturn", "yabause"]).emulators(&["mednafen"]).wikidata(&["Q200912"]).ss(22).igdb(&[32]).launchbox(&["Sega Saturn"]).ident(&["saturn"]),
    s(13, "Dreamcast", "Sega Dreamcast", "Sega", 6, 1998, "MD5", 1, 0, &["Sega - Dreamcast"], &["gdi", "cdi", "chd"]).redump(&["dc"]).homebrew(&["Sega - Dreamcast"]).cores(&["flycast"]).emulators(&["flycast"]).wikidata(&["Q184198"]).ss(23).igdb(&[23]).launchbox(&["Sega Dreamcast"]).ident(&["dc"]),
    s(14, "PlayStation", "Sony PlayStation", "Sony", 5, 1994, "MD5", 1, 0, &["Sony - PlayStation"], &["cue", "bin", "iso", "chd", "img", "pbp"]).redump(&["psx", "psx-bios"]).hacks(&["Sony - PlayStation"]).cores(&["mednafen_psx_hw", "swanstation", "pcsx_rearmed"]).emulators(&["duckstation"]).wikidata(&["Q10677"]).ss(57).igdb(&[7]).launchbox(&["Sony Playstation"]).ident(&["ps1"]),
    s(15, "PlayStation 2", "Sony PlayStation 2", "Sony", 6, 2000, "MD5", 1, 0, &["Sony - PlayStation 2"], &["iso", "chd", "bin", "cue"]).redump(&["ps2", "ps2-bios"]).hacks(&["Sony - PlayStation 2"]).cores(&["pcsx2"]).emulators(&["pcsx2"]).wikidata(&["Q10680"]).ss(58).igdb(&[8]).launchbox(&["Sony Playstation 2"]).ident(&["ps2"]),
    s(16, "PSP", "PlayStation Portable", "Sony", 7, 2004, "MD5", 1, 1, &["Sony - PlayStation Portable"], &["iso", "cso", "pbp"]).redump(&["psp"]).nointro(&["Sony - PlayStation Portable", "Sony - PlayStation Portable (PSN)"]).hacks(&["Sony - PlayStation Portable"]).cores(&["ppsspp"]).emulators(&["ppsspp"]).wikidata(&["Q170325"]).ss(61).igdb(&[38]).launchbox(&["Sony PSP"]).ident(&["psp"]),
    s(17, "Atari 2600", "Atari 2600", "Atari", 2, 1977, "CRC32", 0, 0, &["Atari - Atari 2600", "Atari - 2600"], &["a26", "bin"]).nointro(&["Atari - 2600"]).cores(&["stella"]).wikidata(&["Q206261"]).ss(26).igdb(&[59]).launchbox(&["Atari 2600"]),
    s(18, "Atari 7800", "Atari 7800 ProSystem", "Atari", 3, 1986, "CRC32", 0, 0, &["Atari - Atari 7800", "Atari - 7800"], &["a78", "bin"]).nointro(&["Atari - 7800"]).cores(&["prosystem"]).wikidata(&["Q753600"]).ss(41).igdb(&[60]).launchbox(&["Atari 7800"]),
    s(19, "Lynx", "Atari Lynx", "Atari", 4, 1989, "CRC32", 0, 1, &["Atari - Atari Lynx", "Atari - Lynx"], &["lnx", "lyx", "o"]).nointro(&["Atari - Lynx"]).cores(&["handy", "mednafen_lynx"]).ss(28).igdb(&[61]).launchbox(&["Atari Lynx"]),
    s(20, "TurboGrafx-16", "TurboGrafx-16 / PC Engine", "NEC", 4, 1987, "CRC32", 0, 0, &["NEC - PC Engine - TurboGrafx-16"], &["pce"]).nointro(&["NEC - PC Engine - TurboGrafx 16"]).hacks(&["NEC - PC Engine - TurboGrafx 16"]).cores(&["mednafen_pce_fast", "mednafen_pce"]).wikidata(&["Q1057377"]).ss(31).igdb(&[86]).launchbox(&["NEC TurboGrafx-16"]),
    s(21, "TurboGrafx-CD", "TurboGrafx-CD / PC Engine CD", "NEC", 4, 1988, "MD5", 1, 0, &["NEC - PC Engine CD & TurboGrafx CD"], &["cue", "bin", "chd"]).redump(&["pce"]).cores(&["mednafen_pce", "mednafen_pce_fast"]).launchbox(&["NEC TurboGrafx-CD"]),
    s(22, "Neo Geo", "SNK Neo Geo", "SNK", 4, 1990, "CRC32", 0, 0, &[], &["neo"]).tosec(&["SNK Neo-Geo"]).cores(&["fbneo"]).wikidata(&["Q1054350"]).launchbox(&["SNK Neo Geo AES", "SNK Neo Geo MVS"]).libretro("SNK - Neo Geo"),
    s(23, "Sega CD", "Sega CD / Mega CD", "Sega", 4, 1991, "MD5", 1, 0, &["Sega - Mega-CD - Sega CD"], &["cue", "bin", "chd", "iso"]).redump(&["mcd"]).cores(&["genesis_plus_gx", "picodrive"]).wikidata(&["Q1047516"]).ss(20).igdb(&[78]).launchbox(&["Sega CD"]).ident(&["segacd"]),
    s(24, "Game Gear", "Sega Game Gear", "Sega", 4, 1990, "CRC32", 0, 1, &["Sega - Game Gear"], &["gg"]).nointro(&["Sega - Game Gear"]).hacks(&["Sega - Game Gear"]).cores(&["genesis_plus_gx"]).wikidata(&["Q751719"]).ss(21).igdb(&[35]).launchbox(&["Sega Game Gear"]),
    s(25, "Sega 32X", "Sega 32X", "Sega", 5, 1994, "CRC32", 0, 0, &["Sega - 32X"], &["32x"]).nointro(&["Sega - 32X"]).hacks(&["Sega - 32x"]).cores(&["picodrive"]).wikidata(&["Q1063978"]).ss(19).igdb(&[30]).launchbox(&["Sega 32X"]).ident(&["md"]),
    s(26, "Atari Jaguar", "Atari Jaguar", "Atari", 5, 1993, "CRC32", 0, 0, &["Atari - Atari Jaguar", "Atari - Jaguar"], &["j64", "jag", "rom", "abs", "cof"]).nointro(&["Atari - Jaguar"]).cores(&["virtualjaguar"]).wikidata(&["Q650601"]).ss(27).igdb(&[62]).launchbox(&["Atari Jaguar"]),
    s(27, "Neo Geo Pocket", "Neo Geo Pocket / Color", "SNK", 5, 1998, "CRC32", 0, 1, &["SNK - NeoGeo Pocket", "SNK - NeoGeo Pocket Color"], &["ngp", "ngc"]).nointro(&["SNK - Neo Geo Pocket", "SNK - Neo Geo Pocket Color"]).hacks(&["SNK - Neo Geo Pocket Color"]).cores(&["mednafen_ngp"]).ss(25).igdb(&[119]).launchbox(&["SNK Neo Geo Pocket", "SNK Neo Geo Pocket Color"]),
    s(28, "WonderSwan", "Bandai WonderSwan / Color", "Bandai", 5, 1999, "CRC32", 0, 1, &["Bandai - WonderSwan", "Bandai - WonderSwan Color"], &["ws", "wsc"]).nointro(&["Bandai - WonderSwan", "Bandai - WonderSwan Color"]).hacks(&["Bandai - WonderSwan", "Bandai - WonderSwan Color"]).cores(&["mednafen_wswan"]).wikidata(&["Q1065792"]).ss(45).igdb(&[57]).launchbox(&["WonderSwan", "WonderSwan Color"]),
    s(29, "Virtual Boy", "Nintendo Virtual Boy", "Nintendo", 5, 1995, "CRC32", 0, 1, &["Nintendo - Virtual Boy"], &["vb", "vboy"]).nointro(&["Nintendo - Virtual Boy"]).cores(&["mednafen_vb"]).wikidata(&["Q164651"]).ss(11).igdb(&[87]).launchbox(&["Nintendo Virtual Boy"]),
    s(30, "Nintendo 3DS", "Nintendo 3DS", "Nintendo", 8, 2011, "MD5", 1, 1, &["Nintendo - Nintendo 3DS"], &["3ds", "cia", "cci", "cxi"]).nointro(&["Nintendo - Nintendo 3DS", "Nintendo - New Nintendo 3DS"]).wikidata(&["Q203597"]).igdb(&[37]).launchbox(&["Nintendo 3DS"]),
    s(31, "Nintendo Switch", "Nintendo Switch", "Nintendo", 9, 2017, "SHA1", 1, 1, &[], &["nsp", "xci"]).wikidata(&["Q19610114"]).igdb(&[130]).launchbox(&["Nintendo Switch"]),
    s(32, "PlayStation Vita", "Sony PlayStation Vita", "Sony", 8, 2011, "MD5", 0, 1, &[], &["vpk"]).nointro(&["Sony - PlayStation Vita"]).wikidata(&["Q188808"]).igdb(&[46]).launchbox(&["Sony Playstation Vita"]),
    s(33, "Commodore 64", "Commodore 64", "Commodore", 2, 1982, "CRC32", 0, 0, &[], &["d64", "t64", "prg", "crt", "tap", "g64"]).nointro(&["Commodore - 64"]).tosec(&["Commodore C64"]).cores(&["vice_x64"]).wikidata(&["Q99775"]).ss(66).igdb(&[15]).launchbox(&["Commodore 64"]),
    s(34, "Amiga", "Commodore Amiga", "Commodore", 3, 1985, "CRC32", 0, 0, &[], &["adf", "ipf", "hdf", "lha"]).nointro(&["Commodore - Amiga"]).tosec(&["Commodore Amiga"]).cores(&["puae"]).wikidata(&["Q100047"]).ss(64).igdb(&[16]).launchbox(&["Commodore Amiga"]),
    s(35, "ZX Spectrum", "Sinclair ZX Spectrum", "Sinclair", 2, 1982, "CRC32", 0, 0, &[], &["tzx", "tap", "z80", "sna", "dsk"]).tosec(&["Sinclair ZX Spectrum"]).cores(&["fuse"]).wikidata(&["Q23882"]).ss(76).igdb(&[26]).launchbox(&["Sinclair ZX Spectrum"]).libretro("Sinclair - ZX Spectrum"),
    s(36, "SuperGrafx", "NEC PC Engine SuperGrafx", "NEC", 4, 1989, "CRC32", 0, 0, &["NEC - PC Engine SuperGrafx"], &["sgx", "pce"]).nointro(&["NEC - PC Engine SuperGrafx"]).cores(&["mednafen_supergrafx"]).launchbox(&["PC Engine SuperGrafx"]),
    s(37, "Xbox", "Microsoft Xbox", "Microsoft", 6, 2001, "MD5", 1, 0, &[], &["iso", "xiso"]).redump(&["xbox", "xbox-bios"]).emulators(&["xemu"]).wikidata(&["Q132020"]).igdb(&[11]).launchbox(&["Microsoft Xbox"]).libretro("Microsoft - Xbox"),
    s(38, "Xbox 360", "Microsoft Xbox 360", "Microsoft", 7, 2005, "MD5", 1, 0, &[], &["iso", "xex"]).redump(&["xbox360"]).wikidata(&["Q48263"]).igdb(&[12]).launchbox(&["Microsoft Xbox 360"]).libretro("Microsoft - Xbox 360"),
    s(39, "Arcade", "Arcade (MAME)", "Various", 0, 1970, "SHA1", 0, 0, &[], &[]).cores(&["mame"]).emulators(&["mame"]).launchbox(&["Arcade"]).libretro("MAME").arcade(),
    s(40, "3DO", "3DO Interactive Multiplayer", "Panasonic", 5, 1993, "MD5", 1, 0, &[], &["iso", "cue", "bin", "chd"]).redump(&["3do"]).cores(&["opera"]).ss(29).igdb(&[50]).launchbox(&["3DO Interactive Multiplayer"]).libretro("The 3DO Company - 3DO"),
    s(41, "Neo Geo CD", "SNK Neo Geo CD", "SNK", 5, 1994, "MD5", 1, 0, &[], &["cue", "bin", "chd"]).redump(&["ngcd"]).cores(&["neocd"]).launchbox(&["SNK Neo Geo CD"]).libretro("SNK - Neo Geo CD"),
    s(42, "FDS", "Nintendo Famicom Disk System", "Nintendo", 3, 1986, "CRC32", 0, 0, &["Nintendo - Family Computer Disk System"], &["fds", "qd"]).nointro(&["Nintendo - Family Computer Disk System"]).cores(&["mesen", "fceumm"]).wikidata(&["Q135321"]).ss(106).igdb(&[51]).launchbox(&["Nintendo Famicom Disk System"]),
    s(43, "Atari 5200", "Atari 5200", "Atari", 2, 1982, "CRC32", 0, 0, &["Atari - Atari 5200", "Atari - 5200"], &["a52", "bin"]).nointro(&["Atari - 5200"]).cores(&["atari800"]).launchbox(&["Atari 5200"]),
    s(44, "Atari 8-bit", "Atari 8-bit Family", "Atari", 2, 1979, "CRC32", 0, 0, &[], &["atr", "xex", "xfd", "car", "cas", "rom"]).nointro(&["Atari - 8-bit Family"]).tosec(&["Atari 8bit"]).cores(&["atari800"]).launchbox(&["Atari 800", "Atari XEGS"]),
    s(45, "Atari ST", "Atari ST", "Atari", 3, 1985, "CRC32", 0, 0, &[], &["st", "stx", "msa"]).nointro(&["Atari - ST"]).tosec(&["Atari ST"]).cores(&["hatari"]).wikidata(&["Q627302"]).launchbox(&["Atari ST"]),
    s(46, "ColecoVision", "ColecoVision", "Coleco", 2, 1982, "CRC32", 0, 0, &["Coleco - ColecoVision"], &["col", "cv"]).nointro(&["Coleco - ColecoVision"]).cores(&["gearcoleco", "bluemsx"]).launchbox(&["ColecoVision"]),
    s(47, "Intellivision", "Mattel Intellivision", "Mattel", 2, 1979, "CRC32", 0, 0, &["Mattel - Intellivision"], &["int", "rom", "bin"]).nointro(&["Mattel - Intellivision"]).homebrew(&["Mattel - Intellivision"]).cores(&["freeintv"]).launchbox(&["Mattel Intellivision"]),
    s(48, "MSX", "MSX", "Microsoft", 3, 1983, "CRC32", 0, 0, &["Microsoft - MSX"], &["rom", "mx1", "mx2", "dsk", "cas"]).nointro(&["Microsoft - MSX"]).tosec(&["MSX MSX"]).cores(&["bluemsx", "fmsx"]).wikidata(&["Q853547"]).ss(113).igdb(&[27]).launchbox(&["Microsoft MSX"]),
    s(49, "MSX2", "MSX2", "Microsoft", 3, 1985, "CRC32", 0, 0, &["Microsoft - MSX2"], &["rom", "mx2", "dsk", "cas"]).nointro(&["Microsoft - MSX2"]).tosec(&["MSX MSX2", "MSX MSX2+"]).hacks(&["Microsoft - MSX2"]).cores(&["bluemsx", "fmsx"]).launchbox(&["Microsoft MSX2", "Microsoft MSX2+"]),
    s(50, "PC-FX", "NEC PC-FX", "NEC", 5, 1994, "MD5", 1, 0, &[], &["cue", "bin", "chd"]).redump(&["pc-fx"]).cores(&["mednafen_pcfx"]).launchbox(&["NEC PC-FX"]).libretro("NEC - PC-FX"),
    s(51, "CD-i", "Philips CD-i", "Philips", 4, 1991, "MD5", 1, 0, &[], &["chd", "iso", "cue", "bin"]).redump(&["cdi"]).cores(&["same_cdi"]).launchbox(&["Philips CD-i"]).libretro("Philips - CD-i"),
    s(52, "CD32", "Commodore Amiga CD32", "Commodore", 5, 1993, "MD5", 1, 0, &[], &["cue", "bin", "chd", "iso"]).redump(&["cd32"]).launchbox(&["Commodore Amiga CD32"]).libretro("Commodore - CD32"),
    s(53, "Naomi", "Sega Naomi", "Sega", 6, 1998, "MD5", 1, 0, &[], &["chd"]).redump(&["naomi"]).cores(&["flycast"]).emulators(&["flycast"]).launchbox(&["Sega Naomi", "Sega Naomi 2"]).libretro("Sega - Naomi"),
    s(54, "SG-1000", "Sega SG-1000", "Sega", 3, 1983, "CRC32", 0, 0, &["Sega - SG-1000"], &["sg"]).nointro(&["Sega - SG-1000"]).cores(&["genesis_plus_gx"]).wikidata(&["Q1136956"]).ss(109).launchbox(&["Sega SG-1000"]),
    s(55, "Atari Jaguar CD", "Atari Jaguar CD", "Atari", 5, 1995, "MD5", 1, 0, &[], &["cue", "bin", "chd"]).redump(&["ajcd"]).launchbox(&["Atari Jaguar CD"]),
    s(56, "Wii U", "Nintendo Wii U", "Nintendo", 8, 2012, "SHA1", 1, 0, &[], &["wud", "wux", "rpx"]).emulators(&["cemu"]).igdb(&[41]).launchbox(&["Nintendo Wii U"]).libretro("Nintendo - Wii U"),
    s(57, "PlayStation 3", "Sony PlayStation 3", "Sony", 7, 2006, "SHA1", 1, 0, &[], &["iso", "pkg"]).redump(&["ps3"]).emulators(&["rpcs3"]).wikidata(&["Q10683"]).igdb(&[9]).launchbox(&["Sony Playstation 3"]).libretro("Sony - PlayStation 3"),
    s(58, "Amstrad CPC", "Amstrad CPC", "Amstrad", 3, 1984, "CRC32", 0, 0, &[], &["dsk", "sna", "cdt"]).tosec(&["Amstrad CPC"]).cores(&["cap32"]).launchbox(&["Amstrad CPC"]).libretro("Amstrad - CPC"),
    s(59, "Enterprise 128", "Enterprise 64/128", "Intelligent Software", 3, 1985, "CRC32", 0, 0, &[], &["img", "dsk"]).tosec(&["Enterprise 64 & 128"]).launchbox(&["Enterprise"]),
    s(60, "ZX 81", "Sinclair ZX 81", "Sinclair", 2, 1981, "CRC32", 0, 0, &[], &["p", "81"]).tosec(&["Sinclair ZX81"]).launchbox(&["Sinclair ZX-81"]).libretro("Sinclair - ZX 81"),
    s(61, "Videoton TVC", "Videoton TVC", "Videoton", 3, 1985, "CRC32", 0, 0, &[], &["cas"]).tosec(&["Videoton TV-Computer"]),
    s(62, "Sega Pico", "Sega Pico", "Sega", 4, 1993, "CRC32", 0, 0, &["Sega - PICO"], &["md", "bin"]).nointro(&["Sega - PICO"]).cores(&["picodrive"]).launchbox(&["Sega Pico"]),
    s(63, "VIC-20", "Commodore VIC-20", "Commodore", 2, 1981, "CRC32", 0, 0, &[], &["prg", "crt", "d64", "tap"]).nointro(&["Commodore - VIC-20"]).tosec(&["Commodore VIC20"]).cores(&["vice_xvic"]).launchbox(&["Commodore VIC-20"]),
    s(64, "Odyssey2", "Magnavox Odyssey2 / Philips Videopac", "Magnavox", 2, 1978, "CRC32", 0, 0, &["Magnavox - Odyssey 2"], &["bin"]).nointro(&["Magnavox - Odyssey2"]).cores(&["o2em"]).launchbox(&["Magnavox Odyssey 2", "Philips Videopac+"]),
    s(65, "Supervision", "Watara Supervision", "Watara", 4, 1992, "CRC32", 0, 1, &["Watara - Supervision"], &["sv"]).nointro(&["Watara - Supervision"]).cores(&["potator"]).launchbox(&["Watara Supervision"]),
    s(66, "Pocket Challenge V2", "Benesse Pocket Challenge V2", "Benesse", 5, 1999, "CRC32", 0, 1, &[], &["pc2"]).nointro(&["Benesse - Pocket Challenge V2"]),
    s(67, "PC-98", "NEC PC-98", "NEC", 3, 1982, "CRC32", 0, 0, &[], &["hdi", "fdi", "d88", "hdm", "nhd", "fdd", "thd", "hdd", "nfd", "d98", "98d", "xdf", "dup", "2hd", "tfd", "dcp", "dcu"]).tosec(&["NEC PC-9801", "NEC PC-9821"]).cores(&["np2kai"]).wikidata(&["Q183505"]).ss(208).igdb(&[149]).launchbox(&["NEC PC-9801"]).libretro("NEC - PC-98").dats(&["NEC - PC-98"]).loose_sets("cmd"),
    s(68, "Interton VC 4000", "Interton VC 4000", "Interton", 2, 1978, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Interton - VC 4000"]).launchbox(&["Interton VC 4000"]),
    s(69, "Arcadia 2001", "Emerson Arcadia 2001", "Emerson", 2, 1982, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Emerson - Arcadia 2001"]).launchbox(&["Emerson Arcadia 2001"]),
    s(70, "Vectrex", "GCE Vectrex", "GCE", 2, 1982, "CRC32", 0, 0, &["GCE - Vectrex"], &["vec", "gam", "bin"]).nointro(&["GCE - Vectrex"]).homebrew(&["GCE - Vectrex"]).cores(&["vecx"]).launchbox(&["GCE Vectrex"]),
    s(71, "Pokemon Mini", "Nintendo Pokémon Mini", "Nintendo", 6, 2001, "CRC32", 0, 1, &["Nintendo - Pokemon Mini"], &["min"]).nointro(&["Nintendo - Pokemon Mini"]).homebrew(&["Nintendo - Pokemon Mini"]).cores(&["pokemini"]).launchbox(&["Nintendo Pokemon Mini"]),
    s(72, "Channel F", "Fairchild Channel F", "Fairchild", 2, 1976, "CRC32", 0, 0, &["Fairchild - Channel F"], &["chf", "bin"]).nointro(&["Fairchild - Channel F"]).cores(&["freechaf"]).launchbox(&["Fairchild Channel F"]),
    s(73, "Super Cassette Vision", "Epoch Super Cassette Vision", "Epoch", 2, 1984, "CRC32", 0, 0, &[], &["bin", "0"]).nointro(&["Epoch - Super Cassette Vision"]).launchbox(&["Epoch Super Cassette Vision"]),
    s(74, "GP32", "GamePark GP32", "GamePark", 6, 2001, "CRC32", 0, 1, &[], &["gxb"]).nointro(&["GamePark - GP32"]).launchbox(&["GamePark GP32"]),
    s(75, "Game.com", "Tiger Game.com", "Tiger", 5, 1997, "CRC32", 0, 1, &[], &["tgc"]).nointro(&["Tiger - Game.com"]).launchbox(&["Tiger Game.com"]),
    s(76, "Studio II", "RCA Studio II", "RCA", 2, 1977, "CRC32", 0, 0, &[], &["st2", "bin"]).nointro(&["RCA - Studio II"]).launchbox(&["RCA Studio II"]),
    s(77, "Atomiswave", "Sega Atomiswave", "Sammy", 6, 2003, "MD5", 0, 0, &[], &["bin"]).cores(&["flycast"]).emulators(&["flycast"]).launchbox(&["Sammy Atomiswave"]).libretro("Atomiswave"),
    s(78, "Casio PV-1000", "Casio PV-1000", "Casio", 2, 1983, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Casio - PV-1000"]).launchbox(&["Casio PV-1000"]),
    s(79, "Super A'Can", "Funtech Super A'Can", "Funtech", 4, 1995, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Funtech - Super Acan"]).launchbox(&["Funtech Super Acan"]),
    s(80, "Casio Loopy", "Casio Loopy", "Casio", 5, 1995, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Casio - Loopy"]).launchbox(&["Casio Loopy"]),
    s(81, "Sharp X1", "Sharp X1", "Sharp", 3, 1982, "CRC32", 0, 0, &[], &["2d", "d88", "tap", "dx1", "2hd", "tfd", "88d", "hdm", "xdf", "dup"]).nointro(&["Sharp - X1"]).tosec(&["Sharp X1", "Sharp X1 Turbo"]).cores(&["x1"]).wikidata(&["Q2710884"]).ss(220).igdb(&[77]).launchbox(&["Sharp X1"]).loose_sets(""),
    s(82, "X68000", "Sharp X68000", "Sharp", 4, 1987, "CRC32", 0, 0, &[], &["dim", "xdf", "hdf", "d88"]).nointro(&["Sharp - X68000"]).tosec(&["Sharp X68000"]).cores(&["px68k"]).wikidata(&["Q1758277"]).ss(79).igdb(&[121]).launchbox(&["Sharp X68000"]),
    s(83, "PC", "IBM PC Compatible", "IBM", 5, 1981, "MD5", 1, 0, &[], &["iso", "img"]).redump(&["pc"]).cores(&["dosbox_pure"]).launchbox(&["Windows", "Windows 3.X"]),
    s(84, "PlayStation 4", "Sony PlayStation 4", "Sony", 8, 2013, "SHA1", 1, 0, &[], &["pkg"]).launchbox(&["Sony Playstation 4"]).libretro("Sony - PlayStation 4"),
    s(85, "Xbox One", "Microsoft Xbox One", "Microsoft", 8, 2013, "SHA1", 1, 0, &[], &[]).launchbox(&["Microsoft Xbox One"]),
    s(86, "Mac", "Apple Macintosh", "Apple", 3, 1984, "MD5", 1, 0, &[], &["dsk", "img", "hfv"]).redump(&["mac"]).tosec(&["Apple Macintosh"]).launchbox(&["Apple Mac OS"]),
    s(87, "FM Towns", "Fujitsu FM Towns", "Fujitsu", 4, 1989, "MD5", 1, 0, &[], &["cue", "bin", "chd", "iso"]).redump(&["fmt"]).tosec(&["Fujitsu FM Towns"]).emulators(&["mame-fmtowns"]).wikidata(&["Q531896"]).ss(253).igdb(&[118]).launchbox(&["Fujitsu FM Towns Marty"]),
    s(88, "PC-88", "NEC PC-8801", "NEC", 3, 1981, "CRC32", 0, 0, &[], &["d88", "t88", "cmt", "u88"]).tosec(&["NEC PC-8801"]).cores(&["quasi88"]).wikidata(&["Q1338888"]).ss(221).igdb(&[125]).launchbox(&["NEC PC-8801"]).libretro("NEC - PC-8001 - PC-8801").loose_sets("m3u"),
    s(89, "CDTV", "Commodore CDTV", "Commodore", 4, 1991, "MD5", 1, 0, &[], &["cue", "bin", "chd", "iso"]).redump(&["cdtv"]).launchbox(&["Commodore CDTV"]).libretro("Commodore - CDTV"),
    s(90, "Amiga CD", "Amiga CD", "Commodore", 4, 1993, "MD5", 1, 0, &[], &["cue", "bin", "chd", "iso"]).redump(&["acd"]),
    s(91, "Archimedes", "Acorn Archimedes", "Acorn", 4, 1987, "MD5", 1, 0, &[], &["adf", "apd"]).redump(&["arch"]).tosec(&["Acorn Archimedes"]).launchbox(&["Acorn Archimedes"]),
    s(92, "Triforce", "Sega Triforce", "Sega", 6, 2002, "MD5", 1, 0, &[], &["iso"]).redump(&["trf"]).launchbox(&["Sega Triforce"]),
    s(93, "Chihiro", "Sega Chihiro", "Sega", 6, 2002, "MD5", 1, 0, &[], &["chd"]).redump(&["chihiro"]),
    s(94, "VIS", "Tandy VIS", "Tandy", 5, 1992, "MD5", 1, 0, &[], &["cue", "bin", "chd"]).redump(&["vis"]),
    s(95, "Lindbergh", "Sega Lindbergh", "Sega", 7, 2005, "MD5", 1, 0, &[], &[]).redump(&["lindbergh"]),
    s(96, "Palm OS", "Palm OS", "Palm", 6, 1996, "CRC32", 0, 1, &[], &["prc", "pdb"]).nointro(&["Mobile - Palm OS"]),
    s(97, "Playdia", "Bandai Playdia", "Bandai", 5, 1994, "MD5", 1, 0, &[], &["cue", "bin", "chd"]),
    s(98, "Apple II", "Apple II / IIe / IIc / IIGS", "Apple", 2, 1977, "CRC32", 0, 0, &[], &["dsk", "do", "po", "nib", "woz", "2mg"]).tosec(&["Apple II", "Apple IIGS"]).wikidata(&["Q201652"]).launchbox(&["Apple II", "Apple IIGS"]),
    s(99, "BBC Micro", "Acorn BBC Micro / Electron", "Acorn", 3, 1981, "CRC32", 0, 0, &[], &["ssd", "dsd", "uef"]).tosec(&["Acorn BBC", "Acorn Electron"]).launchbox(&["BBC Microcomputer System", "Acorn Electron"]),
    s(100, "Commodore 16", "Commodore 16 / Plus/4", "Commodore", 3, 1984, "CRC32", 0, 0, &[], &["prg", "d64", "tap"]).nointro(&["Commodore - Plus-4"]).tosec(&["Commodore C16, C116 & Plus-4"]).cores(&["vice_xplus4"]).launchbox(&["Commodore Plus 4"]),
    s(101, "Mega Duck", "Mega Duck / Cougar Boy", "Welback", 4, 1993, "CRC32", 0, 1, &["Welback - Mega Duck"], &["bin"]).tosec(&["Creatronic Mega Duck & Cougar Boy"]).cores(&["sameduck"]).launchbox(&["Mega Duck"]),
    s(102, "Microvision", "Milton Bradley Microvision", "Milton Bradley", 1, 1979, "CRC32", 0, 1, &[], &["bin"]),
    s(103, "Nuon", "VM Labs Nuon", "VM Labs", 6, 1999, "MD5", 1, 0, &[], &["iso"]).redump(&["nuon"]).launchbox(&["Nuon"]),
    s(104, "V.Smile", "VTech V.Smile", "VTech", 6, 2004, "CRC32", 0, 0, &[], &["bin"]).nointro(&["VTech - V.Smile"]).launchbox(&["VTech V.Smile"]),
    s(105, "Leapster", "LeapFrog Leapster", "LeapFrog", 6, 2003, "CRC32", 0, 1, &[], &["bin"]).nointro(&["LeapFrog - Leapster Learning Game System"]),
    s(106, "LeapPad", "LeapFrog LeapPad", "LeapFrog", 5, 1999, "CRC32", 0, 0, &[], &["bin"]).nointro(&["LeapFrog - LeapPad"]),
    s(107, "Beena", "Sega Beena", "Sega", 7, 2005, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Sega - Beena"]),
    s(108, "CreatiVision", "VTech CreatiVision", "VTech", 2, 1981, "CRC32", 0, 0, &[], &["bin", "rom"]).nointro(&["VTech - CreatiVision"]).launchbox(&["VTech CreatiVision"]),
    s(109, "Picno", "Konami Picno", "Konami", 4, 1993, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Konami - Picno"]),
    s(110, "Game Master", "Hartung Game Master", "Hartung", 4, 1990, "CRC32", 0, 1, &[], &["bin"]).nointro(&["Hartung - Game Master"]).launchbox(&["Hartung Game Master"]),
    s(111, "Symbian", "Mobile Symbian", "Nokia", 7, 2000, "CRC32", 0, 1, &[], &["sis"]).nointro(&["Mobile - Symbian"]),
    s(112, "Adventure Vision", "Entex Adventure Vision", "Entex", 2, 1982, "CRC32", 0, 0, &[], &["bin"]).nointro(&["Entex - Adventure Vision"]).launchbox(&["Entex Adventure Vision"]),
    s(113, "DOS", "MS-DOS", "Microsoft", 4, 1981, "MD5", 0, 0, &[], &["exe", "com", "bat", "ima", "imd", "td0", "vfd"]).tosec(&["IBM PC Compatibles", "IBM PCjr"]).dats(&["DOS"]).cores(&["dosbox_pure", "dosbox_core", "dosbox_svn"]).emulators(&["dosbox-staging", "dosbox-x"]).wikidata(&["Q170434", "Q47604"]).ss(135).igdb(&[13]).launchbox(&["MS-DOS"]).libretro("DOS").folders(Folders::Zip),
    s(114, "ScummVM", "ScummVM", "ScummVM", 0, 2001, "MD5", 0, 0, &[], &[]).cores(&["scummvm"]).emulators(&["scummvm"]).ss(123).libretro("ScummVM").titles(&["ScummVM"]).folders(Folders::Kept),
    s(115, "FM-7", "Fujitsu FM-7 / FM-77", "Fujitsu", 3, 1982, "CRC32", 0, 0, &[], &["d77", "t77"]).tosec(&["Fujitsu FM-7", "Fujitsu FM77-AV"]).emulators(&["mame-fm7"]).wikidata(&["Q2379925"]).ss(97).igdb(&[152]).loose_sets(""),
    s(116, "MAME 2003-Plus", "Arcade (MAME 2003-Plus)", "Various", 0, 1970, "SHA1", 0, 0, &[], &[]).urls(&["https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/mame/MAME%202003-Plus%20XML.xml"]).cores(&["mame2003_plus"]).launchbox(&["Arcade"]).libretro("MAME").arcade(),
    s(117, "FBNeo", "Arcade (FinalBurn Neo)", "Various", 0, 1970, "CRC32", 0, 0, &[], &[]).urls(&["https://raw.githubusercontent.com/libretro/FBNeo/master/dats/FinalBurn%20Neo%20(ClrMame%20Pro%20XML%2C%20Arcade%20only).dat"]).cores(&["fbneo"]).launchbox(&["Arcade"]).libretro("FBNeo - Arcade Games").arcade(),
];

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
mod tests {
    use super::*;

    #[test]
    fn ids_and_keys_are_unique() {
        for (i, a) in SYSTEMS.iter().enumerate() {
            for b in &SYSTEMS[i + 1..] {
                assert_ne!(a.id, b.id);
                assert!(!a.key.eq_ignore_ascii_case(b.key), "{}", a.key);
            }
        }
    }

    #[test]
    fn dat_names_resolve() {
        let nes = from_dat_name("Nintendo - Nintendo Entertainment System (Headered) (Parent-Clone)");
        assert_eq!(nes.map(|s| s.key), Some("NES"));
        assert_eq!(
            from_dat_name("Nintendo - Game Boy").map(|s| s.key),
            Some("Game Boy")
        );
        assert_eq!(
            from_dat_name("Nintendo - Game Boy Color (20240101)").map(|s| s.key),
            Some("Game Boy Color")
        );
        assert_eq!(
            from_dat_name("Atari - Atari Lynx (LYX)").map(|s| s.key),
            Some("Lynx")
        );
        assert_eq!(from_dat_name("Something Else"), None);
    }

    #[test]
    fn extensions_are_lowercase_and_not_archives() {
        for s in SYSTEMS {
            for e in s.extensions {
                assert_eq!(*e, e.to_ascii_lowercase(), "{}", s.key);
                assert!(!e.starts_with('.') && *e != "zip" && *e != "7z", "{}", s.key);
            }
        }
        assert!(is_rom_extension("SFC"));
        assert!(!is_rom_extension("mkv"));
        assert!(!is_rom_extension("url"));
        assert!(!is_rom_extension("nfo"));
    }

    #[test]
    fn by_key_is_case_insensitive() {
        assert_eq!(by_key("snes").map(|s| s.id), Some(2));
        assert!(by_key("nope").is_none());
    }
}
