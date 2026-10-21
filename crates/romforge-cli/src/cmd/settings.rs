//! `settings`.

use std::path::PathBuf;

use anyhow::Result;

use romforge_core::{Library, settings};

use crate::args::Command;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Settings {
            source,
            library,
            keep_originals,
            in_place,
            staging,
            regions,
            threads,
            list_age_days,
            steamgriddb_key,
            igdb_client_id,
            igdb_client_secret,
            screenscraper_dev_id,
            screenscraper_dev_password,
            screenscraper_user,
            screenscraper_password,
        } => {
            let mut s = lib.settings()?;
            let mut changed = source.is_some()
                || library.is_some()
                || keep_originals.is_some()
                || in_place.is_some()
                || staging.is_some()
                || regions.is_some()
                || threads.is_some()
                || list_age_days.is_some();
            for (given, slot) in [
                (steamgriddb_key, &mut s.keys.steamgriddb_key),
                (igdb_client_id, &mut s.keys.igdb_client_id),
                (igdb_client_secret, &mut s.keys.igdb_client_secret),
                (screenscraper_dev_id, &mut s.keys.screenscraper_dev_id),
                (screenscraper_dev_password, &mut s.keys.screenscraper_dev_password),
                (screenscraper_user, &mut s.keys.screenscraper_user),
                (screenscraper_password, &mut s.keys.screenscraper_password),
            ] {
                if let Some(v) = given {
                    *slot = Some(v);
                    changed = true;
                }
            }
            let set = |p: String| (!p.is_empty()).then(|| PathBuf::from(p));
            if let Some(p) = source {
                s.source_dir = set(p);
            }
            if let Some(p) = library {
                s.library_dir = set(p);
            }
            if let Some(k) = keep_originals {
                s.keep_originals = k;
            }
            if let Some(k) = in_place {
                s.in_place = k;
            }
            if let Some(p) = staging {
                s.staging_dir = set(p);
            }
            if let Some(r) = regions {
                s.regions = r
                    .split(',')
                    .map(|r| r.trim().to_owned())
                    .filter(|r| !r.is_empty())
                    .collect();
            }
            if let Some(n) = threads {
                s.threads = n;
            }
            if let Some(d) = list_age_days {
                s.list_age_days = d;
            }
            if changed {
                s = lib.save_settings(&s)?;
            }
            let shown = |p: &Option<PathBuf>| p.as_ref().map_or("-".to_owned(), |p| p.display().to_string());
            println!("source          {}", shown(&s.source_dir));
            println!("library         {}", shown(&s.library_dir));
            println!("keep originals  {}", if s.keep_originals { "yes" } else { "no" });
            println!(
                "in place        {}",
                if s.in_place {
                    "yes (replace originals)"
                } else {
                    "no"
                }
            );
            println!("staging         {}", s.staging().display());
            println!("regions         {}", s.region_order().join(", "));
            println!(
                "list age        {}",
                match s.list_age_days {
                    0 => "never online on Refresh".to_owned(),
                    d => format!("{d} days"),
                }
            );
            println!(
                "threads         {}",
                match s.threads {
                    0 => format!("automatic ({})", romforge_core::parallel::threads(0)),
                    n => n.to_string(),
                }
            );
            let key = |v: &Option<String>| v.as_deref().map_or("-".to_owned(), settings::mask);
            println!("steamgriddb     {}", key(&s.keys.steamgriddb_key));
            println!(
                "igdb            {} / {}",
                key(&s.keys.igdb_client_id),
                key(&s.keys.igdb_client_secret)
            );
            println!(
                "screenscraper   dev {} / {}, user {}",
                key(&s.keys.screenscraper_dev_id),
                key(&s.keys.screenscraper_dev_password),
                key(&s.keys.screenscraper_user)
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}
