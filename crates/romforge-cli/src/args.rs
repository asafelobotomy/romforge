//! The command line: `Cli` and its top-level commands.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use romforge_core::organize;

use crate::subcommands::{
    CleanupCommand, DatCommand, DatabasesCommand, EmulatorCommand, FirmwareCommand, HomeCommand,
    ManualCommand, OrganizeCommand, ScummvmCommand, SearchCommand, SteamCommand, ThemesCommand,
};

#[derive(Parser)]
#[command(version, about = "Catalog, identify and organize retro ROM collections")]
pub struct Cli {
    /// SQLite database [default: the RomForge folder's (`home create`), else
    /// $XDG_DATA_HOME/romforge/romforge.db]
    #[arg(long, global = true, env = "ROMFORGE_DB")]
    pub db: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Manage DATs
    #[command(subcommand)]
    Dat(DatCommand),
    /// Hash a folder (and zip/7z members) and identify it. Never changes files.
    ///
    /// Only files with a known ROM extension are hashed. Symlinked folders
    /// inside DIR are not followed (DIR itself may be a symlink).
    Scan {
        dir: PathBuf,
        #[command(flatten)]
        scope: ScopeArgs,
        /// Hash every file, not only known ROM extensions
        #[arg(long)]
        all_files: bool,
        /// Print only the summary: no per-file lines, no progress
        #[arg(long, short)]
        quiet: bool,
        /// Hashing threads [default: one per CPU, at most 8]; 1 for a slow
        /// spinning disk
        #[arg(long, short)]
        jobs: Option<usize>,
        /// Hash every file again, even ones unchanged since the last scan
        #[arg(long)]
        rehash: bool,
    },
    /// The RomForge folder that holds everything: games, art, databases
    /// and the library itself
    #[command(subcommand)]
    Home(HomeCommand),
    /// Move the identified games of a scanned folder into the RomForge
    /// folder's Games/, repacked one file each
    ///
    /// Shows the plan and moves nothing unless --apply. Naming a folder
    /// rescans it into the library first (hashes only; ROM files stay put).
    /// Unidentified files stay where they are. Originals go to the RomForge
    /// trash (unless "keep originals"); `cleanup undo` puts them back.
    Import {
        /// Only this folder [default: every scanned folder outside RomForge]
        dir: Option<PathBuf>,
        /// Move them now
        #[arg(long)]
        apply: bool,
        #[arg(long, short)]
        verbose: bool,
    },
    /// Apply a translation or hack you fetched (IPS, BPS, UPS, PPF, xdelta,
    /// or a zip holding one) to a game: a new copy, listed as a version of
    /// it, the original untouched
    ///
    /// Shows what it would do unless --apply. Without PATCH: the patches a
    /// scan found for the game, and where to look for one.
    Patch {
        /// The game: its title, or part of it
        game: String,
        patch: Option<PathBuf>,
        /// A hack [default: what the patch's name and readme say]
        #[arg(long, conflicts_with = "translation")]
        hack: bool,
        /// A translation [default: what the patch's name and readme say]
        #[arg(long)]
        translation: bool,
        /// What the copy is called after the game ("English v1.1") [default: the patch's name]
        #[arg(long)]
        tag: Option<String>,
        /// Write it
        #[arg(long)]
        apply: bool,
    },
    /// ScummVM games: named by ScummVM's own detection list
    #[command(subcommand)]
    Scummvm(ScummvmCommand),
    /// Rename and move identified files into a tidy folder
    #[command(subcommand)]
    Organize(OrganizeCommand),
    /// Repack identified games from the Source folder as one file each,
    /// named by the DAT, with art and details inside
    ///
    /// Cartridges become zips, discs CHDs with a sidecar folder, in the
    /// Library folder (else Source). Without "keep originals" the old files
    /// go to .romforge-trash next to them, for `cleanup undo`.
    #[command(subcommand)]
    Cleanup(CleanupCommand),
    /// Turn identified disc images into CHD where they are, under their own
    /// names (platforms whose emulators read CHD); the originals as the
    /// cleanup setting says. Shows what it would do unless --apply
    Convert {
        /// Write it
        #[arg(long)]
        apply: bool,
        /// Also list what is skipped and why
        #[arg(short, long)]
        verbose: bool,
    },
    /// Third-party game databases you download yourself:
    /// OpenVGDB and the LaunchBox Games Database. Free, but published
    /// without a licence, so romforge doesn't ship them; Enrich uses them to
    /// fill what the open sources leave empty.
    #[command(subcommand, alias = "db")]
    Databases(DatabasesCommand),
    /// Fetch metadata and art for the games you have
    ///
    /// From libretro's database and thumbnail server; art is saved under
    /// $ROMFORGE_CACHE (default ~/.cache/romforge), never next to ROMs.
    Enrich {
        /// Art to fetch: boxart, title, snap, hero, logo, icon, box3d; `none`
        /// for metadata only (hero, logo, icon and box3d need keyed sources)
        #[arg(long, value_delimiter = ',', default_value = "boxart")]
        art: Vec<String>,
        /// Only this system
        #[arg(long)]
        system: Option<String>,
        /// Fetch again what was fetched or found missing before
        #[arg(long)]
        refresh: bool,
        /// Skip Wikidata (developer, credits, series, links)
        #[arg(long)]
        no_wikidata: bool,
    },
    /// Show metadata and art for games whose name contains QUERY
    Info { query: String },
    /// Start a game (by name, part of a name, or file path) in an emulator
    ///
    /// Unlike organize/cleanup, this runs unless `--dry-run`: starting the
    /// game is the point. Without that flag romforge may also write the
    /// emulator's config keys it manages.
    Launch {
        query: String,
        /// Emulator to use (see `emulator list`), e.g. retroarch:snes9x
        #[arg(long, short)]
        emulator: Option<String>,
        /// Print the command (and config writes) instead of running it
        #[arg(long)]
        dry_run: bool,
        /// Wait until the game ends and record its play time
        #[arg(long)]
        wait: bool,
    },
    /// List, add or remove emulators
    #[command(subcommand)]
    Emulator(EmulatorCommand),
    /// BIOS files: what each system's emulators need, and what you have
    #[command(subcommand)]
    Firmware(FirmwareCommand),
    /// A game's manual (a PDF): yours, archive.org's or Vimm's Lair's scan
    #[command(subcommand)]
    Manual(ManualCommand),
    /// romforge in Steam: one shortcut (fullscreen mode), its art and a
    /// Steam Deck controller template. Steam must be closed
    #[command(subcommand)]
    Steam(SteamCommand),
    /// The fullscreen mode's themes: yours, and the community's from the
    /// romforge-themes index
    #[command(subcommand)]
    Themes(ThemesCommand),
    /// Show or change your Source / Library folders and cleanup options
    Settings {
        /// Folder of unprocessed ROMs ("" to clear)
        #[arg(long)]
        source: Option<String>,
        /// Folder processed games are sorted into ("" to clear)
        #[arg(long)]
        library: Option<String>,
        /// Keep the Source files (cleanup writes new copies into the Library)
        #[arg(long)]
        keep_originals: Option<bool>,
        /// Replace originals: rename and repack each game where it is (the
        /// original to the trash) instead of moving it
        #[arg(long)]
        in_place: Option<bool>,
        /// Scratch folder for cleanup [default: <cache>/staging] ("" for the default)
        #[arg(long)]
        staging: Option<String>,
        /// Which region's version is a game's main row, best first
        /// ("USA, Europe, Japan"; "" for the default)
        #[arg(long)]
        regions: Option<String>,
        /// Threads for long jobs (hashing, repacking, downloads): 0 for one
        /// per CPU (at most 8), 1 for a slow spinning disk
        #[arg(long)]
        threads: Option<usize>,
        /// Refresh downloads a system's online lists again once they are
        /// this many days old [default: 30; 0: never]
        #[arg(long)]
        list_age_days: Option<u32>,
        /// The app opens in its fullscreen mode (for a pad and a TV)
        #[arg(long)]
        start_fullscreen: Option<bool>,
        /// Optional online-source credentials ("" clears). Values given here
        /// end up in your shell history.
        /// SteamGridDB API key
        #[arg(long, value_name = "KEY")]
        steamgriddb_key: Option<String>,
        /// IGDB (Twitch) client id
        #[arg(long)]
        igdb_client_id: Option<String>,
        /// IGDB (Twitch) client secret
        #[arg(long)]
        igdb_client_secret: Option<String>,
        /// ScreenScraper developer id
        #[arg(long)]
        screenscraper_dev_id: Option<String>,
        /// ScreenScraper developer password
        #[arg(long)]
        screenscraper_dev_password: Option<String>,
        /// ScreenScraper user login
        #[arg(long)]
        screenscraper_user: Option<String>,
        /// ScreenScraper user password
        #[arg(long)]
        screenscraper_password: Option<String>,
    },
    /// Rescan the Source and Library folders and every folder scanned before
    ///
    /// First brings the lists games are named by up to date: ScummVM's and
    /// MAME's when the installed program changed, and the online lists
    /// (DATs, serials, clone lists) of the systems you have files for once
    /// they are `settings --list-age-days` old. Then the lists of systems
    /// this scan found first.
    Refresh {
        #[arg(long, short)]
        quiet: bool,
        /// Only rescan; leave the lists as they are
        #[arg(long)]
        no_lists: bool,
        /// Hashing threads [default: one per CPU, at most 8]; 1 for a slow
        /// spinning disk
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
        /// Hash every file again, even ones unchanged since the last scan
        #[arg(long)]
        rehash: bool,
    },
    /// List games as the app does: one row per game, lone files as their own
    Games {
        /// Only this platform (system key)
        #[arg(long)]
        platform: Option<String>,
        /// Sort by title, platform, year or status
        #[arg(long, default_value = "title")]
        sort: String,
        /// Sort the other way
        #[arg(long)]
        desc: bool,
    },
    /// Identify what hashes couldn't, and settle Wikidata matches
    #[command(subcommand)]
    Search(SearchCommand),
    /// Show file statuses and have/missing per DAT
    Report {
        /// List the games of one DAT
        #[arg(long)]
        dat: Option<i64>,
        /// With --dat: only missing games
        #[arg(long, requires = "dat")]
        missing: bool,
    },
}

