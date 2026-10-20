//! `manual`.

use anyhow::Result;

use romforge_core::Library;

use crate::args::Command;
use crate::subcommands::ManualCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    let Command::Manual(cmd) = command else {
        unreachable!()
    };
    match cmd {
        ManualCommand::Show { game } => {
            let row = lib.find_row(&game)?;
            let mut info = lib.manual_info(&row.id)?;
            if info.path.is_none() && !info.checked {
                lib.manual_lookup(&row.id)?;
                info = lib.manual_info(&row.id)?;
            }
            match (&info.path, &info.vimm_id) {
                (Some(p), _) => println!(
                    "{}: {} ({}; page {})",
                    row.title,
                    p.display(),
                    info.source.as_deref().unwrap_or("yours"),
                    info.last_page
                ),
                (None, Some(id)) => println!(
                    "{}: no manual yet; Vimm's Lair has one ({}{id}): `manual download`",
                    row.title,
                    romforge_core::manual::VIMM_PAGE
                ),
                (None, None) => println!("{}: no manual, and Hasheous knows none on Vimm's Lair", row.title),
            }
        }
        ManualCommand::Find { game } => {
            let row = lib.find_row(&game)?;
            let offers = lib.manual_offers(&row.id)?;
            if offers.is_empty() {
                println!("{}: archive.org has no manual under this name", row.title);
            }
            for (i, o) in offers.iter().enumerate() {
                let size = o
                    .size
                    .map(|s| format!(", {:.1} MB", s as f64 / 1e6))
                    .unwrap_or_default();
                println!("{}. {} ({}{size})", i + 1, o.name, o.source);
            }
        }
        ManualCommand::Get { game, pick } => {
            let row = lib.find_row(&game)?;
            let offers = lib.manual_offers(&row.id)?;
            let o = offers
                .get(pick.saturating_sub(1))
                .ok_or_else(|| anyhow::anyhow!("{}: no manual {pick} (see `manual find`)", row.title))?;
            println!(
                "{}: downloaded to {}",
                row.title,
                lib.download_manual_offer(&row.id, o)?.display()
            );
        }
        ManualCommand::Add { game, pdf } => {
            let row = lib.find_row(&game)?;
            println!("{}: {}", row.title, lib.add_manual(&row.id, &pdf)?.display());
        }
        ManualCommand::Download { game, dpi } => {
            let row = lib.find_row(&game)?;
            println!(
                "{}: downloaded to {}",
                row.title,
                lib.download_manual(&row.id, dpi)?.display()
            );
        }
        ManualCommand::Remove { game } => {
            let row = lib.find_row(&game)?;
            lib.remove_manual(&row.id)?;
            println!("{}: manual removed", row.title);
        }
    }
    Ok(())
}
