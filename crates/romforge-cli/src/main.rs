//! Thin command-line shell over romforge-core.

mod args;
mod cmd;
mod print;
mod subcommands;

use anyhow::{Context, Result, bail};
use clap::Parser;

use romforge_core::{Library, systems};

use crate::args::{Cli, Command};
use crate::subcommands::{DatCommand, EmulatorCommand, HomeCommand, ThemesCommand};

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
            use romforge_core::home::{Created, Home};
            // The folder in use: the one the database given is in, else
            // the one recorded.
            let current = match &cli.db {
                Some(d) => Home::of_db(d),
                None => home.clone(),
            };
            let made = romforge_core::home::create(parent, current.as_ref(), &db);
            if made.result.is_err() {
                match &made.db {
                    Some(at) => eprintln!("the library is at {}", at.display()),
                    None => eprintln!("the library couldn't be found again"),
                }
            }
            let done = made
                .result
                .with_context(|| format!("setting up the RomForge folder in {}", parent.display()))?;
            let new = done.home();
            match &done {
                Created::Existing { .. } => println!("{} is already a RomForge folder", new.root().display()),
                Created::Adopted { moved: a, .. } => println!(
                    "moved the library into {}: database {}, {} downloaded databases, {} images",
                    new.root().display(),
                    if a.database { "yes" } else { "no" },
                    a.databases,
                    a.art
                ),
                Created::New { .. } => {}
            }
            println!("using {}", new.root().display());
            return Ok(());
        }
        Command::Home(HomeCommand::Move { new_parent }) => {
            let Some(old) = &home else {
                bail!("no RomForge folder yet (`romforge-cli home create <parent>`)");
            };
            let made = romforge_core::home::move_to(old, new_parent);
            if let (Err(_), Some(at)) = (&made.result, &made.db) {
                eprintln!("the library is still at {}", at.display());
            }
            let new = made.result?;
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

    // Steam needs no library.
    if let Command::Steam(c) = &cli.command {
        return cmd::steam::run(c);
    }

    // Making an index needs no library.
    if let Command::Themes(ThemesCommand::Index { repo, check }) = &cli.command {
        return cmd::themes::index(repo, *check);
    }

    if let Command::Dat(DatCommand::Systems) = cli.command {
        for s in systems::SYSTEMS {
            let sources: String = s.dat_sources().iter().map(|t| format!("  [{t}]")).collect();
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
        Command::Emulator(c @ (EmulatorCommand::Config { .. } | EmulatorCommand::GameSettings { .. })) => {
            cmd::emulator_config::run(&mut lib, c)?
        }
        c @ (Command::Launch { .. } | Command::Emulator(_)) => cmd::launch::run(&mut lib, c)?,
        c @ Command::Firmware(_) => cmd::firmware::run(&mut lib, c)?,
        c @ Command::Saves(_) => cmd::saves::run(&mut lib, c)?,
        c @ Command::Manual(_) => cmd::manual::run(&mut lib, c)?,
        c @ Command::Settings { .. } => cmd::settings::run(&mut lib, c)?,
        c @ Command::Search(_) => cmd::search::run(&mut lib, c)?,
        c @ Command::Games { .. } => cmd::games::run(&mut lib, c)?,
        Command::Themes(c) => cmd::themes::run(&lib, c)?,
        Command::Steam(_) => unreachable!("handled before the library opens"),
        Command::Home(_) => unreachable!("handled before the library opens"),
    }
    Ok(())
}
