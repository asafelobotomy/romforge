//! `saves`.

use anyhow::Result;

use romforge_core::Library;

use crate::args::Command;
use crate::subcommands::SavesCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    let Command::Saves(c) = command else {
        return Ok(());
    };
    match c {
        SavesCommand::Gather {
            emulator,
            apply,
            undo,
        } => gather(lib, &emulator, apply, undo)?,
        SavesCommand::Backup { emulator } => {
            let d = lib.backup_saves(&emulator)?;
            println!(
                "{emulator}: {} save folder(s) copied, {} unchanged, {} old copies removed",
                d.copied, d.same, d.pruned
            );
        }
        SavesCommand::Backups { emulator } => {
            for b in lib.save_backups(&emulator)? {
                let gone = if b.source.is_none() {
                    "  (gone from the emulator)"
                } else {
                    ""
                };
                println!("{}{gone}", b.save.display());
                match b.versions.as_slice() {
                    [] => println!("    no copy yet"),
                    v => println!("    {}", v.join(", ")),
                }
            }
        }
        SavesCommand::Restore {
            emulator,
            save,
            version,
            apply,
            undo,
        } => {
            if undo {
                let at = lib.undo_restore_save(&emulator)?;
                println!("{emulator}: {} is as it was before the restore", at.display());
                return Ok(());
            }
            let save = save.unwrap_or_default();
            let p = lib.restore_save_plan(&emulator, &save, version.as_deref())?;
            if let Some(why) = &p.problem {
                println!("{emulator}: can't restore {}: {why}", save.display());
                return Ok(());
            }
            println!("{}\n    → {}", p.from.display(), p.to.display());
            if p.replaces {
                println!("    the save there now goes to the trash");
            }
            if apply {
                let id = lib.restore_save(&emulator, &save, version.as_deref())?;
                println!("{emulator}: restored from {} (restore #{id})", p.version);
            } else {
                println!("run again with --apply to restore it (--undo takes it back)");
            }
        }
    }
    Ok(())
}

fn gather(lib: &mut Library, emulator: &str, apply: bool, undo: bool) -> Result<()> {
    if undo {
        let n = lib.undo_gather_saves(emulator)?;
        println!("{emulator}: {n} save file(s) put back where they were");
        return Ok(());
    }
    let p = lib.gather_saves_plan(emulator)?;
    for m in &p.moves {
        let taken = if m.taken { "  (a file is there: stays)" } else { "" };
        println!("{}\n    → {}{taken}", m.from.display(), m.to.display());
    }
    for u in &p.unplaced {
        println!("{}\n    stays: its game isn't in the library", u.display());
    }
    match (&p.problem, apply) {
        (Some(why), _) => println!("{emulator}: nothing to gather: {why}"),
        (None, false) => println!("run again with --apply to move them (--undo puts them back)"),
        (None, true) => {
            let (id, n) = lib.gather_saves(emulator)?;
            println!("{emulator}: {n} save file(s) moved into Saves/ (gathering #{id})");
        }
    }
    Ok(())
}
