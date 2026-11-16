//! Looking at what romforge downloads, and at what it keeps, with oysterAV
//! (`oyst-cli scan <paths> --ndjson`).
//!
//! The contract is oysterAV's (`docs/cli/gui-contract.md`, "Apps that save
//! files"): progress lines, then one `{"event":"result","clean":…,
//! "findings":[{"path","threat_name"}],"pack_errors":[{"error"}]}`; exit 0
//! clean, 1 threats found, 3 another oysterAV job is running (ask again
//! later), anything else a failure. oysterAV only looks: nothing here moves,
//! renames or deletes a file it flagged (a download that is refused is
//! romforge's own half-finished file, never one of yours).
//!
//! The setting `scan_downloads` makes romforge look at each emulator
//! AppImage and core it downloads before it is put in place; a threat, or a
//! scan that could not finish, stops the install. `romforge-cli oysterav`
//! looks at what is already there, whatever the setting says.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::Error;

const DEFAULT_CLI: &str = "/usr/bin/oyst-cli";
/// oysterAV runs one job at a time; when it is busy, ask again this often,
/// this many times (about five minutes).
const BUSY_DELAY: Duration = Duration::from_secs(5);
const BUSY_TRIES: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Clean,
    /// oysterAV found something; the detail names it.
    Threat,
    /// The scan could not run or finish; the detail says why.
    Error,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Clean => "clean",
            State::Threat => "threat",
            State::Error => "error",
        }
    }
}

/// One file oysterAV flagged.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    pub path: PathBuf,
    pub threat: String,
}

/// What oysterAV said about a scan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Verdict {
    pub state: State,
    /// The threat names, or why the scan failed; empty when clean.
    pub detail: String,
    pub findings: Vec<Finding>,
}

impl Verdict {
    fn error(why: impl Into<String>) -> Verdict {
        Verdict {
            state: State::Error,
            detail: why.into(),
            findings: Vec::new(),
        }
    }
}

/// `$ROMFORGE_OYST_CLI` (for tests and odd installs), else `/usr/bin/oyst-cli`.
pub fn oyst_cli() -> PathBuf {
    std::env::var_os("ROMFORGE_OYST_CLI").map_or_else(|| DEFAULT_CLI.into(), PathBuf::from)
}

enum Answer {
    Done(Verdict),
    /// Another oysterAV job is running.
    Busy,
}

fn read_answer(code: Option<i32>, stdout: &str, stderr: &str) -> Answer {
    if code == Some(3) {
        return Answer::Busy;
    }
    let result = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .rfind(|v| v.get("event").and_then(Value::as_str) == Some("result"));
    let Some(result) = result else {
        let why = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("oysterAV gave no result");
        return Answer::Done(Verdict::error(why.trim().chars().take(300).collect::<String>()));
    };
    let mut findings = Vec::new();
    for f in result
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        findings.push(Finding {
            path: f.get("path").and_then(Value::as_str).unwrap_or_default().into(),
            threat: f
                .get("threat_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown threat")
                .to_owned(),
        });
    }
    if !findings.is_empty() {
        let mut names: Vec<&str> = Vec::new();
        for f in &findings {
            if !names.contains(&f.threat.as_str()) {
                names.push(&f.threat);
            }
        }
        let detail = names.join(", ");
        return Answer::Done(Verdict {
            state: State::Threat,
            detail,
            findings,
        });
    }
    let problem = result
        .get("pack_errors")
        .and_then(Value::as_array)
        .and_then(|e| e.first())
        .and_then(|e| e.get("error").and_then(Value::as_str));
    Answer::Done(
        match (problem, result.get("clean").and_then(Value::as_bool), code) {
            (Some(p), _, _) => Verdict::error(p.chars().take(300).collect::<String>()),
            (None, Some(true), Some(0)) => Verdict {
                state: State::Clean,
                detail: String::new(),
                findings: Vec::new(),
            },
            _ => Verdict::error("oysterAV could not finish the scan"),
        },
    )
}

/// Scan files or folders (absolute paths), waiting (up to about five
/// minutes) while oysterAV is busy with another job. This never fails: a
/// scan that could not run is the `Error` state.
pub fn scan(paths: &[&Path]) -> Verdict {
    scan_with(&oyst_cli(), paths, BUSY_DELAY, BUSY_TRIES)
}

pub fn scan_with(cli: &Path, paths: &[&Path], busy_delay: Duration, tries: u32) -> Verdict {
    if paths.iter().any(|p| !p.is_absolute()) {
        return Verdict::error("the place to scan is not known");
    }
    if let Some(p) = paths.iter().find(|p| !p.exists()) {
        return Verdict::error(format!("{} is not there", p.display()));
    }
    if !cli.exists() {
        return Verdict::error("oysterAV is not installed (oyst-cli not found)");
    }
    let tries = tries.max(1);
    for attempt in 0..tries {
        let out = match Command::new(cli)
            .args(["scan", "--ndjson"])
            .args(paths)
            .stdin(Stdio::null())
            .output()
        {
            Ok(o) => o,
            Err(e) => return Verdict::error(format!("could not start oysterAV: {e}")),
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        match read_answer(out.status.code(), &stdout, &stderr) {
            Answer::Done(v) => return v,
            Answer::Busy if attempt + 1 < tries => std::thread::sleep(busy_delay),
            Answer::Busy => {}
        }
    }
    Verdict::error("oysterAV stayed busy with another job")
}

/// Look at a file romforge has just downloaded, before it is put in
/// place: `Ok` only when oysterAV says it is clean. A threat or a scan that
/// could not finish is an error that says so, and the caller removes the
/// file (its own half-finished download).
pub fn check_download(file: &Path, label: &str) -> Result<(), Error> {
    let v = scan(&[file]);
    match v.state {
        State::Clean => Ok(()),
        State::Threat => Err(Error::Antivirus(format!(
            "oysterAV found {} in {label}; it was not installed",
            v.detail
        ))),
        State::Error => Err(Error::Antivirus(format!(
            "{label} could not be checked ({}); it was not installed. \
             Turn off \"Check downloads with oysterAV\" to install without checking",
            v.detail
        ))),
    }
}

#[cfg(test)]
#[path = "oysterav/tests.rs"]
mod tests;
