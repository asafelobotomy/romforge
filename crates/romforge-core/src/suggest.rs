//! Suggestions for files no hash identified, each with its evidence
//! (milestone 12.6).
//!
//! Candidates come from three places: a CRC32 that matched without a SHA1
//! or MD5, a game ID read from the file's own header that a DAT game's
//! serial carries, and titles alike to the file's name or its header
//! title. Each candidate is then weighed on everything known: name, header
//! title, size, CRC, serial, region. With no DAT title alike the name, the
//! other names LaunchBox's database (when downloaded) gives a game lead to
//! DAT titles too ([`aliases`]). The band says how strong that is; no
//! band is ever counted or shown as verified (house rule 8). Confirming a
//! suggestion makes the file "identified by you".

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::ident::GameId;
use crate::{Error, titles, view};

mod aliases;
mod assess;
mod facts;
#[cfg(test)]
mod tests;

pub(crate) use facts::verified_copy;

use assess::assess;
use facts::{applies, by_serial, dat_names, facts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    /// A patch beside it, applied to your hash-verified copy of the game,
    /// gives exactly this file: a proven patch of it (still never
    /// verified: the file itself is in no DAT).
    PatchOf,
    /// CRC32 and size agree, but the DAT's SHA1 or MD5 doesn't (or it has
    /// none).
    LikelyCrc,
    /// A compressed disc image (RVZ, WIA, GCZ, CSO, ZSO) holding an image
    /// of the game's size, its ID in the game's serial. Only decompressing
    /// it all could verify it.
    LikelyCompressed,
    /// The same size as the game's ROM and its name or serial agrees: a bad
    /// dump, a hack or a translation of it.
    LikelyModified,
    /// The same title (or its serial with a title that fits).
    Suggested,
    /// A similar title, or a serial alone.
    Possible,
}

impl Band {
    pub fn label(self) -> &'static str {
        match self {
            Band::PatchOf => "Proven patch of it",
            Band::LikelyCrc => "Likely CRC match",
            Band::LikelyCompressed => "Likely compressed image",
            Band::LikelyModified => "Likely modified copy",
            Band::Suggested => "Suggested by name",
            Band::Possible => "Possible similar name",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    /// `name`, `header_title`, `size`, `crc`, `serial`, `region`, `hash`.
    pub kind: String,
    /// For or against.
    pub agrees: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Suggestion {
    /// System key.
    pub system: String,
    /// The DAT game's name.
    pub title: String,
    pub band: Band,
    /// How alike the names are ([`titles::score`]), 0 to 1.
    pub score: f32,
    pub evidence: Vec<Evidence>,
    /// When the file's name says translation or hack: taking it names the
    /// file so ("translation of X"), never verified.
    pub derived: Option<view::Derived>,
}

/// How much a patch beside a file says about a DAT game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Proof {
    /// BPS / UPS: made from a ROM of its size and CRC32.
    MadeFrom,
    /// ... and makes a file of this one's CRC32.
    Turns,
    /// Applied to your hash-verified copy, it gives exactly this file
    /// (SHA1).
    Applied,
}

/// What is known about a row's files.
struct Facts {
    /// What the titles are scored against: what the owner typed, or `own`.
    text: String,
    /// The row's own title or file name, which alone says what the file is
    /// (a translation, a region): typed text only searches.
    own: String,
    sizes: Vec<u64>,
    ids: Vec<GameId>,
    systems: Vec<i64>,
    /// (system id, DAT game) a CRC32 hit.
    crc: BTreeSet<(i64, String)>,
    /// (system id, DAT game) a patch beside a file was made from.
    patched: BTreeMap<(i64, String), Proof>,
    /// Patches beside the files that name no source (IPS), with the SHA1
    /// of the file beside them, to try on verified copies of candidates.
    unnamed: Vec<(Vec<u8>, String)>,
    /// (system id, DAT game) whose ROM has the CRC32 an NKit image gives
    /// for its original image.
    nkit: BTreeSet<(i64, String)>,
}

/// Suggestions for a row, strongest first: by band, then how much of the
/// evidence agrees, then the name. `text`: what the owner typed (the
/// row's title or file name when empty); `system`: limit to this key.
/// `databases`: the folder of downloaded databases (LaunchBox's names).
pub(crate) fn for_detail(
    conn: &Connection,
    databases: Option<&std::path::Path>,
    d: &view::GameDetail,
    text: Option<&str>,
    system: Option<&str>,
    limit: usize,
) -> Result<Vec<Suggestion>, Error> {
    Suggester::new(databases).suggest(conn, d, text, system, limit)
}

/// DAT names (each with its systems) and their title index.
type Names = (BTreeMap<String, Vec<i64>>, titles::TitleIndex);

/// Suggestions for many rows, keeping each set of systems' title index.
#[derive(Default)]
pub struct Suggester {
    indexes: BTreeMap<Vec<i64>, Names>,
    /// Folders and archives listed for patches, once each.
    patches: crate::patch::PatchFinder,
    aliases: Option<aliases::Aliases>,
}

impl Suggester {
    /// With LaunchBox's names, when its database is in `databases`.
    pub fn new(databases: Option<&std::path::Path>) -> Self {
        Suggester {
            aliases: databases.and_then(aliases::Aliases::open),
            ..Suggester::default()
        }
    }

