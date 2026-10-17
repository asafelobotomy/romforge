//! Thin command-line shell over romforge-core.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use romforge_core::{
    DatState, ImportedDat, Library, LibraryFilter, ScanOptions, ScanProgress, Scope, Status, enrich, fetch,
    launch, organize, systems,
};

#[derive(Parser)]
#[command(version, about = "Catalog, identify and organize retro ROM collections")]
struct Cli {
    /// SQLite database [default: $XDG_DATA_HOME/romforge/romforge.db]
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
    },
    /// Rename and move identified files into a tidy folder (milestone 2)
    #[command(subcommand)]
    Organize(OrganizeCommand),
    /// Fetch metadata and art for the games you have (milestone 3)
    ///
    /// From libretro's database and thumbnail server; art is saved under
    /// $ROMFORGE_CACHE (default ~/.cache/romforge), never next to ROMs.
    Enrich {
        /// Art to fetch: boxart, title, snap; `none` for metadata only
        #[arg(long, value_delimiter = ',', default_value = "boxart")]
        art: Vec<String>,
        /// Only this system
        #[arg(long)]
        system: Option<String>,
        /// Fetch again what was fetched or found missing before
        #[arg(long)]
        refresh: bool,
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
        /// Arguments; {rom} is the file
        #[arg(long, allow_hyphen_values = true, num_args = 1.., default_value = "{rom}")]
        args: Vec<String>,
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
    /// Only files under this folder
    #[arg(long)]
    from: Option<PathBuf>,
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
            from: self.from.clone(),
            system: self.system.clone(),
            one_g1r: self.one_g1r,
            regions: self.regions.clone(),
            ..organize::OrganizeOptions::new(&self.dest)
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
    let db = cli.db.unwrap_or_else(romforge_core::default_db_path);

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
        Command::Enrich { art, system, refresh } => {
            let mut kinds = Vec::new();
            for a in art.iter().filter(|a| !a.eq_ignore_ascii_case("none")) {
                match enrich::ArtKind::parse(a) {
                    Some(k) => kinds.push(k),
                    None => bail!("unknown art kind {a:?} (boxart, title, snap, none)"),
                }
            }
            let opts = enrich::EnrichOptions {
                art: kinds,
                system,
                refresh,
                ..enrich::EnrichOptions::new(enrich::default_cache_dir())
            };
            let tty = std::io::stderr().is_terminal();
            let r = lib.enrich(&opts, |e| {
                if tty {
                    let line = match e {
                        enrich::EnrichEvent::Metadata { system, field } => format!("{system}: {field}"),
                        enrich::EnrichEvent::Art { system, game, kind } => {
                            format!("{system}: {} {game}", kind.as_str())
                        }
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
                args,
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
