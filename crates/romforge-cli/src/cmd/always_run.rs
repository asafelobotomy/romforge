//! `always-run`: the version Play starts for a game with several, as the
//! app's Play box keeps it with "Always run".

use anyhow::{Result, bail};

use romforge_core::Library;

use crate::subcommands::AlwaysRunCommand;

pub fn run(lib: &Library, command: AlwaysRunCommand) -> Result<()> {
    match command {
        AlwaysRunCommand::List => {
            let all = lib.always_run_list()?;
            if all.is_empty() {
                println!("Play asks which version for every game (`always-run set <game> <version>`)");
            }
            for a in &all {
                println!("{}  [{}]", a.title, a.system);
            }
        }
        AlwaysRunCommand::Show { game } => {
            let row = lib.find_row(&game)?;
            let versions = lib.playable_versions(&row.id)?;
            if versions.len() < 2 {
                println!("{} has no other playable version", row.title);
                return Ok(());
            }
            let always = lib.always_run(&row.id)?;
            for (i, v) in versions.iter().enumerate() {
                let mark = if always.as_ref().is_some_and(|a| a.id == v.id) {
                    "  (always run)"
                } else {
                    ""
                };
                println!("  {:>2}  {}{mark}", i + 1, v.title);
            }
            if always.is_none() {
                println!("Play asks which one");
            }
        }
        AlwaysRunCommand::Set { game, version } => {
            let row = lib.find_row(&game)?;
            let versions = lib.playable_versions(&row.id)?;
            let picked = match version.parse::<usize>() {
                Ok(n) => n.checked_sub(1).and_then(|i| versions.get(i)),
                Err(_) => versions.iter().find(|v| v.title == version || v.id == version),
            };
            let Some(v) = picked else {
                bail!(
                    "no version {version:?} of {} ({} playable: `always-run show {game:?}`)",
                    row.title,
                    versions.len()
                );
            };
            lib.set_always_run(&row.id, Some(&v.id))?;
            println!("Play on {} now always runs {}", row.title, v.title);
        }
        AlwaysRunCommand::Clear { game } => {
            let row = lib.find_row(&game)?;
            lib.set_always_run(&row.id, None)?;
            println!("Play on {} asks which version again", row.title);
        }
    }
    Ok(())
}
