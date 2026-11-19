//! `scan`, `refresh` and `report`.

use std::io::IsTerminal;

use anyhow::{Result, bail};

use romforge_core::{Library, ScanOptions, Scope, Status};

use crate::args::Command;
use crate::print::{Progress, bytes, print_dats};

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Scan {
            dir,
            scope,
            all_files,
            quiet,
            jobs,
            rehash,
            dry_run,
        } => {
            let scope = match Scope::choose(scope.system.as_deref(), scope.dat) {
                Ok(scope) => scope,
                Err(romforge_core::Error::UnknownSystem(key)) => {
                    bail!("unknown system {key:?}; see `romforge-cli dat systems`")
                }
                Err(e) => return Err(e.into()),
            };
            let opts = ScanOptions {
                all_files,
                jobs: jobs.unwrap_or(0),
                rehash,
                ..ScanOptions::default()
            };
            let mut meter = Progress::new(!quiet && std::io::stderr().is_terminal());
            if dry_run {
                let preview = lib.scan_preview_scoped(&dir, Some(scope), &opts, |p| meter.update(p))?;
                meter.finish();
                if !quiet {
                    for g in &preview.games {
                        println!(
                            "{:<13} {}  [{}]{}",
                            g.status.as_str(),
                            g.title,
                            g.system.as_deref().unwrap_or("?"),
                            if g.new { "  (new)" } else { "" }
                        );
                    }
                }
                print_problems(&preview.report);
                let mut counts: Vec<(&str, usize)> = Vec::new();
                for g in &preview.games {
                    match counts.iter_mut().find(|(s, _)| *s == g.status.as_str()) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((g.status.as_str(), 1)),
                    }
                }
                let counts: Vec<_> = counts.iter().map(|(s, n)| format!("{n} {s}")).collect();
                let new = preview.games.iter().filter(|g| g.new).count();
                println!(
                    "{}. {} games ({new} new); nothing was kept (run without --dry-run to scan)",
                    scan_line(&preview.report, !quiet, "would scan", &counts.join(", ")),
                    preview.games.len()
                );
                return Ok(());
            }
            let report = lib.scan_with(&dir, scope, &opts, |p| meter.update(p))?;
            meter.finish();
            print_problems(&report);
            let files = lib.files_under(&report.root)?;
            let root = report.root.to_string_lossy();
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
            let line = scan_line(&report, !quiet, "scanned", &counts.join(", "));
            println!("{line}");
        }
        Command::Refresh {
            quiet,
            no_lists,
            jobs,
            rehash,
        } => {
            let tty = !quiet && std::io::stderr().is_terminal();
            let mut meter = Progress::new(tty);
            let opts = ScanOptions {
                jobs: jobs.unwrap_or(0),
                rehash,
                ..ScanOptions::default()
            };
            let results = if no_lists {
                lib.refresh(&opts, |_, p| meter.update(p))?
            } else {
                let lists = lib.list_options(Default::default())?;
                let mut last = String::new();
                let (report, results) = lib.refresh_with_lists(true, &opts, &lists, |s| {
                    if tty && s.detail != last {
                        eprint!("\r\x1b[2K{}", s.detail);
                        last = s.detail;
                    }
                })?;
                if tty {
                    eprint!("\r\x1b[2K");
                }
                for u in &report.updated {
                    println!("updated {u}");
                }
                for p in &report.problems {
                    eprintln!("lists: {p}");
                }
                results
            };
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
            print_dats(lib)?;
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn print_problems(report: &romforge_core::ScanReport) {
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
}

/// "scanned 12 entries (3 MB) in /roms: 5 matched; 4 files unchanged since the last
/// scan; skipped 2 non-ROM files (.txt 2)"; `verb` is "would scan" for a
/// dry run. Symlinked folders are named on stderr unless `quiet`.
fn scan_line(report: &romforge_core::ScanReport, notes: bool, verb: &str, counts: &str) -> String {
    let root = report.root.to_string_lossy();
    let mut line = format!(
        "{verb} {} entries ({}) in {root}: {counts}",
        report.hashed,
        bytes(report.bytes_hashed)
    );
    if report.unchanged > 0 {
        line += &format!("; {} files unchanged since the last scan", report.unchanged);
    }
    if report.skipped > 0 {
        line += &format!("; skipped {} non-ROM files", report.skipped);
        if !report.skipped_kinds.is_empty() {
            line += &format!(
                " ({})",
                romforge_core::SkippedKind::summary(&report.skipped_kinds, 6)
            );
        }
    }
    if !report.skipped_links.is_empty() {
        let n = report.skipped_links.len();
        line += &format!("; skipped {n} symlinked folder{}", if n == 1 { "" } else { "s" });
        if notes {
            for link in &report.skipped_links {
                eprintln!("not followed: symlinked folder {}", link.display());
            }
        }
    }
    line
}
