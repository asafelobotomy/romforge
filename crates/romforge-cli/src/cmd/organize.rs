//! `organize`.

use anyhow::{Result, bail};

use romforge_core::Library;

use crate::args::Command;
use crate::print::print_plan;
use crate::subcommands::OrganizeCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Organize(OrganizeCommand::Plan(args)) => {
            let plan = lib.organize_plan(&args.options())?;
            print_plan(&plan, args.verbose);
            println!(
                "{} to move, {} staying; nothing changed (run `organize apply` to move)",
                plan.moves.len(),
                plan.skipped.len()
            );
        }
        Command::Organize(OrganizeCommand::Apply(args)) => {
            let plan = lib.organize_plan(&args.options())?;
            print_plan(&plan, args.verbose);
            if plan.moves.is_empty() {
                println!(
                    "nothing to move ({} files stay; -v shows why)",
                    plan.skipped.len()
                );
                return Ok(());
            }
            let applied = lib.organize_apply(&plan, &args.dest)?;
            for (m, e) in &applied.failed {
                eprintln!("error: {}: {e}", m.from.display());
            }
            println!(
                "batch #{}: moved {}, failed {} (undo with `organize undo --batch {}`)",
                applied.batch_id,
                applied.moved,
                applied.failed.len(),
                applied.batch_id
            );
            if !applied.failed.is_empty() {
                bail!("{} moves failed", applied.failed.len());
            }
        }
        Command::Organize(OrganizeCommand::Undo { batch }) => {
            let undone = lib.organize_undo(batch)?;
            for (p, e) in &undone.failed {
                eprintln!("error: {}: {e}", p.display());
            }
            println!(
                "batch #{}: restored {}, failed {}",
                undone.batch_id,
                undone.restored,
                undone.failed.len()
            );
            if !undone.failed.is_empty() {
                bail!("{} files could not be moved back", undone.failed.len());
            }
        }
        Command::Organize(OrganizeCommand::History) => {
            for b in lib.organize_batches()? {
                let state = match &b.undone_at {
                    Some(t) => format!("undone {t}"),
                    None => "applied".into(),
                };
                println!(
                    "#{:<4} {}  {:>5} moves  {state}  -> {}",
                    b.batch_id, b.created_at, b.moves, b.dest
                );
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
