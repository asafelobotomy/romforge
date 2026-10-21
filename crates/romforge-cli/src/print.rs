//! Printing shared by several commands, and the progress meter.

use anyhow::Result;
use std::io::Write;
use std::time::{Duration, Instant};

use romforge_core::{ImportedDat, Library, ScanProgress, organize};

pub fn print_cleanup(plan: &romforge_core::cleanup::CleanupPlan, verbose: bool) {
    let what = if plan.keep_originals {
        "originals kept".to_owned()
    } else {
        format!("originals to {}", romforge_core::layout::TRASH_DIR)
    };
    println!("into {} ({what})", plan.dest.display());
    for it in &plan.items {
        if it.existing {
            println!(
                "  {}  already there; {} copy file(s) outside go to the trash",
                it.target.display(),
                it.copies.len()
            );
        } else if let Some(folder) = &it.folder {
            println!(
                "  {}  <-  the whole of {} ({} file(s))",
                it.target.display(),
                folder.display(),
                it.parts.len()
            );
        } else {
            println!("  {}  <-  {} file(s)", it.target.display(), it.parts.len());
        }
        if verbose {
            let from = |p: &romforge_core::cleanup::Part| {
                if p.member.is_empty() {
                    p.path.display().to_string()
                } else {
                    format!("{}#{}", p.path.display(), p.member)
                }
            };
            for p in &it.parts {
                println!("      {}  <-  {}", p.name, from(p));
            }
            if let Some(c) = &it.sheet {
                println!("      cue sheet (pregaps, if it names these tracks): {}", from(c));
            }
            for t in &it.trash {
                println!("      trash: {}", t.display());
            }
            for k in &it.kept {
                println!("      stays (holds other files): {}", k.display());
            }
        }
    }
    for (t, why) in &plan.skipped {
        println!("  skip {t}: {why}");
    }
}

pub fn print_plan(plan: &organize::Plan, verbose: bool) {
    for m in &plan.moves {
        println!("move  {}\n   -> {}", m.from.display(), m.to.display());
    }
    if verbose {
        for s in &plan.skipped {
            println!("stay  {}  ({})", s.path.display(), s.reason);
        }
    }
}

pub fn print_imported(d: ImportedDat) {
    let system = d.system.unwrap_or("no system (pass --system to set one)");
    let note = match (d.unchanged, d.replaced) {
        (true, _) => " (already imported)".to_owned(),
        (false, Some(old)) => format!(" (replaces DAT #{old})"),
        _ => String::new(),
    };
    println!(
        "DAT #{}: {} [{}] {} games, {} roms{}{}",
        d.dat_id,
        d.name,
        system,
        d.games,
        d.roms,
        d.header_hint.map(|h| format!(", header {h}")).unwrap_or_default(),
        note
    );
}

pub fn print_dats(lib: &Library) -> Result<()> {
    for d in lib.summary()?.dats {
        println!(
            "#{:<3} {:<55} {:<16} have {:>5} / {:<5} missing {:<5} imported {} from {}",
            d.dat_id,
            d.name,
            d.system.unwrap_or_else(|| "-".into()),
            d.have,
            d.games,
            d.missing,
            d.imported_at,
            d.source_path
        );
    }
    Ok(())
}

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// A one-line progress meter on stderr, redrawn at most every 200 ms.
pub struct Progress {
    enabled: bool,
    last: Option<Instant>,
    drawn: bool,
}

impl Progress {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last: None,
            drawn: false,
        }
    }

    pub fn update(&mut self, p: ScanProgress) {
        if !self.enabled
            || self
                .last
                .is_some_and(|t| t.elapsed() < Duration::from_millis(200))
        {
            return;
        }
        self.last = Some(Instant::now());
        self.drawn = true;
        let mut err = std::io::stderr().lock();
        let of = match p.units_total {
            0 => String::new(),
            n => format!("[{}/{n}] ", p.units_done),
        };
        let _ = write!(
            err,
            "\r\x1b[2K{of}hashed {} files, {}; skipped {}",
            p.hashed,
            bytes(p.bytes_hashed),
            p.skipped
        );
        let _ = err.flush();
    }

    pub fn finish(&mut self) {
        if self.drawn {
            eprint!("\r\x1b[2K");
        }
    }
}
