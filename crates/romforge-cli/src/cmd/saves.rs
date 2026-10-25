//! `saves`.

use anyhow::Result;

use romforge_core::Library;

use crate::args::Command;
use crate::subcommands::SavesCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    let Command::Saves(SavesCommand::Gather {
        emulator,
        apply,
        undo,
    }) = command
    else {
        return Ok(());
    };
    if undo {
        let n = lib.undo_gather_saves(&emulator)?;
        println!("{emulator}: {n} save file(s) put back where they were");
        return Ok(());
    }
    let p = lib.gather_saves_plan(&emulator)?;
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
            let (id, n) = lib.gather_saves(&emulator)?;
            println!("{emulator}: {n} save file(s) moved into Saves/ (gathering #{id})");
        }
    }
    Ok(())
}