    pub fn suggest(
        &mut self,
        conn: &Connection,
        d: &view::GameDetail,
        text: Option<&str>,
        system: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Suggestion>, Error> {
        let f = facts(conn, d, text, system, &mut self.patches)?;
        if !self.indexes.contains_key(&f.systems) {
            let names = dat_names(conn, &f.systems)?;
            let index = titles::TitleIndex::new(names.keys().cloned());
            self.indexes.insert(f.systems.clone(), (names, index));
        }
        let (names, index) = &self.indexes[&f.systems];
        let aliased = match self.aliases.as_mut() {
            Some(a) => aliases::dat_titles(a, &f.systems, &f.text, names, index)?,
            None => BTreeMap::new(),
        };
        suggest_with(conn, &f, names, index, &aliased, limit)
    }
}

fn suggest_with(
    conn: &Connection,
    f: &Facts,
    names: &BTreeMap<String, Vec<i64>>,
    index: &titles::TitleIndex,
    aliased: &BTreeMap<(i64, String), String>,
    limit: usize,
) -> Result<Vec<Suggestion>, Error> {
    let serials = by_serial(conn, f)?;
    let mut candidates: BTreeSet<(i64, String)> = f.crc.clone();
    candidates.extend(aliased.keys().cloned());
    candidates.extend(f.patched.keys().cloned());
    candidates.extend(f.nkit.iter().cloned());
    candidates.extend(serials.keys().cloned());
    let header_titles: Vec<String> = f.ids.iter().filter_map(|i| i.title.clone()).collect();
    for q in std::iter::once(&f.text).chain(&header_titles) {
        for (title, _) in index.find(q, 40) {
            for sid in &names[&title] {
                candidates.insert((*sid, title.clone()));
            }
        }
    }
    // IPS names no source: try it on your verified copies of the
    // candidates (a few, best named first).
    let mut proofs = f.patched.clone();
    if !f.unnamed.is_empty() {
        let mut tried: Vec<&(i64, String)> = candidates.iter().collect();
        tried.sort_by(|a, b| titles::score(&f.text, &b.1).total_cmp(&titles::score(&f.text, &a.1)));
        for (sid, title) in tried.into_iter().take(20) {
            for (data, sha1) in &f.unnamed {
                if applies(conn, data, *sid, title, sha1)? {
                    proofs.insert((*sid, title.clone()), Proof::Applied);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (sid, title) in &candidates {
        if !f.systems.is_empty() && !f.systems.contains(sid) {
            continue;
        }
        let serial = serials.get(&(*sid, title.clone())).map(String::as_str);
        let alias = aliased.get(&(*sid, title.clone())).map(String::as_str);
        if let Some(s) = assess(conn, f, &proofs, (*sid, title), serial, alias)? {
            out.push(s);
        }
    }
    let agreeing = |s: &Suggestion| s.evidence.iter().filter(|e| e.agrees).count();
    out.sort_by(|a, b| {
        a.band
            .cmp(&b.band)
            .then(agreeing(b).cmp(&agreeing(a)))
            .then(b.score.total_cmp(&a.score))
            .then_with(|| a.title.cmp(&b.title))
    });
    out.truncate(limit);
    Ok(out)
}
