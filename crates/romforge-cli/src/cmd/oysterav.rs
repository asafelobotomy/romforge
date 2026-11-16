//! `oysterav`: ask oysterAV to look at what romforge keeps.
//!
//! oysterAV only looks. Nothing here moves, renames or deletes a file it
//! flags; what to do about one is chosen in oysterAV (Quarantine).

use std::path::Path;

use anyhow::{Result, bail};
use clap::Args;
use serde_json::json;

use romforge_core::oysterav::{self, State};
use romforge_core::{AvTarget, Library};

#[derive(Args)]
pub struct OysterAvArgs {
    /// What to look at: `emulators`, `games`, or a path. Without any, both
    /// `emulators` and `games`.
    pub targets: Vec<String>,
    /// One JSON line per event (`start`, `scanned`, `done`), as uQ Settings
    /// reads them
    #[arg(long)]
    pub ndjson: bool,
}

fn resolve(lib: &Library, asked: &[String]) -> Result<Vec<AvTarget>> {
    let known = lib.oysterav_targets()?;
    if asked.is_empty() {
        if known.is_empty() {
            bail!("no RomForge folder or Library folder to look at yet");
        }
        return Ok(known);
    }
    let mut out = Vec::new();
    for a in asked {
        let named: Vec<&AvTarget> = known.iter().filter(|t| &t.name == a).collect();
        if !named.is_empty() {
            out.extend(named.into_iter().cloned());
            continue;
        }
        let p = Path::new(a);
        if a == "emulators" || a == "games" {
            continue; // there is none of that yet: nothing to look at
        }
        let Ok(path) = p.canonicalize() else {
            bail!("{a:?} is not emulators, games or a place that exists");
        };
        out.push(AvTarget {
            name: path.display().to_string(),
            path,
        });
    }
    Ok(out)
}

fn emit(ndjson: bool, line: serde_json::Value, text: impl FnOnce() -> String) {
    if ndjson {
        println!("{line}");
    } else {
        println!("{}", text());
    }
}

/// Returns whether any threat was found (the exit status 1).
pub fn run(lib: &Library, args: OysterAvArgs) -> Result<bool> {
    let targets = resolve(lib, &args.targets)?;
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    emit(args.ndjson, json!({"event": "start", "targets": names}), || {
        format!("looking at {} with oysterAV", names.join(", "))
    });
    let (mut threats, mut errors) = (0usize, 0usize);
    for t in &targets {
        let v = oysterav::scan(&[t.path.as_path()]);
        match v.state {
            State::Threat => threats += v.findings.len().max(1),
            State::Error => errors += 1,
            State::Clean => {}
        }
        let findings: Vec<_> = v
            .findings
            .iter()
            .map(|f| json!({"path": f.path, "threat": f.threat}))
            .collect();
        emit(
            args.ndjson,
            json!({"event": "scanned", "target": t.name, "path": t.path, "state": v.state.as_str(),
                   "detail": v.detail, "findings": findings}),
            || match v.state {
                State::Clean => format!("{}: clean", t.name),
                State::Error => format!("{}: could not be checked: {}", t.name, v.detail),
                State::Threat => {
                    let mut s = format!("{}: THREAT {}", t.name, v.detail);
                    for f in &v.findings {
                        s += &format!("\n  {}  {}", f.path.display(), f.threat);
                    }
                    s
                }
            },
        );
    }
    emit(
        args.ndjson,
        json!({"event": "done", "clean": threats == 0 && errors == 0, "threats": threats, "errors": errors}),
        || match (threats, errors) {
            (0, 0) => "all clean".to_owned(),
            (t, e) => format!("{t} threat(s), {e} place(s) that could not be checked"),
        },
    );
    Ok(threats > 0)
}
