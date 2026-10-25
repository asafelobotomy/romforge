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

use std::path::{Path, PathBuf};

use crate::home::Home;
use crate::launch::{self, Env, KNOWN_EMULATORS, RETROARCH};
use crate::{Error, systems};

mod data;
pub mod gate;
pub use data::{FileReq, Member, Profile};
mod check;
mod import;
use check::{Checker, digest};
mod index;
pub(crate) use index::{BiosIndex, index};
mod journal;
pub use journal::undo;
pub mod find;
mod members;
mod originals;
pub use import::*;
mod ready;
use ready::{is_chip, same_core};
mod walk;
pub use walk::SourceId;

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
    /// A BIOS zip with some of its members, not all (`FileState::missing`).
    Incomplete,
    /// A file whose name fits but whose hash isn't known, placed because you
    /// picked it: yours, never "accepted".
    PlacedByYou,
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
    /// Needed only by some of the system's games, so it doesn't decide
    /// whether the emulator is ready, and which, to show as it is: those of
    /// a sub-system (`systems::System::sub_systems`: "Satellaview games"),
    /// or "games with that chip" for a cartridge's coprocessor ROM.
    pub only_for: Option<String>,
    pub state: State,
    /// For an incomplete BIOS zip: the members it lacks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
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
    /// The library's badges: systems whose emulator Play would use lacks a
    /// required file.
    pub needs: Vec<BiosNeed>,
}

/// A system that can't start its games for want of a BIOS file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BiosNeed {
    pub system: &'static str,
    /// The emulator Play would start (as the app names it).
    pub emulator: String,
}

/// The systems whose first installed core or emulator (the one Play tries
/// first: a core when RetroArch has one, else a standalone emulator, as
/// `launch` orders them) is in RetroBIOS's list and not ready.
fn needs(systems: &[SystemFirmware]) -> Vec<BiosNeed> {
    systems
        .iter()
        .filter_map(|s| {
            let e = s.emulators.iter().find(|e| e.installed)?;
            (e.profiled && !e.ready).then(|| BiosNeed {
                system: s.key,
                emulator: e.label.clone(),
            })
        })
        .collect()
}

impl FirmwareReport {
    /// Only `system` (as the systems table names it) and only the emulator
    /// or core `emulator` (`retroarch:<core>`, or as `emulator list` names
    /// it), where given; systems left with nothing go.
    pub fn only(mut self, system: Option<&str>, emulator: Option<&str>) -> FirmwareReport {
        self.systems
            .retain(|s| system.is_none_or(|k| s.key.eq_ignore_ascii_case(k)));
        if let Some(e) = emulator {
            for s in &mut self.systems {
                s.emulators.retain(|x| x.name.eq_ignore_ascii_case(e));
            }
            self.systems.retain(|s| !s.emulators.is_empty());
        }
        self.needs = needs(&self.systems);
        self
    }
}

/// Fetch RetroBIOS's emulator list into `dir`. Returns how many profiles
/// it has.
pub fn download(dir: &Path) -> Result<usize, Error> {
    let bytes = crate::fetch::get(URL)?;
    let (profiles, _) = data::parse(&bytes)?;
    crate::atomic::write(&dir.join(FILE), &bytes)?;
    Ok(profiles.len())
}

/// The list kept in `dir`, read (and when RetroBIOS made it).
fn load(dir: &Path) -> Result<(Vec<Profile>, String), Error> {
    let bytes = std::fs::read(dir.join(FILE)).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::Launch("no BIOS list yet: download RetroBIOS's list first (`firmware download`)".into())
        }
        _ => e.into(),
    })?;
    data::parse(&bytes)
}

/// One core or emulator of a system: its profile, if RetroBIOS has one,
/// and the folder it finds BIOS files in.
struct Slot<'a> {
    system: &'static systems::System,
    /// `retroarch:<core>` or the emulator's name.
    name: String,
    profile: Option<&'a Profile>,
    dir: PathBuf,
    installed: bool,
    /// For a MAME machine (`KnownEmulator::machine`): its own set, when the
    /// list gives none for it.
    machine: Vec<FileReq>,
}

