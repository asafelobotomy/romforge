//! Milestone 20: which BIOS files each system's emulators need, and which
//! you have.
//!
//! What emulators need comes from RetroBIOS (abdess.github.io/retrobios):
//! its emulator profiles, read from each emulator's source, are MIT and
//! served with no account or key. Only that list is fetched; romforge
//! never downloads, links or ships BIOS files themselves (house rules 7
//! and 10). Your files are checked where each emulator finds them in the
//! RomForge folder: `Bios/RetroArch/` for RetroArch's cores (its system
//! folder, milestone 18, 1.3), `Bios/<emulator>/` for the others.
//!
//! A file is *accepted* when its size and SHA1 or MD5 are ones the
//! emulator's code accepts (house rule 8). That says the emulator takes
//! it, not that it is a verified dump, so it is never counted as
//! DAT-verified. A file with no hash in the list can only be *present*.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::home::Home;
use crate::launch::{self, Env, KNOWN_EMULATORS, RETROARCH};
use crate::{Error, systems};

mod data;
pub use data::{FileReq, Profile};

/// RetroBIOS's emulator profiles.
pub const URL: &str = "https://abdess.github.io/retrobios/api/v1/emulators.json";
/// Where the list is kept, in the downloaded databases' folder.
pub const FILE: &str = "retrobios-emulators.json";

/// What one file's check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Size and SHA1 or MD5 are ones the emulator accepts.
    Accepted,
    /// There, but the list gives no SHA1 or MD5 to check it by.
    Present,
    /// There, but neither its size nor its hashes are ones accepted.
    WrongHash,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileState {
    pub name: String,
    /// Where it goes.
    pub place: PathBuf,
    pub required: bool,
    /// The emulator runs without it.
    pub hle: bool,
    pub archive: bool,
    /// Loaded only for games of these regions; empty: all.
    pub regions: Vec<String>,
    pub state: State,
}

/// One emulator or core for a system.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EmulatorFirmware {
    /// `retroarch:<core>` or the emulator's name, as `emulator list` says.
    pub name: String,
    pub label: String,
    pub installed: bool,
    /// RetroBIOS has a profile for it.
    pub profiled: bool,
    pub files: Vec<FileState>,
    /// It can start games: every required file it can't run without is
    /// accepted or present (one of a group of alternatives, and one region's
    /// file where the files are regional).
    pub ready: bool,
    /// Where required files are regional: the regions whose games it can
    /// start.
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SystemFirmware {
    pub key: &'static str,
    pub name: &'static str,
    pub emulators: Vec<EmulatorFirmware>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FirmwareReport {
    /// When RetroBIOS generated the list.
    pub generated_at: String,
    pub bios_dir: PathBuf,
    pub systems: Vec<SystemFirmware>,
}

/// Fetch RetroBIOS's emulator list into `dir`. Returns how many profiles
/// it has.
pub fn download(dir: &Path) -> Result<usize, Error> {
    let bytes = crate::fetch::get(URL)?;
    let (profiles, _) = data::parse(&bytes)?;
    crate::atomic::write(&dir.join(FILE), &bytes)?;
    Ok(profiles.len())
}

/// Check the RomForge folder's `Bios/` against the list in `dir`.
pub fn report(dir: &Path, home: &Home, env: &Env) -> Result<FirmwareReport, Error> {
    let bytes = std::fs::read(dir.join(FILE)).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::Launch("no BIOS list yet: download RetroBIOS's list first (`firmware download`)".into())
        }
        _ => e.into(),
    })?;
    let (profiles, generated_at) = data::parse(&bytes)?;
    let mut check = Checker::default();
    let mut out = Vec::new();
    for s in systems::SYSTEMS.iter().filter(|s| !s.arcade) {
        let mut emulators = Vec::new();
        for core in s.cores {
            let p = profiles.iter().find(|p| p.cores.iter().any(|c| c == core));
            let dir = home.bios().join("RetroArch");
            let installed = env.find(&RETROARCH).is_some() && env.core_installed(core);
            emulators.push(check.emulator(&format!("retroarch:{core}"), p, s, &dir, installed));
        }
        for name in s.emulators {
            let Some(e) = KNOWN_EMULATORS.iter().find(|e| e.name == *name) else {
                continue;
            };
            let id = e.settings_name();
            let p = profiles
                .iter()
                .find(|p| p.id == id)
                .or_else(|| profiles.iter().find(|p| p.cores.iter().any(|c| c == id)));
            let dir = home.bios().join(id);
            emulators.push(check.emulator(e.name, p, s, &dir, env.find(e).is_some()));
        }
        if !emulators.is_empty() {
            out.push(SystemFirmware {
                key: s.key,
                name: s.name,
                emulators,
            });
        }
    }
    Ok(FirmwareReport {
        generated_at,
        bios_dir: home.bios(),
        systems: out,
    })
}

