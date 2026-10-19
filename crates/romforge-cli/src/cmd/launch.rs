//! `launch` and `emulator`.

use anyhow::{Result, bail};

use romforge_core::{Library, launch};

use crate::args::Command;
use crate::subcommands::EmulatorCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Launch {
            query,
            emulator,
            dry_run,
        } => {
            let spec = lib.launch_plan(&lib.env()?, &query, emulator.as_deref())?;
            let shown: Vec<String> = std::iter::once(spec.program.display().to_string())
                .chain(spec.args.iter().map(|a| {
                    if a.contains(' ') {
                        format!("'{a}'")
                    } else {
                        a.clone()
                    }
                }))
                .collect();
            println!(
                "{} [{}] with {}: {}",
                spec.game,
                spec.system,
                spec.emulator,
                shown.join(" ")
            );
            if !dry_run {
                let pid = launch::spawn(&spec)?;
                println!("started (pid {pid})");
            }
        }
        Command::Emulator(EmulatorCommand::List { all }) => {
            for e in lib.emulators(&lib.env()?)? {
                match (&e.found, all) {
                    (Some(how), _) => println!("{:<28} {:<40} {how}", e.name, e.systems.join(", ")),
                    (None, true) => println!("{:<28} {:<40} -", e.name, e.systems.join(", ")),
                    (None, false) => {}
                }
            }
        }
        Command::Emulator(EmulatorCommand::Add {
            name,
            system,
            program,
            args,
            archives,
        }) => {
            lib.add_emulator(&launch::UserEmulator {
                name: name.clone(),
                system,
                program,
                args: args.split_whitespace().map(str::to_owned).collect(),
                archives,
            })?;
            println!("added {name}");
        }
        Command::Emulator(EmulatorCommand::Path { name, program }) => {
            let mut s = lib.settings()?;
            match program {
                Some(p) => s.emulator_paths.insert(name.clone(), p),
                None => s.emulator_paths.remove(&name),
            };
            let saved = lib.save_settings(&s)?;
            match saved
                .emulator_paths
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(&name))
            {
                Some((n, p)) => println!("{n}: {}", p.display()),
                None => println!("{name}: found on its own again"),
            }
        }
        Command::Emulator(EmulatorCommand::Remove { name }) => {
            let n = lib.remove_emulator(&name)?;
            if n == 0 {
                bail!("no emulator named {name:?} (only ones you added can be removed)");
            }
            println!("removed {name}");
        }
        _ => unreachable!(),
    }
    Ok(())
}
