//! The subcommands of `dat`, `organize`, `cleanup`, `search`, ….

use std::path::PathBuf;

use clap::Subcommand;

use crate::args::OrganizeArgs;

#[derive(Subcommand)]
pub enum DatCommand {
    /// Import Logiqx XML DATs (No-Intro, Redump, ...), or zips of them
    Import {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// System key (see `romforge-cli dat systems`); guessed from the DAT name if omitted
        #[arg(long)]
        system: Option<String>,
    },
    /// Download and import the DATs of systems (see `dat systems`)
    ///
    /// Cartridge systems get No-Intro DATs from libretro-database's mirror,
    /// disc systems Redump game and BIOS DATs from redump.org, and home
    /// computers TOSEC DATs from tosecdev.org's pack. Run it again to update.
    Download {
        #[arg(required_unless_present = "all")]
        systems: Vec<String>,
        /// Every system that has a DAT source
        #[arg(long, conflicts_with = "systems")]
        all: bool,
        /// Also libretro's ROM hack and homebrew DATs
        #[arg(long)]
        extras: bool,
    },
    /// Import the installed MAME's own list (`mame -listxml`: SHA1 of every
    /// ROM and disk) as the Arcade system's DAT; run again after updating
    /// MAME
    Mame,
    /// Compare imported DATs with No-Intro's latest versions on DAT-o-MATIC
    Check {
        /// Also list No-Intro DATs you haven't imported
        #[arg(long)]
        all: bool,
    },
    /// List imported DATs
    List,
    /// List known system keys
    Systems,
}

#[derive(Subcommand)]
pub enum OrganizeCommand {
    /// Show what would move where; changes nothing
    Plan(OrganizeArgs),
    /// Move the files as `plan` shows, journaled for `undo`
    Apply(OrganizeArgs),
    /// Move the files of the latest batch (or --batch N) back
    Undo {
        #[arg(long)]
        batch: Option<i64>,
    },
    /// List applied batches
    History,
}

#[derive(Subcommand)]
pub enum DatabasesCommand {
    /// What is downloaded, and when
    List,
    /// Download (or update) one: openvgdb or launchbox
    Download { name: String },
    /// Build one from a zip you downloaded yourself
    Import { name: String, zip: PathBuf },
    /// Delete the local copy
    Remove { name: String },
}

#[derive(Subcommand)]
pub enum HomeCommand {
    /// Create the RomForge folder in PARENT (or use PARENT if it is one)
    /// and make it the one used; an existing library (database, art,
    /// downloaded databases) moves into it
    Create { parent: PathBuf },
    /// Where the RomForge folder is
    Show,
    /// Move the RomForge folder into NEW_PARENT (everything in it comes
    /// along; nothing is ever replaced)
    Move { new_parent: PathBuf },
}

#[derive(Subcommand)]
pub enum ScummvmCommand {
    /// Ask the installed ScummVM for its detection list
    /// (`--dump-all-detection-entries`), keep it in the databases folder
    /// and name the ScummVM games in every scanned folder by it; then ask
    /// ScummVM itself (`--detect`) about those folders, for the engines
    /// its list leaves out (SCUMM, SCI, Sky, Glk)
    Read,
    /// The same from a list you saved yourself
    /// (`scummvm --dump-all-detection-entries > list.dat`)
    Import { file: PathBuf },
}