/// Hashes of the files looked at, so one file needed by many cores is read
/// once.
#[derive(Default)]
struct Checker {
    seen: HashMap<PathBuf, Option<crate::hash::Digest>>,
}

impl Checker {
    fn emulator(
        &mut self,
        name: &str,
        p: Option<&Profile>,
        s: &systems::System,
        dir: &Path,
        installed: bool,
    ) -> EmulatorFirmware {
        let for_system = |f: &&FileReq| f.system.as_deref().is_none_or(|sys| s.retrobios.contains(&sys));
        let files: Vec<FileState> = p
            .map(|p| {
                p.files
                    .iter()
                    .filter(for_system)
                    .map(|f| self.file(f, dir))
                    .collect()
            })
            .unwrap_or_default();
        let reqs: Vec<&FileReq> = p
            .map(|p| p.files.iter().filter(for_system).collect())
            .unwrap_or_default();
        let (ready, regions) = readiness(&reqs, &files);
        let label = match (name.strip_prefix("retroarch:"), p) {
            (Some(_), Some(p)) => format!("RetroArch: {}", p.display_name),
            (Some(core), None) => format!("RetroArch: {core}"),
            (None, _) => launch::known(name).map_or(name, |e| e.label).to_owned(),
        };
        EmulatorFirmware {
            name: name.to_owned(),
            label,
            installed,
            profiled: p.is_some(),
            files,
            ready,
            regions,
        }
    }

    fn file(&mut self, f: &FileReq, dir: &Path) -> FileState {
        let place = dir.join(f.place());
        let digest = self
            .seen
            .entry(place.clone())
            .or_insert_with(|| digest(&place))
            .clone();
        let state = match digest {
            None => State::Missing,
            Some(d) => {
                let size_ok = f.size.is_none_or(|s| s == d.size);
                let hashed = !f.sha1.is_empty() || !f.md5.is_empty();
                if size_ok && (f.sha1.contains(&d.sha1) || f.md5.contains(&d.md5)) {
                    State::Accepted
                } else if size_ok && !hashed {
                    State::Present
                } else {
                    State::WrongHash
                }
            }
        };
        FileState {
            name: f.name.clone(),
            place,
            required: f.required,
            hle: f.hle,
            archive: f.archive,
            regions: f.regions.clone(),
            state,
        }
    }
}

/// Whether an emulator can start games with `files` (checked, in the order
/// of `reqs`), and for which regions when its required files are regional.
fn readiness(reqs: &[&FileReq], files: &[FileState]) -> (bool, Vec<String>) {
    let ok = |f: &FileState| matches!(f.state, State::Accepted | State::Present);
    let needed = |r: &&FileReq| r.required && !r.hle;
    let pairs: Vec<(&FileReq, &FileState)> = reqs
        .iter()
        .copied()
        .zip(files)
        .filter(|(r, _)| needed(r))
        .collect();
    // A group of alternatives needs one; so does a set of regional files.
    let mut groups: HashMap<&str, bool> = HashMap::new();
    let mut regions = Vec::new();
    let mut regional = false;
    let mut plain = true;
    for (r, f) in &pairs {
        if let Some(g) = &r.variant_group {
            *groups.entry(g).or_default() |= ok(f);
        } else if !r.regions.is_empty() {
            regional = true;
            if ok(f) {
                regions.extend(r.regions.iter().cloned());
            }
        } else {
            plain &= ok(f);
        }
    }
    regions.sort();
    regions.dedup();
    let ready = plain && groups.values().all(|g| *g) && (!regional || !regions.is_empty());
    (ready, regions)
}

/// A plain file's size and hashes (a folder or nothing: `None`).
fn digest(path: &Path) -> Option<crate::hash::Digest> {
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok().filter(|m| m.is_file())?;
    crate::hash::hash_stream(std::io::BufReader::new(file), meta.len())
        .ok()
        .map(|h| h.raw)
}

#[cfg(test)]
mod tests;
