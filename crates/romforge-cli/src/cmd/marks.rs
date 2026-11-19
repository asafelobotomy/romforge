//! `favourite`, `art`, `hotkeys` and `emulator pick`: what the fullscreen
//! mode and the game details let you set for a game.

use anyhow::{Context, Result, bail};

use romforge_core::enrich::{ArtKind, EnrichOptions, default_cache_dir};
use romforge_core::{Library, hotkeys};

use crate::args::Command;
use crate::subcommands::{EmulatorCommand, FavouriteCommand};

/// How a game is named in these lists.
fn shown(system: &str, title: &str) -> String {
    format!("{title}  [{system}]")
}

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Favourite(FavouriteCommand::List) => {
            let marks = lib.marks()?;
            if marks.favourites.is_empty() {
                println!("no favourites (`favourite add <game>`)");
            }
            for (system, title) in &marks.favourites {
                println!("{}", shown(system, title));
            }
            if !marks.played.is_empty() {
                println!("played, the latest first:");
                for p in &marks.played {
                    let mins = p.seconds / 60;
                    println!(
                        "  {}  {} session(s), {}h {:02}m",
                        shown(&p.system, &p.title),
                        p.sessions,
                        mins / 60,
                        mins % 60
                    );
                }
            }
        }
        Command::Favourite(c) => {
            let (game, on) = match c {
                FavouriteCommand::Add { game } => (game, true),
                FavouriteCommand::Remove { game } => (game, false),
                FavouriteCommand::List => unreachable!(),
            };
            let row = lib.find_row(&game)?;
            let Some(system) = row.system else {
                bail!("{} has no platform, so it can't be a favourite", row.title);
            };
            lib.set_favourite(&system, &row.title, on)?;
            println!(
                "{} {} favourites",
                shown(&system, &row.title),
                if on { "added to the" } else { "removed from the" }
            );
        }
        Command::Art { game, kind, pick } => {
            let kind = ArtKind::parse(&kind).with_context(|| format!("no art kind {kind:?}"))?;
            let row = lib.find_row(&game)?;
            let opts = EnrichOptions::new(default_cache_dir());
            let found = lib.art_choices(&opts, &row.id, kind)?;
            for e in &found.errors {
                eprintln!("error: {e}");
            }
            let Some(n) = pick else {
                println!(
                    "{} ({}): {} to pick from",
                    row.title,
                    kind.as_str(),
                    found.choices.len()
                );
                for (i, c) in found.choices.iter().enumerate() {
                    println!(
                        "  {:>2}  {:<13} {}{}",
                        i + 1,
                        c.source,
                        c.label.as_deref().unwrap_or(""),
                        if c.current { "  (in use)" } else { "" }
                    );
                }
                if !found.choices.is_empty() {
                    println!("`art {game:?} {} --pick N` keeps one", kind.as_str());
                }
                return Ok(());
            };
            let Some(c) = n.checked_sub(1).and_then(|i| found.choices.get(i)) else {
                bail!("no number {n}: {} picture(s) are on offer", found.choices.len());
            };
            lib.choose_art(&opts, &row.id, kind, &c.url, &c.source)?;
            println!("{} now uses {}'s {}", row.title, c.source, kind.as_str());
        }
        Command::Hotkeys { emulator } => {
            let emulators = if emulator.is_empty() {
                lib.emulators(&lib.env()?)?
                    .into_iter()
                    .filter(|e| e.found.is_some())
                    .map(|e| e.name)
                    .collect()
            } else {
                emulator
            };
            let v = hotkeys::view(emulators.iter().map(String::as_str));
            println!("{}", v.hint);
            for h in &v.hotkeys {
                println!(
                    "  {:<16} {}{}",
                    h.label,
                    h.chord.join("+"),
                    if h.hold { " (hold)" } else { "" }
                );
            }
            let ring: Vec<_> = v.radial.iter().map(|s| s.label).collect();
            println!(
                "radial menu: hold {} for {} s: {}",
                v.radial_button,
                v.radial_hold_secs,
                ring.join(", ")
            );
            for s in &v.support {
                let name = |a: &hotkeys::Action| hotkeys::label(*a);
                let actions: Vec<_> = s.actions.iter().map(name).collect();
                println!("{}: {}", s.emulator, actions.join(", "));
                if let Some(n) = s.note {
                    println!("    {n}");
                }
            }
        }
        Command::Emulator(EmulatorCommand::Pick {
            game,
            emulator,
            clear,
        }) => {
            let row = lib.find_row(&game)?;
            let Some(system) = row.system else {
                bail!("{} has no platform", row.title);
            };
            let env = lib.env()?;
            if let Some(name) = emulator {
                lib.set_game_emulator(&env, &system, &row.title, Some(&name))?;
                println!("{} now starts in {name}", shown(&system, &row.title));
            } else if clear {
                lib.set_game_emulator(&env, &system, &row.title, None)?;
                println!(
                    "{}: romforge chooses the emulator again",
                    shown(&system, &row.title)
                );
            } else {
                let (picked, choices) = lib.game_emulator(&env, &system, &row.title)?;
                println!(
                    "{}: {}",
                    shown(&system, &row.title),
                    picked
                        .as_ref()
                        .map_or("romforge's choice".to_owned(), |p| format!("always {p}"))
                );
                for c in &choices {
                    println!("  {c}");
                }
                if choices.is_empty() {
                    println!("  no installed emulator plays {system} (`emulator list --all`)");
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
