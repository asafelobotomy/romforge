//! Thin command-line shell over romforge-core.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use romforge_core::{
    DatState, ImportedDat, Library, LibraryFilter, ScanOptions, ScanProgress, Scope, Status, enrich, extdb,
    fetch, launch, organize, search, settings, systems,
};

#[derive(Parser)]
#[command(version, about = "Catalog, identify and organize retro ROM collections")]
struct Cli {
    /// SQLite database [default: the RomForge folder's (`home create`), else
    /// $XDG_DATA_HOME/romforge/romforge.db]
    #[arg(long, global = true, env = "ROMFORGE_DB")]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    /// and the library itself (milestone 13)
    #[command(subcommand)]
    Home(HomeCommand),
    /// Move the identified games of a scanned folder into the RomForge
    /// folder's Games/, repacked one file each (milestone 13)
    ///
    /// Shows the plan and changes nothing unless --apply. Unidentified
    /// files stay where they are. Originals go to the RomForge trash
    /// (unless "keep originals"); `cleanup undo` puts them back.
    Import {
        /// Only this folder [default: every scanned folder outside RomForge]
        dir: Option<PathBuf>,
        /// Move them now
        #[arg(long)]
        apply: bool,
        #[arg(long, short)]
        verbose: bool,
    },
    /// Rename and move identified files into a tidy folder (milestone 2)
    #[command(subcommand)]
    Organize(OrganizeCommand),
    /// Repack identified games from the Source folder as one file each,
    /// named by the DAT, with art and details inside (milestone 10)
    ///
    /// Cartridges become zips, discs CHDs with a sidecar folder, in the
    /// Library folder (else Source). Without "keep originals" the old files
    /// go to .romforge-trash next to them, for `cleanup undo`.
    #[command(subcommand)]
    Cleanup(CleanupCommand),
    /// Third-party game databases you download yourself (milestone 11):
    /// OpenVGDB and the LaunchBox Games Database. Free, but published
    /// without a licence, so romforge doesn't ship them; Enrich uses them to
    /// fill what the open sources leave empty.
    #[command(subcommand, alias = "db")]
    Databases(DatabasesCommand),
    /// Fetch metadata and art for the games you have (milestone 3)
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
    Launch {
        query: String,
        /// Emulator to use (see `emulator list`), e.g. retroarch:snes9x
        #[arg(long, short)]
        emulator: Option<String>,
        /// Print the command instead of running it
        #[arg(long)]
        dry_run: bool,
    },
    /// List, add or remove emulators
    #[command(subcommand)]
    Emulator(EmulatorCommand),
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
        /// Scratch folder for cleanup [default: <cache>/staging] ("" for the default)
        #[arg(long)]
        staging: Option<String>,
        /// Which region's version is a game's main row, best first
        /// ("USA, Europe, Japan"; "" for the default)
        #[arg(long)]
        regions: Option<String>,
        /// Optional online-source credentials ("" clears). Values given here
        /// end up in your shell history.
        #[arg(long, value_name = "KEY")]
        steamgriddb_key: Option<String>,
        #[arg(long)]
        igdb_client_id: Option<String>,
        #[arg(long)]
        igdb_client_secret: Option<String>,
        #[arg(long)]
        screenscraper_dev_id: Option<String>,
        #[arg(long)]
        screenscraper_dev_password: Option<String>,
        #[arg(long)]
        screenscraper_user: Option<String>,
        #[arg(long)]
        screenscraper_password: Option<String>,
    },
    /// Rescan the Source and Library folders and every folder scanned before
    Refresh {
        #[arg(long, short)]
        quiet: bool,
        /// Hash every file again, even ones unchanged since the last scan
        #[arg(long)]
        rehash: bool,
    },
    /// List games as the app does: one row per game, lone files as their own
    Games {
        /// Only this platform (system key)
        #[arg(long)]
        platform: Option<String>,
    },
    /// Identify what hashes couldn't, and settle Wikidata matches (milestone 8)
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