#[derive(Args)]
pub struct OrganizeArgs {
    /// Destination folder; files go to DEST/<system>/...
    #[arg(long)]
    pub dest: PathBuf,
    /// Folder (or file) whose identified files move; nothing outside it does
    #[arg(long)]
    pub from: PathBuf,
    /// Only files of this system
    #[arg(long)]
    pub system: Option<String>,
    /// Keep one version per title (best region, then highest revision)
    #[arg(long = "1g1r")]
    pub one_g1r: bool,
    /// Region preference for --1g1r, best first
    #[arg(long, value_delimiter = ',', default_value = "USA,World,Europe,Japan")]
    pub regions: Vec<String>,
    /// Also list files that won't move, with the reason
    #[arg(long, short)]
    pub verbose: bool,
}

impl OrganizeArgs {
    pub fn options(&self) -> organize::OrganizeOptions {
        organize::OrganizeOptions {
            system: self.system.clone(),
            one_g1r: self.one_g1r,
            regions: self.regions.clone(),
            ..organize::OrganizeOptions::new(&self.from, &self.dest)
        }
    }
}

#[derive(Args)]
#[group(multiple = false)]
pub struct ScopeArgs {
    /// Only identify against DATs of this system
    #[arg(long)]
    pub system: Option<String>,
    /// Only identify against this DAT id
    #[arg(long)]
    pub dat: Option<i64>,
}
