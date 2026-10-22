//! `import`, `convert` and `cleanup`.

use anyhow::{Result, bail};

use romforge_core::{Library, Scope, cleanup};

use crate::args::Command;
use crate::print::{bytes, print_cleanup};
use crate::subcommands::CleanupCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Import { dir, apply, verbose } => {
            if lib.home().is_none() {
                bail!("no RomForge folder yet: `romforge-cli home create <parent>` first");
            }
            let scanned = if let Some(d) = &dir {
                lib.scan(d, Scope::All)?;
                true
            } else {
                false
            };
            let plan = lib.import_plan(dir.as_deref())?;
            print_cleanup(&plan, verbose);
            if !apply {
                println!(
                    "{} to import into {}, {} skipped; {} (run with --apply to move them)",
                    plan.items.len(),
                    plan.dest.display(),
                    plan.skipped.len(),
                    if scanned {
                        "hashed into the library, nothing moved"
                    } else {
                        "nothing moved"
                    }
                );
                return Ok(());
            }
            if plan.items.is_empty() {
                println!("nothing to import ({} skipped)", plan.skipped.len());
                return Ok(());
            }
            apply_plan(lib, &plan, "imported", "games")?;
        }
        Command::Convert { apply, verbose } => {
            let plan = lib.convert_plan(None)?;
            print_cleanup(&plan, verbose);
            if !apply {
                println!(
                    "{} to convert, {} skipped; nothing changed (run with --apply)",
                    plan.items.len(),
                    plan.skipped.len()
                );
                return Ok(());
            }
            if plan.items.is_empty() {
                println!("nothing to convert ({} skipped)", plan.skipped.len());
                return Ok(());
            }
            apply_plan(lib, &plan, "converted", "discs")?;
        }
        Command::Cleanup(CleanupCommand::Plan { verbose }) => {
            let plan = lib.cleanup_plan(None)?;
            print_cleanup(&plan, verbose);
            println!(
                "{} to write, {} skipped; nothing changed (run `cleanup apply` to write)",
                plan.items.len(),
                plan.skipped.len()
            );
        }
        Command::Cleanup(CleanupCommand::Apply { verbose }) => {
            let plan = lib.cleanup_plan(None)?;
            print_cleanup(&plan, verbose);
            if plan.items.is_empty() {
                println!("nothing to write ({} skipped)", plan.skipped.len());
                return Ok(());
            }
            apply_plan(lib, &plan, "wrote", "games")?;
        }
        Command::Cleanup(CleanupCommand::Undo { batch }) => {
            let u = lib.cleanup_undo(batch)?;
            for e in &u.failed {
                eprintln!("error: {e}");
            }
            println!(
                "batch #{}: removed {}, restored {}, failed {}",
                u.batch_id,
                u.removed,
                u.restored,
                u.failed.len()
            );
            if !u.failed.is_empty() {
                bail!("{} steps could not be undone", u.failed.len());
            }
        }
        Command::Cleanup(CleanupCommand::EmptyTrash { older_than, yes }) => {
            let t = if yes {
                lib.empty_trash(older_than)?
            } else {
                lib.trash_contents(older_than)?
            };
            for b in &t.batches {
                println!(
                    "#{:<4} {}  {:>6} files  {}",
                    b.batch_id,
                    b.created_at,
                    b.files,
                    bytes(b.bytes)
                );
            }
            for f in &t.failed {
                eprintln!("error: {f}");
            }
            if yes {
                println!(
                    "deleted {} files ({}) from {} batch(es); those can no longer be undone",
                    t.files,
                    bytes(t.bytes),
                    t.batches.len()
                );
            } else {
                println!(
                    "{} files ({}) in {} batch(es); nothing deleted (run with --yes to delete them for good)",
                    t.files,
                    bytes(t.bytes),
                    t.batches.len()
                );
            }
        }
        Command::Cleanup(CleanupCommand::History) => {
            for b in lib.cleanup_batches()? {
                let state = match (&b.undone_at, &b.emptied_at) {
                    (Some(t), _) => format!("undone {t}"),
                    (None, Some(t)) => format!("trash emptied {t}"),
                    (None, None) => "applied".into(),
                };
                let keep = if b.keep_originals {
                    "kept originals"
                } else {
                    "trashed originals"
                };
                println!(
                    "#{:<4} {}  {:>5} written  {:>5} trashed  {keep}  {state}",
                    b.batch_id, b.created_at, b.outputs, b.trashed
                );
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// Carry out `plan` with a line per game, then say what was done
/// (`verb`: imported, converted, wrote); fails when any of its `what`
/// (games, discs) failed.
fn apply_plan(lib: &mut Library, plan: &cleanup::CleanupPlan, verb: &str, what: &str) -> Result<()> {
    let total = plan.items.len();
    let r = lib.cleanup_apply(plan, |n, title| eprintln!("[{}/{total}] {title}", n + 1))?;
    for (t, e) in &r.failed {
        eprintln!("error: {t}: {e}");
    }
    println!(
        "batch #{}: {verb} {}, trashed {} originals, failed {} (undo with `cleanup undo --batch {}`)",
        r.batch_id,
        r.done,
        r.trashed,
        r.failed.len(),
        r.batch_id
    );
    if !r.failed.is_empty() {
        bail!("{} {what} failed", r.failed.len());
    }
    Ok(())
}
