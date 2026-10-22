//! `romforge-cli themes`: the fullscreen mode's themes, and the community
//! index (romforge_core::themes::remote).

use std::path::Path;

use anyhow::{Context, Result, bail};
use romforge_core::Library;
use romforge_core::themes::remote::{self, Source, Status};

use crate::subcommands::ThemesCommand;

pub fn run(lib: &Library, command: ThemesCommand) -> Result<()> {
    let dir = lib.themes_dir();
    let source = |index: Option<String>| index.map_or_else(Source::from_env, |i| Source::parse(&i));
    match command {
        ThemesCommand::List => {
            for e in lib.themes() {
                let name = e.manifest.as_ref().map_or(e.id.as_str(), |m| m.name.as_str());
                let from = if e.builtin {
                    "built in".to_owned()
                } else if e.downloaded {
                    "downloaded".to_owned()
                } else {
                    "yours".to_owned()
                };
                match &e.problem {
                    None => println!("{:<16} {name} ({from})", e.id),
                    Some(p) => println!("{:<16} refused ({from}): {p}", e.id),
                }
            }
            if let Some(d) = &dir {
                println!("your themes: {}", d.display());
            }
        }
        ThemesCommand::Available { index } => {
            let source = source(index);
            let idx = source
                .index()
                .with_context(|| format!("reading {}", source.describe()))?;
            for r in &idx.themes {
                let state = match remote::status(dir.as_deref(), r) {
                    Status::NotInstalled => "not installed".to_owned(),
                    Status::Installed { version } => format!("installed ({version})"),
                    Status::UpdateAvailable { installed } => format!("update: {installed} → {}", r.version),
                    Status::Taken => "a theme of yours has this name".to_owned(),
                    Status::Refused { problem } => format!("refused: {problem}"),
                };
                println!(
                    "{:<16} {} {} by {}, {} ({} KB): {state}",
                    r.id,
                    r.name,
                    r.version,
                    if r.author.is_empty() { "?" } else { &r.author },
                    r.licence,
                    r.size().div_ceil(1024)
                );
            }
        }
        ThemesCommand::Install {
            id,
            accept_licence,
            index,
        } => {
            let dir = dir.context("no themes folder (no library on disk)")?;
            let source = source(index);
            let idx = source
                .index()
                .with_context(|| format!("reading {}", source.describe()))?;
            let Some(r) = idx.themes.iter().find(|r| r.id == id) else {
                bail!("{id:?} isn't in {}", source.describe());
            };
            println!(
                "{} {} by {}",
                r.name,
                r.version,
                if r.author.is_empty() { "?" } else { &r.author }
            );
            println!(
                "licence: {}{}",
                r.licence,
                if r.licence_url.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", r.licence_url)
                }
            );
            if !accept_licence {
                bail!("read its licence, then run again with --accept-licence");
            }
            remote::install(&dir, &source, r, |done, total| {
                eprint!("\r{} / {} KB", done.div_ceil(1024), total.div_ceil(1024));
            })?;
            eprintln!();
            println!("installed in {}", dir.join(&r.id).display());
        }
        ThemesCommand::Remove { id } => {
            let dir = dir.context("no themes folder")?;
            remote::remove(&dir, &id)?;
            println!("removed {id}");
        }
        ThemesCommand::Index { .. } => unreachable!("handled before the library opens"),
    }
    Ok(())
}

/// Write (or check) `repo/themes.json`.
pub fn index(repo: &Path, check: bool) -> Result<()> {
    let idx = remote::build_index(repo).map_err(|e| anyhow::anyhow!("{e}"))?;
    let text = remote::index_text(&idx);
    let path = repo.join("themes.json");
    if check {
        let now = std::fs::read_to_string(&path).unwrap_or_default();
        if now != text {
            bail!(
                "{} is out of date: run `romforge-cli themes index {}`",
                path.display(),
                repo.display()
            );
        }
        println!("{} themes, all usable, themes.json up to date", idx.themes.len());
    } else {
        std::fs::write(&path, text)?;
        println!("wrote {} ({} themes)", path.display(), idx.themes.len());
    }
    Ok(())
}
