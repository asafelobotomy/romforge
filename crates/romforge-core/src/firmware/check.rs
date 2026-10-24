//! Checking each file at its place: hashed once per run, a BIOS zip by its
//! members, a file you placed yourself kept as yours.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::ready::readiness;
use super::{EmulatorFirmware, FileReq, FileState, Slot, State, members};

/// Hashes of the files looked at, so one file needed by many cores is read
/// once.
#[derive(Default)]
pub(super) struct Checker {
    seen: HashMap<PathBuf, Option<crate::hash::Digest>>,
    zips: HashMap<PathBuf, Option<Vec<(String, crate::hash::Digest)>>>,
    /// Places holding a name-only file you picked (a BIOS import not undone).
    picked: std::collections::HashSet<PathBuf>,
}

impl Checker {
    pub(super) fn with_picked(picked: std::collections::HashSet<PathBuf>) -> Checker {
        Checker {
            picked,
            ..Checker::default()
        }
    }

    pub(super) fn emulator(&mut self, slot: &Slot) -> EmulatorFirmware {
        let reqs: Vec<&FileReq> = slot.files().collect();
        let files: Vec<FileState> = reqs
            .iter()
            .map(|f| FileState {
                only_for: slot.only_for(f),
                ..self.file(f, slot.place(f))
            })
            .collect();
        let (ready, regions) = readiness(&reqs, &files);
        EmulatorFirmware {
            name: slot.name.clone(),
            label: slot.label(),
            installed: slot.installed,
            profiled: slot.profile.is_some(),
            files,
            ready,
            regions,
        }
    }

    fn file(&mut self, f: &FileReq, place: PathBuf) -> FileState {
        let mut missing = Vec::new();
        if !f.contents.is_empty() {
            let members = self
                .zips
                .entry(place.clone())
                .or_insert_with(|| members::zip_members(&place))
                .clone();
            let state = match members {
                None if place.exists() => State::WrongHash,
                None => State::Missing,
                Some(m) => {
                    let (state, lacks) = members::check(&f.contents, &m);
                    missing = lacks;
                    state
                }
            };
            return self.state_of(f, place, state, missing);
        }
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
        self.state_of(f, place, state, missing)
    }

    fn state_of(&self, f: &FileReq, place: PathBuf, state: State, missing: Vec<String>) -> FileState {
        // A file you placed yourself stays yours, whatever its hash says.
        let state = match state {
            State::Present | State::WrongHash | State::Incomplete if self.picked.contains(&place) => {
                State::PlacedByYou
            }
            s => s,
        };
        FileState {
            name: f.name.clone(),
            place,
            required: f.required,
            hle: f.hle,
            archive: f.archive,
            regions: f.regions.clone(),
            only_for: None,
            state,
            missing,
        }
    }
}

/// A plain file's size and hashes (a folder or nothing: `None`).
pub(super) fn digest(path: &Path) -> Option<crate::hash::Digest> {
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok().filter(|m| m.is_file())?;
    crate::hash::hash_stream(std::io::BufReader::new(file), meta.len())
        .ok()
        .map(|h| h.raw)
}