impl Slot<'_> {
    /// How the app names it.
    fn label(&self) -> String {
        match (self.name.strip_prefix("retroarch:"), self.profile) {
            (Some(_), Some(p)) => format!("RetroArch: {}", p.display_name),
            (Some(core), None) => format!("RetroArch: {core}"),
            (None, _) => launch::known(&self.name)
                .map_or(self.name.as_str(), |e| e.label)
                .to_owned(),
        }
    }

    /// Where it finds `f`: the standalone emulator's own place for it when
    /// the list gives one, else its place in RetroArch's layout.
    fn place(&self, f: &FileReq) -> PathBuf {
        let standalone = f
            .standalone_path
            .as_deref()
            .filter(|_| !self.name.starts_with("retroarch:"));
        self.dir.join(standalone.unwrap_or(f.place()))
    }

    /// Its profile's files for its system: those tied to the system, and
    /// those tied to none. A MAME machine gets only its own: MAME's profile
    /// covers every machine, and its untied files are other machines' BIOS
    /// sets and MAME's support files.
    fn files(&self) -> impl Iterator<Item = &FileReq> {
        let ids = self.system.retrobios;
        let tied_only = self.is_machine();
        self.profile
            .into_iter()
            .flat_map(|p| &p.files)
            .filter(move |f| match f.system.as_deref() {
                Some(sys) => ids.contains(&sys),
                None => !tied_only,
            })
            .chain(&self.machine)
    }

    fn is_machine(&self) -> bool {
        launch::known(&self.name).is_some_and(|e| e.machine.is_some())
    }

    /// What `f` is needed only for ([`FileState::only_for`]): the games of a
    /// sub-system the systems table names, or those with a chip.
    fn only_for(&self, f: &FileReq) -> Option<String> {
        if is_chip(&f.name) {
            return Some("games with that chip".into());
        }
        let sys = f.system.as_deref()?;
        self.system
            .sub_systems
            .iter()
            .find(|(id, _)| *id == sys)
            .map(|(_, label)| format!("{label} games"))
    }
}

/// Every system's cores and emulators (arcade systems aside).
fn slots<'a>(profiles: &'a [Profile], home: &Home, env: &Env) -> Vec<Slot<'a>> {
    let mut out = Vec::new();
    for s in systems::SYSTEMS.iter().filter(|s| !s.arcade) {
        for core in s.cores {
            out.push(Slot {
                system: s,
                name: format!("retroarch:{core}"),
                profile: profiles
                    .iter()
                    .find(|p| p.cores.iter().any(|c| same_core(c, core)))
                    .or_else(|| profiles.iter().find(|p| p.id == *core)),
                dir: home.bios().join("RetroArch"),
                installed: env.find(&RETROARCH).is_some() && env.core_path(core).is_some(),
                machine: Vec::new(),
            });
        }
        for name in s.emulators {
            let Some(e) = KNOWN_EMULATORS.iter().find(|e| e.name == *name) else {
                continue;
            };
            let id = e.settings_name();
            out.push(Slot {
                system: s,
                name: e.name.to_owned(),
                profile: profiles
                    .iter()
                    .find(|p| p.id == e.firmware.unwrap_or(id))
                    .or_else(|| profiles.iter().find(|p| p.cores.iter().any(|c| c == id))),
                dir: home.bios().join(id),
                installed: env.find(e).is_some(),
                machine: Vec::new(),
            });
            if let (Some(m), Some(slot)) = (e.machine, out.last_mut()) {
                let set = format!("{m}.zip");
                if !slot.files().any(|f| f.name == set) {
                    slot.machine.push(FileReq {
                        name: set,
                        required: true,
                        archive: true,
                        ..FileReq::default()
                    });
                }
            }
        }
    }
    out
}

/// Check the RomForge folder's `Bios/` against the list in `dir`; `conn`
/// says which name-only files you placed yourself.
pub fn report(
    conn: &rusqlite::Connection,
    dir: &Path,
    home: &Home,
    env: &Env,
) -> Result<FirmwareReport, Error> {
    // The library's BIOS rows follow the list (step 6); a database busy
    // with a scan only leaves them for the next time.
    let _ = index(conn, dir);
    let (profiles, generated_at) = load(dir)?;
    let mut check = Checker::with_picked(journal::picked(conn)?);
    let mut out: Vec<SystemFirmware> = Vec::new();
    for slot in slots(&profiles, home, env) {
        let e = check.emulator(&slot);
        match out.last_mut().filter(|s| s.key == slot.system.key) {
            Some(s) => s.emulators.push(e),
            None => out.push(SystemFirmware {
                key: slot.system.key,
                name: slot.system.name,
                emulators: vec![e],
            }),
        }
    }
    Ok(FirmwareReport {
        generated_at,
        bios_dir: home.bios(),
        needs: needs(&out),
        systems: out,
    })
}

#[cfg(test)]
mod tests;
