//! `emulator config` (All settings) and `emulator game-settings`.

use anyhow::{Context, Result};
use romforge_core::Library;
use romforge_core::emusettings::raw::{RawEdit, RawKind};

use crate::subcommands::EmulatorCommand;

/// `FILE:KEY=VALUE` (or `FILE:KEY` with no value).
fn edit_of(arg: &str, with_value: bool) -> Result<RawEdit> {
    let (file, rest) = arg.split_once(':').context("FILE:KEY expected")?;
    let (key, value) = if with_value {
        let (k, v) = rest.split_once('=').context("FILE:KEY=VALUE expected")?;
        (k, Some(v.to_owned()))
    } else {
        (rest, None)
    };
    Ok(RawEdit {
        file: file.to_owned(),
        key: key.to_owned(),
        value,
    })
}

pub fn run(lib: &mut Library, command: EmulatorCommand) -> Result<()> {
    match command {
        EmulatorCommand::Config {
            emulator,
            set,
            unset,
            undo,
            write_defaults,
        } => {
            if undo {
                let n = lib.undo_raw_settings(&emulator)?;
                println!("{emulator}: last change taken back ({n} key(s))");
                return Ok(());
            }
            if write_defaults {
                println!(
                    "MAME wrote its settings into {}",
                    lib.mame_write_defaults()?.display()
                );
            }
            let edits: Vec<RawEdit> = set
                .iter()
                .map(|a| edit_of(a, true))
                .chain(unset.iter().map(|a| edit_of(a, false)))
                .collect::<Result<_>>()?;
            if !edits.is_empty() {
                lib.edit_raw_settings(&emulator, &edits)?;
                println!(
                    "{emulator}: {} change(s) saved (--undo takes them back)",
                    edits.len()
                );
                return Ok(());
            }
            let v = lib.raw_settings(&emulator)?;
            if v.running {
                println!("({emulator} is running: nothing can be changed until it closes)");
            }
            for f in &v.files {
                println!("{}{}", f.file, if f.exists { "" } else { "  (not written yet)" });
                for s in &f.settings {
                    let kind = match &s.kind {
                        RawKind::Bool { on, off } => format!(" [{on}/{off}]"),
                        _ => String::new(),
                    };
                    match &s.locked {
                        Some(why) => println!("  {} = {}{kind}  (locked: {why})", s.key, s.value),
                        None => println!("  {} = {}{kind}", s.key, s.value),
                    }
                }
            }
        }
        EmulatorCommand::GameSettings {
            system,
            title,
            key,
            value,
        } => {
            let v = lib.game_settings(&system, &title)?;
            if let Some(key) = key {
                let mut values = v.values.clone();
                match &value {
                    Some(val) => values.insert(key.clone(), val.clone()),
                    None => values.remove(&key),
                };
                lib.set_game_settings(&system, &title, &values)?;
                match value {
                    Some(val) => println!("{title}: {key} = {val}"),
                    None => println!("{title}: {key} is the emulator's again"),
                }
                return Ok(());
            }
            if v.settings.is_empty() {
                println!("no emulator for {system} reads settings of a game's own");
            }
            for g in &v.settings {
                let set = v
                    .values
                    .get(g.setting.key)
                    .map_or("(the emulator's)", String::as_str);
                println!("{:<14} {set}  [{}]", g.setting.key, g.emulators.join(", "));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
