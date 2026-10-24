//! `dat`, `databases` and `scummvm`.

use anyhow::{Context, Result, bail};

use romforge_core::{DatState, ImportedDat, Library, extdb, fetch};

use crate::args::Command;
use crate::print::{print_dats, print_imported};
use crate::subcommands::{DatCommand, DatabasesCommand, ScummvmCommand};

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Dat(DatCommand::Mame) => {
            eprintln!("asking MAME for its list (-listxml); this takes a minute…");
            let env = lib.env()?;
            print_imported(lib.mame_dat(&env)?);
        }
        Command::Dat(DatCommand::Import { files, system }) => {
            lib.batch(|lib| -> Result<()> {
                for file in files {
                    let dats = lib
                        .import_dats(&file, system.as_deref())
                        .with_context(|| format!("importing {}", file.display()))?;
                    dats.into_iter().for_each(print_imported);
                }
                Ok(())
            })?;
        }
        Command::Dat(DatCommand::Download { systems, all, extras }) => {
            let mut failed = 0;
            let mut report =
                |key: &str, source: fetch::Source, result: Result<Vec<ImportedDat>, romforge_core::Error>| {
                    match result {
                        Ok(_) if source.is_list() => {
                            println!("{key}: {source}: stored")
                        }
                        Ok(dats) if dats.is_empty() => println!("{key}: {source}: already imported"),
                        Ok(dats) => dats.into_iter().for_each(print_imported),
                        Err(e) => {
                            failed += 1;
                            eprintln!("error: {key}: {source}: {e}");
                        }
                    }
                };
            if all {
                lib.download_all(extras, &mut report)?;
            } else {
                lib.batch(|lib| -> Result<()> {
                    for key in &systems {
                        lib.download_dats(key, extras, |source, r| report(key, source, r))
                            .with_context(|| format!("downloading DATs for {key}"))?;
                    }
                    Ok(())
                })?;
            }
            if failed > 0 {
                bail!("{failed} DAT downloads failed");
            }
        }
        Command::Dat(DatCommand::Check { all }) => {
            let profile = fetch::nointro_profile()?;
            let status = lib.nointro_status(&profile)?;
            let mut missing = 0;
            for s in &status {
                let line = match s.state {
                    DatState::Missing => {
                        missing += 1;
                        if !all {
                            continue;
                        }
                        format!("missing   {:<60} latest {}", s.name, s.latest)
                    }
                    DatState::Current => format!("current   {:<60} {}", s.name, s.latest),
                    DatState::Mirror => format!(
                        "mirror    {:<60} libretro {} (No-Intro {})",
                        s.name,
                        s.imported.as_deref().unwrap_or("?"),
                        s.latest
                    ),
                    DatState::Outdated => format!(
                        "outdated  {:<60} have {} latest {}",
                        s.name,
                        s.imported.as_deref().unwrap_or("?"),
                        s.latest
                    ),
                };
                println!("{line}");
            }
            if missing > 0 && !all {
                println!("{missing} No-Intro DATs for known systems not imported (--all lists them)");
            }
            println!(
                "`dat download` refreshes mirror DATs; others come from {}",
                fetch::NOINTRO_DOWNLOAD_PAGE
            );
        }
        Command::Dat(DatCommand::List) => print_dats(lib)?,
        Command::Dat(DatCommand::Systems) => unreachable!(),
        Command::Databases(cmd) => {
            let dir = lib.extdb_dir().context("no folder for databases")?;
            let pick = |n: &str| {
                extdb::ExtDb::parse(n)
                    .with_context(|| format!("unknown database {n:?} (openvgdb or launchbox)"))
            };
            match cmd {
                DatabasesCommand::List => {
                    for i in extdb::list(&dir) {
                        let state = match (&i.downloaded_at, i.bytes) {
                            (Some(at), Some(b)) => {
                                format!(
                                    "downloaded {at}, {} MB, {} entries",
                                    b >> 20,
                                    i.entries.unwrap_or(0)
                                )
                            }
                            _ => "not downloaded".into(),
                        };
                        println!("{:<10} {:<26} {state}", i.db.key(), i.name);
                        println!("           {}", i.about);
                    }
                    println!("(in {})", dir.display());
                }
                DatabasesCommand::Download { name } => {
                    let db = pick(&name)?;
                    let i = extdb::download(db, &dir, &extdb::Bases::default(), |m| eprintln!("{m}"))?;
                    println!("{}: {} entries", i.name, i.entries.unwrap_or(0));
                }
                DatabasesCommand::Import { name, zip } => {
                    let db = pick(&name)?;
                    let i = extdb::import(db, &zip, &dir, &zip.to_string_lossy(), |m| eprintln!("{m}"))?;
                    println!("{}: {} entries", i.name, i.entries.unwrap_or(0));
                }
                DatabasesCommand::Remove { name, yes } => {
                    if !yes {
                        bail!("refusing to remove {name} without --yes");
                    }
                    let db = pick(&name)?;
                    extdb::remove(db, &dir)?;
                    println!("{} removed", db.name());
                }
            }
        }
        Command::Scummvm(cmd) => {
            // libretro's names for ScummVM games (its box art goes by them);
            // without them, ScummVM's own names stay.
            match lib.scummvm_titles_download() {
                Ok(n) => println!("libretro's ScummVM.dat: names for {n} game ids"),
                Err(e) => eprintln!("warning: libretro's ScummVM.dat: {e} (ScummVM's own names are used)"),
            }
            let r = match cmd {
                ScummvmCommand::Read => {
                    let env = lib.env()?;
                    lib.scummvm_read(&env)?
                }
                ScummvmCommand::Import { file } => {
                    let bytes =
                        std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
                    lib.scummvm_import(&bytes)?
                }
            };
            println!(
                "ScummVM list: {} entries; ScummVM itself named {} folder(s); {} ScummVM game(s) in your folders",
                r.entries, r.asked, r.games
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}
