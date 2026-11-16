//! `games`.

use anyhow::Result;

use romforge_core::Library;

use crate::args::Command;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Overview { json } => {
            let o = lib.overview()?;
            if json {
                return print_json(&o);
            }
            let t = &o.totals;
            println!(
                "{} games ({} verified, {} incomplete), {} unidentified files, {} BIOS files",
                t.games, t.verified, t.incomplete, t.unidentified, t.bios_files
            );
            for p in &o.platforms {
                println!("  {:<28} {} ({} verified)", p.name, p.games, p.verified);
            }
            for r in &o.recent {
                println!(
                    "  added {}  {} [{}]",
                    r.added,
                    r.game.title,
                    r.game.system.as_deref().unwrap_or("-")
                );
            }
            for b in &o.bios {
                println!("  BIOS missing for {} ({})", b.name, b.emulator);
            }
            for m in &o.lists.missing {
                println!("  no DAT for {}", m.name);
            }
        }
        Command::Browse {
            platform,
            status,
            sort,
            limit,
            offset,
            search,
        } => {
            use romforge_core::overview::{GamesQuery, StatusFilter};
            use romforge_core::view::SortBy;
            let bad = || {
                anyhow::anyhow!(
                    "SORT takes title, platform, year or status, with :desc or :asc, not {sort:?}"
                )
            };
            let (key, descending) = match sort.split_once(':') {
                Some((k, "desc")) => (k, true),
                Some((k, "asc")) => (k, false),
                Some(_) => return Err(bad()),
                None => (sort.as_str(), false),
            };
            let by = SortBy::parse(key).ok_or_else(bad)?;
            let status = StatusFilter::parse(&status).ok_or_else(|| {
                anyhow::anyhow!("STATUS takes all, verified, incomplete or unidentified, not {status:?}")
            })?;
            let q = GamesQuery {
                platform: (!platform.eq_ignore_ascii_case("all")).then_some(platform),
                status,
                search: Some(search.join(" ")).filter(|w| !w.trim().is_empty() && w != "*"),
                sort: by,
                descending,
                limit,
                offset,
            };
            return print_json(&lib.games_matching(&q)?);
        }
        Command::Games {
            platform,
            sort,
            desc,
            status,
            search,
            limit,
            offset,
            json,
        } => {
            use romforge_core::overview::{GamesQuery, StatusFilter};
            use romforge_core::view::{GameRow, SortBy};
            let by = SortBy::parse(&sort).ok_or_else(|| {
                anyhow::anyhow!("--sort takes title, platform, year or status, not {sort:?}")
            })?;
            if json {
                let status = StatusFilter::parse(&status).ok_or_else(|| {
                    anyhow::anyhow!(
                        "--status takes all, verified, incomplete or unidentified, not {status:?}"
                    )
                })?;
                let q = GamesQuery {
                    platform,
                    status,
                    search,
                    sort: by,
                    descending: desc,
                    limit: limit.unwrap_or(200),
                    offset,
                };
                return print_json(&lib.games_matching(&q)?);
            }
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

/// Print `value` as JSON with `schema_version` 1 first.
fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let mut v = serde_json::to_value(value)?;
    if let Some(o) = v.as_object_mut() {
        o.insert("schema_version".into(), 1.into());
    }
    println!("{}", serde_json::to_string(&v)?);
    Ok(())
}
