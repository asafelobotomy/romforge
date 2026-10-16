//! Thin command-line shell over romforge-core.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use romforge_core::{
    DatState, ImportedDat, Library, LibraryFilter, ScanOptions, ScanProgress, Scope, Status, fetch, systems,
};

#[derive(Parser)]
#[command(version, about = "Scan and identify retro ROM collections (read-only)")]
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
    },
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
    /// Cartridge systems get No-Intro DATs from libretro-database's mirror;
    /// disc systems get Redump game and BIOS DATs from redump.org. Run it
    /// again to update.
    Download {
        #[arg(required_unless_present = "all")]
        systems: Vec<String>,
        /// Every system that has a DAT source
        #[arg(long, conflicts_with = "systems")]
        all: bool,
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
    let cli = Cli::parse();
    let db = cli.db.unwrap_or_else(romforge_core::default_db_path);

    if let Command::Dat(DatCommand::Systems) = cli.command {
        for s in systems::SYSTEMS {
            let mut sources = String::new();
            if !s.nointro.is_empty() {
                sources += "  [no-intro]";
            }
            if !s.redump.is_empty() {
                sources += "  [redump]";
            }
            println!("{:<24} {}{sources}", s.key, s.name);
        }
        return Ok(());
    }

    let mut lib = Library::open(&db).with_context(|| format!("opening {}", db.display()))?;
    match cli.command {
        Command::Dat(DatCommand::Import { files, system }) => {
            for file in files {
                let dats = lib
                    .import_dats(&file, system.as_deref())
                    .with_context(|| format!("importing {}", file.display()))?;
                dats.into_iter().for_each(print_imported);
            }
        }
        Command::Dat(DatCommand::Download { systems, all }) => {
            let keys: Vec<String> = if all {
                systems::SYSTEMS
                    .iter()
                    .filter(|s| !fetch::sources(s).is_empty())
                    .map(|s| s.key.to_owned())
                    .collect()
            } else {
                systems
            };
            let mut failed = 0;
            for key in keys {
                lib.download_dats(&key, |source, result| match result {
                    Ok(d) => print_imported(d),
                    Err(e) => {
                        failed += 1;
                        eprintln!("error: {key}: {}: {e}", source.url());
                    }
                })
                .with_context(|| format!("downloading DATs for {key}"))?;
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
        } => {
            let scope = match (scope.system, scope.dat) {
                (Some(key), _) => match systems::by_key(&key) {
                    Some(s) => Scope::System(s.id),
                    None => bail!("unknown system {key:?}; see `romforge-cli dat systems`"),
                },
                (None, Some(id)) => Scope::Dat(id),
                (None, None) => Scope::All,
            };
            let opts = ScanOptions { all_files };
            let mut meter = Progress::new(!quiet && std::io::stderr().is_terminal());
            let report = lib.scan_with(&dir, scope, opts, |p| meter.update(p))?;
            meter.finish();
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
        Command::Report {
            dat: Some(id),
            missing,
        } => {
            for g in lib.dat_games(id)? {
                if missing && g.have {
                    continue;
                }
                let mark = if g.have { "have" } else { "missing" };
                println!("{mark:<8} {}  ({}/{} roms)", g.name, g.roms_have, g.roms_total);
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