#[derive(Subcommand)]
pub enum CleanupCommand {
    /// Show what would be written; changes nothing
    Plan {
        /// Also list each game's files and what happens to them
        #[arg(long, short)]
        verbose: bool,
    },
    /// Write the files as `plan` shows, journaled for `undo`
    Apply {
        #[arg(long, short)]
        verbose: bool,
    },
    /// Remove the latest batch's (or --batch N's) files and restore the
    /// originals
    Undo {
        #[arg(long)]
        batch: Option<i64>,
    },
    /// List applied batches
    History,
    /// Delete the originals kept in the trash for undo, for good (their
    /// batches can't be undone after). Shows what would go unless --yes
    EmptyTrash {
        /// Only batches made more than this many days ago
        #[arg(long, value_name = "DAYS")]
        older_than: Option<u32>,
        /// Delete them
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum SearchCommand {
    /// What needs you, with each row's id
    Queue,
    /// Search for a row: DAT titles (offline) or Wikidata items like its
    /// title or file name (or TEXT)
    #[command(alias = "suggest")]
    Find {
        row: String,
        text: Option<String>,
        /// dat or wikidata
        #[arg(long, default_value = "dat")]
        scraper: String,
        /// Only this platform (system key)
        #[arg(long)]
        system: Option<String>,
    },
    /// Name a row's files yourself ("identified by you"), or as a
    /// translation or hack of a DAT game ("translation of X")
    Name {
        row: String,
        #[arg(long)]
        system: String,
        #[arg(long, required_unless_present_any = ["translation_of", "hack_of"])]
        title: Option<String>,
        /// The DAT game it is a translation of (its title is made from it)
        #[arg(long, conflicts_with_all = ["title", "hack_of"])]
        translation_of: Option<String>,
        /// The DAT game it is a hack of
        #[arg(long, conflicts_with = "title")]
        hack_of: Option<String>,
        /// What the translation or hack is ("English v2.0 Addendum")
        #[arg(long)]
        tag: Option<String>,
    },
    /// Undo `name`
    Forget { row: String },
    /// Settle a row's Wikidata item (e.g. Q12345)
    Wikidata { row: String, qid: String },
}

#[derive(Subcommand)]
pub enum FirmwareCommand {
    /// Fetch RetroBIOS's list of the files each emulator loads (the list
    /// only, never BIOS files)
    Download,
    /// Check the RomForge folder's Bios/ against it
    Check {
        /// Only this system (as `dat systems` names it)
        #[arg(long)]
        system: Option<String>,
        /// Show every file, not only required ones and ones you have
        #[arg(long)]
        all: bool,
    },
    /// Put BIOS files from folders, files, zips or 7z where each emulator
    /// finds them, matched by contents (shows the plan unless --apply)
    Import {
        #[arg(required = true)]
        paths: Vec<std::path::PathBuf>,
        /// Do it (copies; never replaces a file; `firmware undo` takes it back)
        #[arg(long)]
        apply: bool,
        /// Also place this file whose name fits but whose hash isn't known
        /// (as the plan shows it); may be given several times
        #[arg(long)]
        also: Vec<String>,
    },
    /// Take away what the last import wrote
    Undo,
}

#[derive(Subcommand)]
pub enum EmulatorCommand {
    /// Emulators romforge knows, and which are installed
    List {
        /// Also those not installed
        #[arg(long)]
        all: bool,
    },
    /// Add (or replace) an emulator for a system; tried before the defaults
    Add {
        name: String,
        #[arg(long)]
        system: String,
        /// Executable (name on PATH, or a full path)
        #[arg(long)]
        program: String,
        /// Arguments as one string, split at spaces; {rom} is the file
        /// (e.g. --args "--fullscreen {rom}")
        #[arg(long, allow_hyphen_values = true, default_value = "{rom}")]
        args: String,
        /// It opens ROMs inside zip/7z itself
        #[arg(long)]
        archives: bool,
    },
    /// Emulator Settings: what is set for every emulator and for each one
    Settings,
    /// Set one of Emulator Settings (e.g. `set global fullscreen false`,
    /// `set duckstation program /opt/DuckStation.AppImage`); no value: unset
    Set {
        /// `global`, or an emulator (retroarch, duckstation, ...)
        emulator: String,
        /// fullscreen, program or args
        key: String,
        #[arg(allow_hyphen_values = true)]
        value: Option<String>,
    },
    /// Remove an emulator you added
    Remove { name: String },
    /// Copy an emulator's existing setup (memory cards, saves, controller
    /// mappings, settings) from its usual folders into romforge's
    ImportSetup {
        emulator: String,
        /// Take back the last import instead
        #[arg(long)]
        undo: bool,
    },
}
