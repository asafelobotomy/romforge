//! `enrich` and `info`.

use std::io::IsTerminal;

use anyhow::{Result, bail};

use romforge_core::{Library, enrich};

use crate::args::Command;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Enrich {
            art,
            system,
            refresh,
            no_wikidata,
        } => {
            let opts = enrich::EnrichOptions::chosen(system, Some(&art), !no_wikidata, refresh)?;
            let tty = std::io::stderr().is_terminal();
            let r = lib.enrich(&opts, |e| {
                if tty {
                    let line = match e {
                        enrich::EnrichEvent::Metadata { system, field } => format!("{system}: {field}"),
                        enrich::EnrichEvent::Art { system, game, kind } => {
                            format!("{system}: {} {game}", kind.as_str())
                        }
                        enrich::EnrichEvent::Wikidata { step } => format!("Wikidata: {step}"),
                        enrich::EnrichEvent::Progress(s) if s.total > 0 => {
                            format!("[{}/{}] {}", s.done, s.total, s.detail)
                        }
                        enrich::EnrichEvent::Progress(s) => s.detail,
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
        _ => unreachable!(),
    }
    Ok(())
}
