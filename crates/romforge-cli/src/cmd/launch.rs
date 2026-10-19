//! `launch` and `emulator`.

use anyhow::{Result, bail};

use romforge_core::{Library, emusettings, launch};

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
            for w in &spec.prepare.writes {
                let verb = if dry_run { "would set" } else { "sets" };
                println!("  {verb} {} key(s) in {}", w.keys.len(), w.path.display());
            }
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
        Command::Emulator(EmulatorCommand::Settings) => {
            let v = lib.emulator_settings()?;
            let show = |scope: &str, values: &emusettings::Values| {
                for (k, val) in values {
                    println!("{scope:<16} {k:<12} {val}");
                }
            };
            show("global", &v.values.global);
            for (name, values) in &v.values.emulators {
                show(name, values);
            }
            for tab in &v.installed {
                println!(
                    "installed: {} ({})",
                    tab.label,
                    tab.found.as_deref().unwrap_or("-")
                );
            }
        }
        Command::Emulator(EmulatorCommand::Set { emulator, key, value }) => {
            let mut s = lib.emulator_settings()?.values;
            let values = if emulator.eq_ignore_ascii_case("global") {
                &mut s.global
            } else {
                s.emulators.entry(emulator.to_ascii_lowercase()).or_default()
            };
            match &value {
                Some(v) => values.insert(key.clone(), v.clone()),
                None => values.remove(&key),
            };
            lib.save_emulator_settings(&s)?;
            match value {
                Some(v) => println!("{emulator} {key} = {v}"),
                None => println!("{emulator} {key} unset"),
            }
        }
        Command::Emulator(EmulatorCommand::ImportSetup { emulator, undo }) => {
            if undo {
                let n = lib.undo_emulator_setup(&emulator)?;
                println!("{emulator}: {n} file(s) taken back");
            } else {
                let d = lib.import_emulator_setup(&emulator)?;
                println!(
                    "{emulator}: {} file(s) copied ({} KiB), {} already there kept",
                    d.copied,
                    d.bytes / 1024,
                    d.kept
                );
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
