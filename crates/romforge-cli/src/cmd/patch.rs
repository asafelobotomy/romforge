//! `patch`.

use anyhow::Result;

use romforge_core::Library;

use crate::args::Command;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Patch {
            game,
            patch,
            hack,
            translation,
            tag,
            apply,
        } => {
            let row = lib.find_row(&game)?;
            let Some(patch) = patch else {
                println!(
                    "{}: where to look for a patch (opens in your browser):",
                    row.title
                );
                for l in romforge_core::view::patch_links(&row.title) {
                    println!("  {:<32} {}", l.label, l.url);
                }
                let found = lib.patches_beside(&row.id)?;
                if !found.is_empty() {
                    println!("patches for it:");
                    for p in found {
                        let how = match p.paired_by.as_str() {
                            "source" => "made from its ROM",
                            "names" => "names its file",
                            "beside" => "beside it, same name",
                            "title" => "named after the game",
                            "readme" => "its readme names the game",
                            _ => "in its folder",
                        };
                        println!(
                            "  {} ({}, {}; {how})",
                            p.path.display(),
                            p.kind,
                            p.relation.as_str()
                        );
                    }
                }
                return Ok(());
            };
            let relation = if hack {
                Some(romforge_core::view::Relation::Hack)
            } else if translation {
                Some(romforge_core::view::Relation::Translation)
            } else {
                None
            };
            let plan = lib.patch_plan(&row.id, &patch, relation, tag.as_deref())?;
            println!("{} ({} patch {})", plan.title, plan.kind, plan.patch_name);
            println!("  applied to {}", plan.applies_to);
            println!("  into {}", plan.target.display());
            for n in &plan.notes {
                println!("  note: {n}");
            }
            if apply {
                let done = lib.patch_apply(&plan)?;
                println!(
                    "batch #{}: wrote {} (undo with `cleanup undo --batch {}`)",
                    done.batch_id,
                    done.target.display(),
                    done.batch_id
                );
            } else {
                println!("nothing written (run with --apply)");
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
