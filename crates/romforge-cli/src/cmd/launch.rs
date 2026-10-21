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
            wait,
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
            if wait && !dry_run {
                let session = launch::Session::start(&spec)?;
                println!("started (pid {}); waiting for it to end…", session.playing.pid);
                let ended = session.wait();
                let kept = lib.record_play(&ended)?;
                println!(
                    "ended after {} s{}",
                    ended.seconds,
                    if kept {
                        "; play time recorded"
                    } else {
                        " (too short to count)"
                    }
                );
            } else if !dry_run {
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
        Command::Emulator(EmulatorCommand::ImportSetup {
            emulator,
            apply,
            undo,
        }) => {
            if undo {
                let n = lib.undo_emulator_setup(&emulator)?;
                println!("{emulator}: {n} file(s) taken back");
            } else if !apply {
                println!(
                    "{emulator}: would copy its existing setup into romforge (run again with --apply; --undo takes the last import back)"
                );
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
        Command::Emulator(EmulatorCommand::Cores { refresh }) => {
            if refresh {
                println!("buildbot: {} cores listed", lib.refresh_emulator_cores()?);
            }
            let list = lib.emulator_cores()?;
            if !list.checked {
                println!("(the buildbot's list hasn't been fetched: --refresh)");
            }
            for c in &list.cores {
                let state = match (&c.installed, &c.ours, c.update) {
                    (_, Some(d), true) => {
                        format!("romforge's {d}, update {}", c.available.as_deref().unwrap_or(""))
                    }
                    (_, Some(d), false) => format!("romforge's {d}"),
                    (Some(p), None, _) => format!("installed: {}", p.display()),
                    (None, None, _) => "not installed".to_owned(),
                };
                let nc = if c.non_commercial { " NON-COMMERCIAL" } else { "" };
                println!(
                    "{:<22} {:<40} [{}{nc}] {state}",
                    c.name, c.display_name, c.license
                );
            }
        }
        Command::Emulator(EmulatorCommand::GetCore { core, yes }) => {
            let list = lib.emulator_cores()?;
            if let Some(c) = list.cores.iter().find(|c| c.name == core) {
                println!("{}: licence {}", c.display_name, c.license);
                if c.non_commercial && !yes {
                    bail!("{core}'s licence forbids commercial use; run again with --yes to accept it");
                }
            }
            let date = lib.download_core(&core)?;
            println!("{core} ({date}) downloaded into the RomForge folder's Emulators/cores");
        }
        Command::Emulator(EmulatorCommand::RemoveCore { core, yes }) => {
            if !yes {
                bail!("refusing to remove {core} without --yes");
            }
            lib.remove_core(&core)?;
            println!("{core} removed");
        }
        Command::Emulator(EmulatorCommand::Install { emulator, apply }) => {
            if !apply {
                println!("would install {emulator} from Flathub into your Flatpak (run again with --apply)");
                return Ok(());
            }
            println!("installing {emulator} from Flathub (this takes a while)…");
            let id = lib.install_emulator(&emulator)?;
            println!("installed {id}");
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
