//! `search`.

use anyhow::{Result, bail};

use romforge_core::{Library, search};

use crate::args::Command;
use crate::subcommands::SearchCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
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
        Command::Search(SearchCommand::Accept { rows }) => {
            for a in lib.search_accept(&rows)? {
                match (&a.named, &a.why_not) {
                    (Some(n), _) => println!("{}: named {n}", a.file),
                    (None, why) => println!("{}: left ({})", a.file, why.as_deref().unwrap_or("?")),
                }
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
        _ => unreachable!(),
    }
    Ok(())
}
