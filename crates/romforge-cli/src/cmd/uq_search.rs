//! `romforge-cli uq-search`: romforge's games in uQ's search without a
//! package (romforge_core::uq::user_provider).

use anyhow::{Result, bail};
use romforge_core::uq::user_provider as up;

use crate::subcommands::UqSearchCommand;

pub fn run(command: &UqSearchCommand) -> Result<()> {
    match command {
        UqSearchCommand::Status => {
            let Some(s) = up::status() else {
                bail!("no home folder");
            };
            if s.packaged {
                println!("a package installed romforge's uQ search; nothing to add");
            } else if s.added {
                println!(
                    "added{}",
                    if s.stale {
                        " (the copy is older than this romforge: `add` updates it)"
                    } else {
                        ""
                    }
                );
            } else {
                println!("not added");
            }
            match &s.source {
                Some(p) => println!("provider to copy: {}", p.display()),
                None => println!("no romforge-provider beside this program to copy"),
            }
            for f in &s.files {
                println!(
                    "  {}{}",
                    f.display(),
                    if f.exists() { "" } else { " (not there)" }
                );
            }
        }
        UqSearchCommand::Add => {
            up::add()?;
            println!(
                "added: uQ's search finds your games within a minute (uQ reads these folders as they change)"
            );
        }
        UqSearchCommand::Remove => {
            if up::remove()? {
                println!("removed");
            } else {
                println!("nothing of romforge's to remove");
            }
        }
    }
    Ok(())
}
