//! Thin command-line shell over romforge-core.

mod args;
mod cmd;
mod print;
mod subcommands;

use anyhow::{Context, Result, bail};
use clap::Parser;

use romforge_core::{Library, enrich, systems};

use crate::args::{Cli, Command};
use crate::subcommands::{DatCommand, HomeCommand};

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
                let art = enrich::default_cache_dir().join(romforge_core::layout::LOOSE_ART_DIR);
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
            new.remember()?;
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
        c @ (Command::Dat(_) | Command::Databases(_) | Command::Scummvm(_)) => cmd::dat::run(&mut lib, c)?,
        c @ (Command::Scan { .. } | Command::Refresh { .. } | Command::Report { .. }) => {
            cmd::scan::run(&mut lib, c)?
        }
        c @ Command::Organize(_) => cmd::organize::run(&mut lib, c)?,
        c @ (Command::Import { .. } | Command::Convert { .. } | Command::Cleanup(_)) => {
            cmd::cleanup::run(&mut lib, c)?
        }
        c @ Command::Patch { .. } => cmd::patch::run(&mut lib, c)?,
        c @ (Command::Enrich { .. } | Command::Info { .. }) => cmd::enrich::run(&mut lib, c)?,
        c @ (Command::Launch { .. } | Command::Emulator(_)) => cmd::launch::run(&mut lib, c)?,
        c @ Command::Settings { .. } => cmd::settings::run(&mut lib, c)?,
        c @ Command::Search(_) => cmd::search::run(&mut lib, c)?,
        c @ Command::Games { .. } => cmd::games::run(&mut lib, c)?,
        Command::Home(_) => unreachable!("handled before the library opens"),
    }
    Ok(())
}
