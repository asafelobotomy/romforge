//! `games`.

use anyhow::Result;

use romforge_core::Library;

use crate::args::Command;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Games { platform, sort, desc } => {
            use romforge_core::view::{GameRow, SortBy};
            let by = SortBy::parse(&sort).ok_or_else(|| {
                anyhow::anyhow!("--sort takes title, platform, year or status, not {sort:?}")
            })?;
            let mut v = lib.library_view()?;
            romforge_core::view::sort_rows(&mut v.rows, by, desc);
            let line = |r: &GameRow, indent: &str| {
                let gathered = format!("{}{}", if r.metadata { "info " } else { "" }, r.art.join(" "));
                println!(
                    "{:<11} {:<16} {indent}{}  [{}]",
                    format!("{:?}", r.status).to_lowercase(),
                    r.system.as_deref().unwrap_or("-"),
                    r.label.as_deref().unwrap_or(&r.title),
                    gathered.trim()
                );
            };
            // A game's original, then its versions under their headings.
            for r in v.main_rows(platform.as_deref()) {
                line(r, "");
                let mut patches = false;
                for (c, of) in v.versions_of(r) {
                    let indent = if c.is_patch() {
                        if !patches {
                            println!("{:<28} - Patches", "");
                            patches = true;
                        }
                        "    "
                    } else {
                        ""
                    };
                    println!("{:<28} {indent}- {}", "", c.heading());
                    for m in of {
                        line(m, &format!("{indent}    "));
                    }
                }
            }
            let n: Vec<String> = v
                .platforms
                .iter()
                .map(|p| format!("{} {}", p.games, p.name))
                .collect();
            println!("{} rows; {}", v.rows.len(), n.join(", "));
        }
        _ => unreachable!(),
    }
    Ok(())
}