#[derive(Subcommand)]
enum DatCommand {
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
enum OrganizeCommand {
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
enum DatabasesCommand {
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
enum HomeCommand {
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
enum CleanupCommand {
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
}

#[derive(Subcommand)]
enum SearchCommand {
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
enum EmulatorCommand {
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
    /// Remove an emulator you added
    Remove { name: String },
}

#[derive(Args)]
struct OrganizeArgs {
    /// Destination folder; files go to DEST/<system>/...
    #[arg(long)]
    dest: PathBuf,
    /// Folder (or file) whose identified files move; nothing outside it does
    #[arg(long)]
    from: PathBuf,
    /// Only files of this system
    #[arg(long)]
    system: Option<String>,
    /// Keep one version per title (best region, then highest revision)
    #[arg(long = "1g1r")]
    one_g1r: bool,
    /// Region preference for --1g1r, best first
    #[arg(long, value_delimiter = ',', default_value = "USA,World,Europe,Japan")]
    regions: Vec<String>,
    /// Also list files that won't move, with the reason
    #[arg(long, short)]
    verbose: bool,
}

impl OrganizeArgs {
    fn options(&self) -> organize::OrganizeOptions {
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
struct ScopeArgs {
    /// Only identify against DATs of this system
    #[arg(long)]
    system: Option<String>,
    /// Only identify against this DAT id
    #[arg(long)]
    dat: Option<i64>,
}

fn main() -> Result<()> {
    // Like other command-line tools, end quietly when the reader of our
    // output goes away (`romforge-cli report | head`) instead of panicking.
    #[cfg(unix)]
    // SAFETY: restoring the default action of a signal before any thread
    // starts.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let home = romforge_core::home::Home::locate();
    let db = cli
        .db
        .clone()
        .or_else(|| home.as_ref().map(|h| h.db()))
        .unwrap_or_else(romforge_core::default_db_path);

    match &cli.command {
        Command::Home(HomeCommand::Create { parent }) => {
            if let Some(old) = &home
                && cli.db.is_none()
                && !std::path::absolute(parent)?.starts_with(old.root())
            {
                bail!(
                    "already using {}; to move it, move that folder and run `home create` on its new place",
                    old.root().display()
                );
            }
            let new = romforge_core::home::Home::create(parent)?;
            if new.db().exists() {
                println!("{} is already a RomForge folder", new.root().display());
            } else if db.exists() && home.as_ref() != Some(&new) {
                let art = enrich::default_cache_dir().join("art");
                let a = romforge_core::home::adopt(&new, &db, &art)
                    .with_context(|| format!("moving {} into {}", db.display(), new.root().display()))?;
                println!(
                    "moved the library into {}: database {}, {} downloaded databases, {} images",
                    new.root().display(),
                    if a.database { "yes" } else { "no" },
                    a.databases,
                    a.art
                );
            }
            new.remember()?;
            println!("using {}", new.root().display());
            return Ok(());
        }
        Command::Home(HomeCommand::Move { new_parent }) => {
            let Some(old) = &home else {
                bail!("no RomForge folder yet (`romforge-cli home create <parent>`)");
            };
            let new = romforge_core::home::relocate(old, new_parent)?;
            // Opening it once fixes the paths it stores.
            Library::open(new.db())?;
            println!("moved to {}", new.root().display());
            return Ok(());
        }
        Command::Home(HomeCommand::Show) => {
            match &home {
                Some(h) => println!("{}", h.root().display()),
                None => println!("no RomForge folder yet (`romforge-cli home create <parent>`)"),
            }
            return Ok(());
        }
        _ => {}
    }

    if let Command::Dat(DatCommand::Systems) = cli.command {
        for s in systems::SYSTEMS {
            let mut sources = String::new();
            for (has, tag) in [
                (!s.nointro.is_empty(), "no-intro"),
                (!s.redump.is_empty(), "redump"),
                (!s.tosec.is_empty(), "tosec"),
                (!s.hacks.is_empty(), "hacks"),
                (!s.homebrew.is_empty(), "homebrew"),
            ] {
                if has {
                    sources += &format!("  [{tag}]");
                }
            }
            println!("{:<24} {}{sources}", s.key, s.name);
        }
        return Ok(());
    }

    let mut lib = Library::open(&db).with_context(|| format!("opening {}", db.display()))?;
    match cli.command {
        Command::Dat(DatCommand::Import { files, system }) => {
            lib.batch(|lib| -> Result<()> {
                for file in files {
                    let dats = lib
                        .import_dats(&file, system.as_deref())
                        .with_context(|| format!("importing {}", file.display()))?;
                    dats.into_iter().for_each(print_imported);
                }
                Ok(())
            })?;
        }
        Command::Dat(DatCommand::Download { systems, all, extras }) => {
            let mut failed = 0;
            let mut report =
                |key: &str, source: fetch::Source, result: Result<Vec<ImportedDat>, romforge_core::Error>| {
                    match result {
                        Ok(_)
                            if matches!(source, fetch::Source::Serials(_) | fetch::Source::CloneList(..)) =>
                        {
                            println!("{key}: {source}: stored")
                        }
                        Ok(dats) if dats.is_empty() => println!("{key}: {source}: already imported"),
                        Ok(dats) => dats.into_iter().for_each(print_imported),
                        Err(e) => {
                            failed += 1;
                            eprintln!("error: {key}: {source}: {e}");
                        }
                    }
                };
            if all {
                lib.download_all(extras, &mut report)?;
            } else {
                lib.batch(|lib| -> Result<()> {
                    for key in &systems {
                        lib.download_dats(key, extras, |source, r| report(key, source, r))
                            .with_context(|| format!("downloading DATs for {key}"))?;
                    }
                    Ok(())
                })?;
            }
            if failed > 0 {
                bail!("{failed} DAT downloads failed");
            }
        }
        Command::Dat(DatCommand::Check { all }) => {
            let profile = fetch::nointro_profile()?;
            let status = lib.nointro_status(&profile)?;
            let mut missing = 0;
            for s in &status {
                let line = match s.state {
                    DatState::Missing => {
                        missing += 1;
                        if !all {
                            continue;
                        }
                        format!("missing   {:<60} latest {}", s.name, s.latest)
                    }
                    DatState::Current => format!("current   {:<60} {}", s.name, s.latest),
                    DatState::Mirror => format!(
                        "mirror    {:<60} libretro {} (No-Intro {})",
                        s.name,
                        s.imported.as_deref().unwrap_or("?"),
                        s.latest
                    ),
                    DatState::Outdated => format!(
                        "outdated  {:<60} have {} latest {}",
                        s.name,
                        s.imported.as_deref().unwrap_or("?"),
                        s.latest
                    ),
                };
                println!("{line}");
            }
            if missing > 0 && !all {
                println!("{missing} No-Intro DATs for known systems not imported (--all lists them)");
            }
            println!(
                "`dat download` refreshes mirror DATs; others come from {}",
                fetch::NOINTRO_DOWNLOAD_PAGE
            );
        }
        Command::Dat(DatCommand::List) => print_dats(&lib)?,
        Command::Dat(DatCommand::Systems) => unreachable!(),
        Command::Scan {
            dir,
            scope,
            all_files,
            quiet,
            jobs,
            rehash,
        } => {
            let scope = match (scope.system, scope.dat) {
                (Some(key), _) => match systems::by_key(&key) {
                    Some(s) => Scope::System(s.id),
                    None => bail!("unknown system {key:?}; see `romforge-cli dat systems`"),
                },
                (None, Some(id)) => Scope::Dat(id),
                (None, None) => Scope::All,
            };
            let opts = ScanOptions {
                all_files,
                jobs: jobs.unwrap_or(0),
                rehash,
            };
            let mut meter = Progress::new(!quiet && std::io::stderr().is_terminal());
            let report = lib.scan_with(&dir, scope, opts, |p| meter.update(p))?;
            meter.finish();
            for (sheet, problem) in &report.sheet_problems {
                eprintln!("sheet: {}: {problem}", sheet.display());
            }
            for (path, member, err) in &report.errors {
                let at = if member.is_empty() {
                    String::new()
                } else {
                    format!("#{member}")
                };
                eprintln!("error: {}{at}: {err}", path.display());
            }
            let root = report.root.to_string_lossy();
            let files = lib.files(&LibraryFilter::default())?;
            let files: Vec<_> = files
                .iter()
                .filter(|f| f.path.starts_with(root.as_ref()))
                .collect();
            if !quiet {
                for f in &files {
                    let shown = f.display_path();
                    let shown = shown
                        .strip_prefix(root.as_ref())
                        .unwrap_or(&shown)
                        .trim_start_matches('/');
                    let games = if f.games.is_empty() {
                        String::new()
                    } else if f.games.len() > 3 {
                        // e.g. a short audio track shared by hundreds of discs
                        format!("  {} | … (+{} more)", f.games[..3].join(" | "), f.games.len() - 3)
                    } else {
                        format!("  {}", f.games.join(" | "))
                    };
                    let header = f
                        .header_kind
                        .as_deref()
                        .map(|h| format!(" [{h}]"))
                        .unwrap_or_default();
                    println!("{:<13} {shown}{header}{games}", f.status.as_str());
                }
            }
            let mut counts = Status::ALL.map(|s| (s, 0));
            for f in &files {
                counts.iter_mut().find(|(s, _)| *s == f.status).unwrap().1 += 1;
            }
            let counts: Vec<_> = counts
                .iter()
                .map(|(s, n)| format!("{n} {}", s.as_str()))
                .collect();
            let mut line = format!(
                "scanned {} entries ({}) in {root}: {}",
                report.hashed,
                bytes(report.bytes_hashed),
                counts.join(", ")
            );
            if report.unchanged > 0 {
                line += &format!("; {} files unchanged since the last scan", report.unchanged);
            }
            if report.skipped > 0 {
                line += &format!("; skipped {} non-ROM files", report.skipped);
            }
            if !report.skipped_links.is_empty() {
                let n = report.skipped_links.len();
                line += &format!("; skipped {n} symlinked folder{}", if n == 1 { "" } else { "s" });
            }
            println!("{line}");
            if !quiet {
                for link in &report.skipped_links {
                    eprintln!("not followed: symlinked folder {}", link.display());
                }
            }
        }
        Command::Organize(OrganizeCommand::Plan(args)) => {
            let plan = lib.organize_plan(&args.options())?;
            print_plan(&plan, args.verbose);
            println!(
                "{} to move, {} staying; nothing changed (run `organize apply` to move)",
                plan.moves.len(),
                plan.skipped.len()
            );
        }
        Command::Organize(OrganizeCommand::Apply(args)) => {
            let plan = lib.organize_plan(&args.options())?;
            print_plan(&plan, args.verbose);
            if plan.moves.is_empty() {
                println!(
                    "nothing to move ({} files stay; -v shows why)",
                    plan.skipped.len()
                );
                return Ok(());
            }
            let applied = lib.organize_apply(&plan, &args.dest)?;
            for (m, e) in &applied.failed {
                eprintln!("error: {}: {e}", m.from.display());
            }
            println!(
                "batch #{}: moved {}, failed {} (undo with `organize undo --batch {}`)",
                applied.batch_id,
                applied.moved,
                applied.failed.len(),
                applied.batch_id
            );
            if !applied.failed.is_empty() {
                bail!("{} moves failed", applied.failed.len());
            }
        }
        Command::Organize(OrganizeCommand::Undo { batch }) => {
            let undone = lib.organize_undo(batch)?;
            for (p, e) in &undone.failed {
                eprintln!("error: {}: {e}", p.display());
            }
            println!(
                "batch #{}: restored {}, failed {}",
                undone.batch_id,
                undone.restored,
                undone.failed.len()
            );
            if !undone.failed.is_empty() {
                bail!("{} files could not be moved back", undone.failed.len());
            }
        }
        Command::Databases(cmd) => {
            let dir = lib.extdb_dir().context("no folder for databases")?;
            let pick = |n: &str| {
                extdb::ExtDb::parse(n)
                    .with_context(|| format!("unknown database {n:?} (openvgdb or launchbox)"))
            };
            match cmd {
                DatabasesCommand::List => {
                    for i in extdb::list(&dir) {
                        let state = match (&i.downloaded_at, i.bytes) {
                            (Some(at), Some(b)) => {
                                format!(
                                    "downloaded {at}, {} MB, {} entries",
                                    b >> 20,
                                    i.entries.unwrap_or(0)
                                )
                            }
                            _ => "not downloaded".into(),
                        };
                        println!("{:<10} {:<26} {state}", i.db.key(), i.name);
                        println!("           {}", i.about);
                    }
                    println!("(in {})", dir.display());
                }
                DatabasesCommand::Download { name } => {
                    let db = pick(&name)?;
                    let i = extdb::download(db, &dir, &extdb::Bases::default(), |m| eprintln!("{m}"))?;
                    println!("{}: {} entries", i.name, i.entries.unwrap_or(0));
                }
                DatabasesCommand::Import { name, zip } => {
                    let db = pick(&name)?;
                    let i = extdb::import(db, &zip, &dir, &zip.to_string_lossy(), |m| eprintln!("{m}"))?;
                    println!("{}: {} entries", i.name, i.entries.unwrap_or(0));
                }
                DatabasesCommand::Remove { name } => {
                    let db = pick(&name)?;
                    extdb::remove(db, &dir)?;
                    println!("{} removed", db.name());
                }
            }
        }
        Command::Home(_) => unreachable!("handled before the library opens"),
        Command::Import { dir, apply, verbose } => {
            if lib.home().is_none() {
                bail!("no RomForge folder yet: `romforge-cli home create <parent>` first");
            }
            if let Some(d) = &dir {
                lib.scan(d, Scope::All)?;
            }
            let plan = lib.import_plan(dir.as_deref())?;
            print_cleanup(&plan, verbose);
            if !apply {
                println!(
                    "{} to import into {}, {} skipped; nothing changed (run with --apply to move them)",
                    plan.items.len(),
                    plan.dest.display(),
                    plan.skipped.len()
                );
                return Ok(());
            }
            if plan.items.is_empty() {
                println!("nothing to import ({} skipped)", plan.skipped.len());
                return Ok(());
            }
            let total = plan.items.len();
            let r = lib.cleanup_apply(&plan, |n, title| eprintln!("[{}/{total}] {title}", n + 1))?;
            for (t, e) in &r.failed {
                eprintln!("error: {t}: {e}");
            }
            println!(
                "batch #{}: imported {}, trashed {} originals, failed {} (undo with `cleanup undo --batch {}`)",
                r.batch_id,
                r.done,
                r.trashed,
                r.failed.len(),
                r.batch_id
            );
            if !r.failed.is_empty() {
                bail!("{} games failed", r.failed.len());
            }
        }
        Command::Cleanup(CleanupCommand::Plan { verbose }) => {
            let plan = lib.cleanup_plan(None)?;
            print_cleanup(&plan, verbose);
            println!(
                "{} to write, {} skipped; nothing changed (run `cleanup apply` to write)",
                plan.items.len(),
                plan.skipped.len()
            );
        }
        Command::Cleanup(CleanupCommand::Apply { verbose }) => {
            let plan = lib.cleanup_plan(None)?;
            print_cleanup(&plan, verbose);
            if plan.items.is_empty() {
                println!("nothing to write ({} skipped)", plan.skipped.len());
                return Ok(());
            }
            let total = plan.items.len();
            let r = lib.cleanup_apply(&plan, |n, title| {
                eprintln!("[{}/{total}] {title}", n + 1);
            })?;
            for (t, e) in &r.failed {
                eprintln!("error: {t}: {e}");
            }
            println!(
                "batch #{}: wrote {}, trashed {} originals, failed {} (undo with `cleanup undo --batch {}`)",
                r.batch_id,
                r.done,
                r.trashed,
                r.failed.len(),
                r.batch_id
            );
            if !r.failed.is_empty() {
                bail!("{} games failed", r.failed.len());
            }
        }
        Command::Cleanup(CleanupCommand::Undo { batch }) => {
            let u = lib.cleanup_undo(batch)?;
            for e in &u.failed {
                eprintln!("error: {e}");
            }
            println!(
                "batch #{}: removed {}, restored {}, failed {}",
                u.batch_id,
                u.removed,
                u.restored,
                u.failed.len()
            );
            if !u.failed.is_empty() {
                bail!("{} steps could not be undone", u.failed.len());
            }
        }
        Command::Cleanup(CleanupCommand::History) => {
            for b in lib.cleanup_batches()? {
                let state = match &b.undone_at {
                    Some(t) => format!("undone {t}"),
                    None => "applied".into(),
                };
                let keep = if b.keep_originals {
                    "kept originals"
                } else {
                    "trashed originals"
                };
                println!(
                    "#{:<4} {}  {:>5} written  {:>5} trashed  {keep}  {state}",
                    b.batch_id, b.created_at, b.outputs, b.trashed
                );
            }
        }
        Command::Organize(OrganizeCommand::History) => {
            for b in lib.organize_batches()? {
                let state = match &b.undone_at {
                    Some(t) => format!("undone {t}"),
                    None => "applied".into(),
                };
                println!(
                    "#{:<4} {}  {:>5} moves  {state}  -> {}",
                    b.batch_id, b.created_at, b.moves, b.dest
                );
            }
        }
        Command::Enrich {
            art,
            system,
            refresh,
            no_wikidata,
        } => {
            let mut kinds = Vec::new();
            for a in art.iter().filter(|a| !a.eq_ignore_ascii_case("none")) {
                match enrich::ArtKind::parse(a) {
                    Some(k) => kinds.push(k),
                    None => {
                        bail!("unknown art kind {a:?} (boxart, title, snap, hero, logo, icon, box3d, none)")
                    }
                }
            }
            let opts = enrich::EnrichOptions {
                art: kinds,
                system,
                refresh,
                ..enrich::EnrichOptions::new(enrich::default_cache_dir())
            };
            let opts = enrich::EnrichOptions {
                wikidata: if no_wikidata { None } else { opts.wikidata.clone() },
                ..opts
            };
            let tty = std::io::stderr().is_terminal();
            let r = lib.enrich(&opts, |e| {
                if tty {
                    let line = match e {
                        enrich::EnrichEvent::Metadata { system, field } => format!("{system}: {field}"),
                        enrich::EnrichEvent::Art { system, game, kind } => {
                            format!("{system}: {} {game}", kind.as_str())
                        }
                        enrich::EnrichEvent::Wikidata { step } => format!("Wikidata: {step}"),
                    };
                    eprint!("\r\x1b[2K{line}");
                }
            })?;
            if tty {
                eprint!("\r\x1b[2K");
            }
            for e in &r.errors {
                eprintln!("error: {e}");
            }
            println!(
                "{} games in {} systems: {} with metadata; art {} saved, {} not found upstream, {} already cached (in {})",
                r.games,
                r.systems,
                r.games_with_metadata,
                r.art_saved,
                r.art_missing,
                r.art_cached,
                opts.cache_dir.display()
            );
            if let Some(w) = &r.wikidata {
                println!(
                    "Wikidata: {} matched, {} ambiguous (settle them in Search), {} not found",
                    w.matched, w.ambiguous, w.unmatched
                );
                for e in &w.errors {
                    eprintln!("error: Wikidata: {e}");
                }
            }
        }
        Command::Info { query } => {
            let found = lib.game_info(&query)?;
            if found.is_empty() {
                bail!("no metadata or art for {query:?}; run `romforge-cli enrich` first");
            }
            for g in found {
                println!("{}  [{}]", g.title, g.system);
                for (k, v) in &g.fields {
                    println!("  {k:<12} {v}");
                }
                for (k, p) in &g.art {
                    println!("  {:<12} {}", k.as_str(), p.display());
                }
            }
        }
        Command::Launch {
            query,
            emulator,
            dry_run,
        } => {
            let spec = lib.launch_plan(&launch::Env::detect(), &query, emulator.as_deref())?;
            let shown: Vec<String> = std::iter::once(spec.program.display().to_string())
                .chain(spec.args.iter().map(|a| {
                    if a.contains(' ') {
                        format!("'{a}'")
                    } else {
                        a.clone()
                    }
                }))
                .collect();
            println!(
                "{} [{}] with {}: {}",
                spec.game,
                spec.system,
                spec.emulator,
                shown.join(" ")
            );
            if !dry_run {
                let pid = launch::spawn(&spec)?;
                println!("started (pid {pid})");
            }
        }
        Command::Emulator(EmulatorCommand::List { all }) => {
            for e in lib.emulators(&launch::Env::detect())? {
                match (&e.found, all) {
                    (Some(how), _) => println!("{:<28} {:<40} {how}", e.name, e.systems.join(", ")),
                    (None, true) => println!("{:<28} {:<40} -", e.name, e.systems.join(", ")),
                    (None, false) => {}
                }
            }
        }
        Command::Emulator(EmulatorCommand::Add {
            name,
            system,
            program,
            args,
            archives,
        }) => {
            lib.add_emulator(&launch::UserEmulator {
                name: name.clone(),
                system,
                program,
                args: args.split_whitespace().map(str::to_owned).collect(),
                archives,
            })?;
            println!("added {name}");
        }
        Command::Emulator(EmulatorCommand::Remove { name }) => {
            let n = lib.remove_emulator(&name)?;
            if n == 0 {
                bail!("no emulator named {name:?} (only ones you added can be removed)");
            }
            println!("removed {name}");
        }
        Command::Settings {
            source,
            library,
            keep_originals,
            staging,
            regions,
            steamgriddb_key,
            igdb_client_id,
            igdb_client_secret,
            screenscraper_dev_id,
            screenscraper_dev_password,
            screenscraper_user,
            screenscraper_password,
        } => {
            let mut s = lib.settings()?;
            let mut changed = source.is_some()
                || library.is_some()
                || keep_originals.is_some()
                || staging.is_some()
                || regions.is_some();
            for (given, slot) in [
                (steamgriddb_key, &mut s.keys.steamgriddb_key),
                (igdb_client_id, &mut s.keys.igdb_client_id),
                (igdb_client_secret, &mut s.keys.igdb_client_secret),
                (screenscraper_dev_id, &mut s.keys.screenscraper_dev_id),
                (screenscraper_dev_password, &mut s.keys.screenscraper_dev_password),
                (screenscraper_user, &mut s.keys.screenscraper_user),
                (screenscraper_password, &mut s.keys.screenscraper_password),
            ] {
                if let Some(v) = given {
                    *slot = Some(v);
                    changed = true;
                }
            }
            let set = |p: String| (!p.is_empty()).then(|| PathBuf::from(p));
            if let Some(p) = source {
                s.source_dir = set(p);
            }
            if let Some(p) = library {
                s.library_dir = set(p);
            }
            if let Some(k) = keep_originals {
                s.keep_originals = k;
            }
            if let Some(p) = staging {
                s.staging_dir = set(p);
            }
            if let Some(r) = regions {
                s.regions = r
                    .split(',')
                    .map(|r| r.trim().to_owned())
                    .filter(|r| !r.is_empty())
                    .collect();
            }
            if changed {
                s = lib.save_settings(&s)?;
            }
            let shown = |p: &Option<PathBuf>| p.as_ref().map_or("-".to_owned(), |p| p.display().to_string());
            println!("source          {}", shown(&s.source_dir));
            println!("library         {}", shown(&s.library_dir));
            println!("keep originals  {}", if s.keep_originals { "yes" } else { "no" });
            println!("staging         {}", s.staging().display());
            println!("regions         {}", s.region_order().join(", "));
            let key = |v: &Option<String>| v.as_deref().map_or("-".to_owned(), settings::mask);
            println!("steamgriddb     {}", key(&s.keys.steamgriddb_key));
            println!(
                "igdb            {} / {}",
                key(&s.keys.igdb_client_id),
                key(&s.keys.igdb_client_secret)
            );
            println!(
                "screenscraper   dev {} / {}, user {}",
                key(&s.keys.screenscraper_dev_id),
                key(&s.keys.screenscraper_dev_password),
                s.keys.screenscraper_user.as_deref().unwrap_or("-")
            );
        }
        Command::Refresh { quiet, rehash } => {
            let mut meter = Progress::new(!quiet && std::io::stderr().is_terminal());
            let opts = ScanOptions {
                rehash,
                ..ScanOptions::default()
            };
            let results = lib.refresh(opts, |_, p| meter.update(p))?;
            meter.finish();
            let mut failed = 0;
            for r in results {
                match r {
                    Ok(report) => println!(
                        "rescanned {}: {} entries hashed, {} files unchanged",
                        report.root.display(),
                        report.hashed,
                        report.unchanged
                    ),
                    Err((path, e)) => {
                        failed += 1;
                        eprintln!("error: {}: {e}", path.display());
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} folders could not be rescanned");
            }
        }
        Command::Search(SearchCommand::Queue) => {
            let q = lib.search_queue(None)?;
            for item in &q {
                let need = match item.need {
                    romforge_core::search::Need::Identify => "identify".to_owned(),
                    romforge_core::search::Need::Wikidata => {
                        format!("wikidata ({})", item.wikidata.as_deref().unwrap_or("?"))
                    }
                };
                println!(
                    "{:<20} {:<18} {:<16} {}",
                    item.row_id,
                    need,
                    item.system.as_deref().unwrap_or("-"),
                    item.title
                );
                if let Some(s) = &item.suggestion {
                    println!("{:<20} {}: {} [{}]", "", s.band.label(), s.title, s.system);
                }
            }
            println!("{} need you", q.len());
        }
        Command::Search(SearchCommand::Find {
            row,
            text,
            scraper,
            system,
        }) => {
            let scraper = match scraper.as_str() {
                "dat" => search::Scraper::Dat,
                "wikidata" => search::Scraper::Wikidata,
                s => bail!("unknown scraper {s:?} (dat or wikidata)"),
            };
            let found = lib.search_find(
                &row,
                scraper,
                text.as_deref(),
                system.as_deref(),
                search::WIKIDATA_API,
            )?;
            for f in found {
                let year = f.year.map_or("    ".into(), |y| y.to_string());
                let id = if scraper == search::Scraper::Wikidata {
                    f.id.as_str()
                } else {
                    ""
                };
                println!(
                    "{:>4.0}%  {year}  {:<16} {:<10} {}{}",
                    f.score * 100.0,
                    f.system.as_deref().unwrap_or(""),
                    id,
                    f.title,
                    f.description.map(|d| format!("  ({d})")).unwrap_or_default()
                );
                for e in &f.evidence {
                    println!("{:>33} {}", if e.agrees { "+" } else { "-" }, e.text);
                }
            }
        }
        Command::Search(SearchCommand::Name {
            row,
            system,
            title,
            translation_of,
            hack_of,
            tag,
        }) => {
            use romforge_core::view::{Derived, Relation};
            let derived = translation_of
                .map(|b| (Relation::Translation, b))
                .or(hack_of.map(|b| (Relation::Hack, b)))
                .map(|(relation, base)| Derived { relation, base, tag });
            match (derived, title) {
                (Some(d), _) => {
                    lib.identify_derived(&row, &system, &d)?;
                    println!("{} [{system}]: {}, identified by you", d.title(), d.label());
                }
                (None, Some(title)) => {
                    lib.identify_as(&row, &system, &title)?;
                    println!("{title} [{system}], identified by you");
                }
                (None, None) => bail!("give --title, --translation-of or --hack-of"),
            }
        }
        Command::Search(SearchCommand::Forget { row }) => {
            lib.forget_identity(&row)?;
            println!("forgotten");
        }
        Command::Search(SearchCommand::Wikidata { row, qid }) => {
            lib.settle_wikidata(romforge_core::wikidata::ENDPOINT, &row, &qid)?;
            println!("matched to {qid}");
        }
        Command::Games { platform } => {
            let v = lib.library_view()?;
            for r in v.rows.iter().filter(|r| {
                platform
                    .as_deref()
                    .is_none_or(|p| r.system.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(p)))
            }) {
                let gathered = format!("{}{}", if r.metadata { "info " } else { "" }, r.art.join(" "));
                println!(
                    "{:<11} {:<16} {}  [{}]",
                    format!("{:?}", r.status).to_lowercase(),
                    r.system.as_deref().unwrap_or("-"),
                    r.title,
                    gathered.trim()
                );
            }
            let n: Vec<String> = v
                .platforms
                .iter()
                .map(|p| format!("{} {}", p.games, p.name))
                .collect();
            println!("{} rows; {}", v.rows.len(), n.join(", "));
        }
        Command::Report {
            dat: Some(id),
            missing,
        } => {
            for g in lib.dat_games(id)? {
                if missing && g.have {
                    continue;
                }
                let mark = if g.have { "have" } else { "missing" };
                let note = if g.cue_differs { ", cue differs" } else { "" };
                println!(
                    "{mark:<8} {}  ({}/{} roms{note})",
                    g.name, g.roms_have, g.roms_total
                );
                for f in &g.files {
                    println!("         <- {f}");
                }
            }
        }
        Command::Report { dat: None, .. } => {
            let s = lib.summary()?;
            let counts: Vec<_> = s
                .files_by_status
                .iter()
                .map(|(st, n)| format!("{n} {}", st.as_str()))
                .collect();
            println!("files: {}", counts.join(", "));
            print_dats(&lib)?;
        }
    }
    Ok(())
}

fn print_cleanup(plan: &romforge_core::cleanup::CleanupPlan, verbose: bool) {
    let what = if plan.keep_originals {
        "originals kept"
    } else {
        "originals to .romforge-trash"
    };
    println!("into {} ({what})", plan.dest.display());
    for it in &plan.items {
        if it.existing {
            println!(
                "  {}  already there; {} copy file(s) outside go to the trash",
                it.target.display(),
                it.trash.len()
            );
        } else {
            println!("  {}  <-  {} file(s)", it.target.display(), it.parts.len());
        }
        if verbose {
            let from = |p: &romforge_core::cleanup::Part| {
                if p.member.is_empty() {
                    p.path.display().to_string()
                } else {
                    format!("{}#{}", p.path.display(), p.member)
                }
            };
            for p in &it.parts {
                println!("      {}  <-  {}", p.name, from(p));
            }
            if let Some(c) = &it.sheet {
                println!("      cue sheet (pregaps, if it names these tracks): {}", from(c));
            }
            for t in &it.trash {
                println!("      trash: {}", t.display());
            }
            for k in &it.kept {
                println!("      stays (holds other files): {}", k.display());
            }
        }
    }
    for (t, why) in &plan.skipped {
        println!("  skip {t}: {why}");
    }
}

fn print_plan(plan: &organize::Plan, verbose: bool) {
    for m in &plan.moves {
        println!("move  {}\n   -> {}", m.from.display(), m.to.display());
    }
    if verbose {
        for s in &plan.skipped {
            println!("stay  {}  ({})", s.path.display(), s.reason);
        }
    }
}

fn print_imported(d: ImportedDat) {
    let system = d.system.unwrap_or("no system (pass --system to set one)");
    let note = match (d.unchanged, d.replaced) {
        (true, _) => " (already imported)".to_owned(),
        (false, Some(old)) => format!(" (replaces DAT #{old})"),
        _ => String::new(),
    };
    println!(
        "DAT #{}: {} [{}] {} games, {} roms{}{}",
        d.dat_id,
        d.name,
        system,
        d.games,
        d.roms,
        d.header_hint.map(|h| format!(", header {h}")).unwrap_or_default(),
        note
    );
}

fn print_dats(lib: &Library) -> Result<()> {
    for d in lib.summary()?.dats {
        println!(
            "#{:<3} {:<55} {:<16} have {:>5} / {:<5} missing {:<5} imported {} from {}",
            d.dat_id,
            d.name,
            d.system.unwrap_or_else(|| "-".into()),
            d.have,
            d.games,
            d.missing,
            d.imported_at,
            d.source_path
        );
    }
    Ok(())
}

fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// A one-line progress meter on stderr, redrawn at most every 200 ms.
struct Progress {
    enabled: bool,
    last: Option<Instant>,
    drawn: bool,
}

impl Progress {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last: None,
            drawn: false,
        }
    }

    fn update(&mut self, p: ScanProgress) {
        if !self.enabled
            || self
                .last
                .is_some_and(|t| t.elapsed() < Duration::from_millis(200))
        {
            return;
        }
        self.last = Some(Instant::now());
        self.drawn = true;
        let mut err = std::io::stderr().lock();
        let _ = write!(
            err,
            "\r\x1b[2Khashed {} files, {}; skipped {}",
            p.hashed,
            bytes(p.bytes_hashed),
            p.skipped
        );
        let _ = err.flush();
    }

    fn finish(&mut self) {
        if self.drawn {
            eprint!("\r\x1b[2K");
        }
    }
}
