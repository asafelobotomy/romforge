//! Milestone 20, step 3: BIOS files the user drops (a folder, a file, a
//! zip or 7z of any of them) go where each emulator finds them.
//!
//! Every file is matched by its contents, never its name: size + SHA1 or
//! MD5 against what each core or emulator accepts (house rule 8), so a pack
//! that renamed everything still works. A file only whose name fits is
//! offered, not placed, unless the user picks it; so is an archive
//! dropped whole that has the name of a set an emulator loads as it is
//! (MAME's `fm7.zip`, whose zip hashes no list can give). Nothing is written by
//! [`plan`]; [`apply`] copies (never moves: the drop stays as it was),
//! writes each file once and hard-links it to its other places, re-hashes
//! what it wrote, never replaces a file, and journals every file it makes
//! so [`undo`] takes them away (house rule 6).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::walk::{SourceId, walk};
use super::{FileReq, Slot, digest, load, slots};
use crate::Error;
use crate::home::Home;
use crate::launch::Env;

/// How a dropped file matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Match {
    /// Its size and hash are ones an emulator accepts.
    Accepted,
    /// Only its name is one an emulator loads: placed only if picked.
    NameOnly,
    Unknown,
}

/// What is at a place already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    /// Nothing: the file goes there.
    New,
    /// The same file.
    AlreadyThere,
    /// Another file, left as it is (nothing is ever replaced).
    Taken,
    /// Another dropped file goes there.
    Claimed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImportTarget {
    pub place: PathBuf,
    /// The cores and emulators that load it from there.
    pub used_by: Vec<String>,
    pub state: TargetState,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImportFile {
    pub source: SourceId,
    /// As the user sees it (`pack.zip › bios/scph5501.bin`).
    pub shown: String,
    pub name: String,
    pub size: u64,
    pub sha1: String,
    pub matched: Match,
    pub targets: Vec<ImportTarget>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportPlan {
    pub files: Vec<ImportFile>,
    /// Parts that couldn't be read, and why.
    pub skipped: Vec<(String, String)>,
    /// Places an apply would fill.
    pub new_places: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Imported {
    /// Files written.
    pub written: usize,
    /// Originals moved to the trash (Settings: not leaving BIOS files where
    /// they are).
    #[serde(default)]
    pub moved: usize,
    /// Further places given a hard link (or a copy) of one.
    pub linked: usize,
    pub batch: Option<i64>,
}

/// One place an emulator loads a file from: what it accepts there and who
/// loads it.
struct Place<'a> {
    reqs: Vec<&'a FileReq>,
    used_by: Vec<String>,
}

fn places<'a>(slots: &'a [Slot<'a>]) -> BTreeMap<PathBuf, Place<'a>> {
    let mut out: BTreeMap<PathBuf, Place> = BTreeMap::new();
    for slot in slots {
        for f in slot.files() {
            let p = out.entry(slot.place(f)).or_insert(Place {
                reqs: Vec::new(),
                used_by: Vec::new(),
            });
            p.reqs.push(f);
            let label = slot.label();
            if !p.used_by.contains(&label) {
                p.used_by.push(label);
            }
        }
    }
    out
}

/// A zip, which is offered whole as well as by its members.
fn is_zip(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

/// The archives emulators load as they are (a MAME machine's `fm7.zip`): a
/// dropped one of these names is also offered whole, not only its members.
fn sets(places: &BTreeMap<PathBuf, Place>) -> HashSet<String> {
    places
        .values()
        .flat_map(|p| &p.reqs)
        .filter(|f| f.archive)
        .map(|f| f.name.to_ascii_lowercase())
        .collect()
}

fn accepts(f: &FileReq, d: &crate::hash::Digest) -> bool {
    f.size.is_none_or(|s| s == d.size) && (f.sha1.contains(&d.sha1) || f.md5.contains(&d.md5))
}

/// What dropping `paths` would do.
pub fn plan(dir: &Path, home: &Home, env: &Env, paths: &[PathBuf]) -> Result<ImportPlan, Error> {
    let (profiles, _) = load(dir)?;
    let slots = slots(&profiles, home, env);
    let places = places(&slots);
    let mut out = ImportPlan::default();
    let mut claimed: HashSet<PathBuf> = HashSet::new();
    let mut found = Vec::new();
    let sets = sets(&places);
    walk(
        paths,
        &|name| is_zip(name),
        &mut |id, r, size| {
            let d = crate::hash::hash_stream(r, size)?.raw;
            found.push((id.clone(), d));
            Ok(())
        },
        &mut out.skipped,
    );
    // Each zip's members, for a BIOS zip matched by what it holds.
    let mut members_of: BTreeMap<SourceId, Vec<(String, crate::hash::Digest)>> = BTreeMap::new();
    for (id, d) in &found {
        if let Some(parent) = id.parent() {
            members_of.entry(parent).or_default().push((id.name(), d.clone()));
        }
    }
    for (id, d) in found {
        let name = id.name();
        let mut matched = Match::Unknown;
        let whole = is_zip(&name);
        let mut hits: Vec<(&PathBuf, &Place)> = if whole {
            // A zip is a BIOS file only as a set: every member a place's
            // zip should hold is in it, by contents (its name aside).
            let members = members_of.get(&id).map_or(&[][..], Vec::as_slice);
            places
                .iter()
                .filter(|(_, p)| {
                    p.reqs.iter().any(|f| {
                        !f.contents.is_empty() && f.contents.iter().all(|m| super::members::holds(members, m))
                    })
                })
                .collect()
        } else {
            places
                .iter()
                .filter(|(_, p)| p.reqs.iter().any(|f| accepts(f, &d)))
                .collect()
        };
        if whole && hits.is_empty() && !sets.contains(&name.to_ascii_lowercase()) {
            continue; // an ordinary zip: its members are what count
        }
        if hits.is_empty() {
            hits = places
                .iter()
                .filter(|(_, p)| p.reqs.iter().any(|f| f.name.eq_ignore_ascii_case(&name)))
                .collect();
            if !hits.is_empty() {
                matched = Match::NameOnly;
            }
        } else {
            matched = Match::Accepted;
        }
        let targets = hits
            .into_iter()
            .map(|(place, p)| {
                let state = match digest(place) {
                    Some(there) if there.sha1 == d.sha1 => TargetState::AlreadyThere,
                    Some(_) => TargetState::Taken,
                    None if matched == Match::Accepted && !claimed.insert(place.clone()) => {
                        TargetState::Claimed
                    }
                    None => TargetState::New,
                };
                if matched == Match::Accepted && state == TargetState::New {
                    out.new_places += 1;
                }
                ImportTarget {
                    place: place.clone(),
                    used_by: p.used_by.clone(),
                    state,
                }
            })
            .collect();
        out.files.push(ImportFile {
            shown: id.shown(),
            source: id,
            name,
            size: d.size,
            sha1: d.sha1,
            matched,
            targets,
        });
    }
    Ok(out)
}

/// Do what [`plan`] says, plus the name-only files whose `shown` is in
/// `also`. Returns what was written; the batch is journaled for [`undo`].
pub fn apply(
    conn: &Connection,
    dir: &Path,
    home: &Home,
    env: &Env,
    paths: &[PathBuf],
    also: &[String],
    move_originals: bool,
) -> Result<Imported, Error> {
    let plan = plan(dir, home, env, paths)?;
    let originals = if move_originals {
        super::originals::taken(&plan, also)
    } else {
        Vec::new()
    };
    let mut wanted: BTreeMap<SourceId, (String, Vec<PathBuf>, bool)> = BTreeMap::new();
    let mut taken: HashSet<PathBuf> = HashSet::new();
    for f in &plan.files {
        let picked =
            f.matched == Match::Accepted || (f.matched == Match::NameOnly && also.contains(&f.shown));
        let places: Vec<PathBuf> = f
            .targets
            .iter()
            .filter(|t| picked && t.state == TargetState::New && taken.insert(t.place.clone()))
            .map(|t| t.place.clone())
            .collect();
        if !places.is_empty() {
            wanted.insert(
                f.source.clone(),
                (f.sha1.clone(), places, f.matched == Match::NameOnly),
            );
        }
    }
    let mut done = Imported {
        written: 0,
        moved: 0,
        linked: 0,
        batch: None,
    };
    if wanted.is_empty() && originals.is_empty() {
        return Ok(done);
    }
    conn.execute(
        concat!(
            "INSERT INTO firmware_batches (created_at) VALUES (",
            crate::db::now!(),
            ")"
        ),
        [],
    )?;
    let batch = conn.last_insert_rowid();
    done.batch = Some(batch);
    let journal = |p: &Path, picked: bool| -> Result<(), Error> {
        conn.execute(
            "INSERT OR IGNORE INTO firmware_outputs (batch_id, path, picked) VALUES (?1, ?2, ?3)",
            params![batch, p.to_string_lossy(), picked],
        )?;
        Ok(())
    };
    let mut skipped = Vec::new();
    let mut failure = None;
    walk(
        paths,
        &|name| is_zip(name),
        &mut |id, r, _| {
            let Some((sha1, places, picked)) = wanted.get(id) else {
                return Ok(());
            };
            let first = &places[0];
            if let Some(dir) = first.parent() {
                std::fs::create_dir_all(dir)?;
            }
            write_once(first, r)?;
            journal(first, *picked)?;
            if digest(first).is_none_or(|d| d.sha1 != *sha1) {
                std::fs::remove_file(first)?;
                return Err(Error::Launch(format!(
                    "{} didn't read back the same",
                    first.display()
                )));
            }
            done.written += 1;
            for p in &places[1..] {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                if std::fs::hard_link(first, p).is_err() {
                    write_once(p, &mut std::fs::File::open(first)?)?;
                }
                journal(p, *picked)?;
                done.linked += 1;
            }
            Ok(())
        },
        &mut skipped,
    );
    if let Some((what, why)) = skipped.into_iter().next() {
        failure = Some(Error::Launch(format!("{what}: {why}")));
    }
    if failure.is_none() {
        done.moved = super::originals::trash(conn, home, batch, &originals)?;
    }
    match failure {
        Some(e) if done.written == 0 => Err(e),
        _ => Ok(done),
    }
}

/// Write `path` from `r` without ever replacing a file there.
fn write_once(path: &Path, r: &mut dyn std::io::Read) -> Result<(), Error> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(e) = std::io::copy(r, &mut f) {
        drop(f);
        let _ = std::fs::remove_file(path);
        return Err(e.into());
    }
    Ok(())
}
