//! `romforge-cli steam`: romforge in Steam (romforge_core::steam).

use anyhow::{Context, Result};
use romforge_core::steam;

use crate::subcommands::SteamCommand;

fn root_and_user(user: Option<&str>) -> Result<(std::path::PathBuf, String)> {
    steam::resolve(user).context("see `romforge-cli steam status`")
}

pub fn run(command: &SteamCommand) -> Result<()> {
    match command {
        SteamCommand::Status => {
            let s = steam::status();
            let Some(root) = &s.root else {
                println!("Steam isn't installed here");
                return Ok(());
            };
            println!(
                "Steam: {}{}",
                root.display(),
                if s.running { " (running)" } else { "" }
            );
            for u in &s.users {
                let added = s.added.iter().find(|(a, _)| *a == u.account);
                println!(
                    "  {} {}{}: {}",
                    u.account,
                    u.name,
                    if u.recent { " (most recent)" } else { "" },
                    added.map_or("romforge not added".to_owned(), |(_, exe)| format!(
                        "romforge added, starts {exe}"
                    ))
                );
            }
            if let Some(exe) = &s.exe {
                println!("adding now would start {}", exe.display());
            }
        }
        SteamCommand::Add { dry_run, user } => {
            let (root, account) = root_and_user(user.as_deref())?;
            let p = steam::plan(&root, &account, &steam::this_exe()?)?;
            println!(
                "{} romforge in Steam user {account}'s library: {} {} ({} other shortcuts kept)",
                if p.replaces { "replace" } else { "add" },
                p.exe,
                p.launch_options,
                p.others
            );
            for f in &p.files {
                println!("  write {}", f.display());
            }
            if *dry_run {
                return Ok(());
            }
            steam::apply(&p)?;
            println!(
                "done ({} backed up). Start Steam; in romforge's controller settings pick the \"romforge (Deck)\" template.",
                steam::BACKUP
            );
        }
        SteamCommand::Remove { user } => {
            let (root, account) = root_and_user(user.as_deref())?;
            if steam::remove(&root, &account)? {
                println!(
                    "romforge taken out of Steam user {account}'s library ({} backed up)",
                    steam::BACKUP
                );
            } else {
                println!("romforge wasn't in Steam user {account}'s library");
            }
        }
    }
    Ok(())
}
