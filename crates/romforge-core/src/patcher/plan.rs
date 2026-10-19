use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::disc::{is_chd, track_of, track_order};
use super::found::guess_relation;
use super::read::{PatchFile, read_patch};
use crate::cleanup::{self, Part, Source};
use crate::organize::sanitize;
use crate::patch::{self, Kind};
use crate::view::{self, Derived, Relation, RowStatus};
use crate::{Error, systems};

/// Which bytes of a cartridge ROM a patch is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    /// The file as it is.
    Raw,
    /// The DAT's form: without a copier header, in the DAT's byte order.
    Dat,
}

/// What applying a patch would do (nothing is written).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchPlan {
    pub row_id: String,
    pub base: String,
    pub system: String,
    pub patch: PathBuf,
    pub patch_name: String,
    pub kind: &'static str,
    pub relation: Relation,
    pub tag: String,
    /// The new game's title: "<base> [Translation: <tag>]".
    pub title: String,
    pub target: PathBuf,
    pub disc: bool,
    /// What it is applied to, in words.
    pub applies_to: String,
    pub notes: Vec<String>,
    #[serde(skip)]
    pub(super) parts: Vec<Part>,
    #[serde(skip)]
    pub(super) form: Form,
    /// The ROM patched (a cartridge), in `parts`.
    #[serde(skip)]
    pub(super) rom: usize,
}

/// Which of a cartridge's ROMs a patch that names no source goes on: the
/// only one; the one the patch is named after; for an xdelta, the first it
/// decodes against; else the biggest (the program ROM, usually), said in a
/// note.
fn pick_rom(
    parts: &[Part],
    roms: &[(Vec<u8>, Option<Vec<u8>>)],
    p: &PatchFile,
    notes: &mut Vec<String>,
) -> usize {
    if parts.len() == 1 {
        return 0;
    }
    let stem = |n: &str| {
        let n = n.rsplit(['/', '\\']).next().unwrap_or(n);
        n.rsplit_once('.').map_or(n, |(s, _)| s).to_lowercase()
    };
    let patch_stem = stem(&p.name);
    if let Some(i) = parts.iter().position(|x| stem(&x.name) == patch_stem) {
        return i;
    }
    if p.kind == Kind::Xdelta
        && let Some(i) = roms.iter().position(|(raw, dat)| {
            patch::apply(&p.data, dat.as_deref().unwrap_or(raw)).is_some()
                || patch::apply(&p.data, raw).is_some()
        })
    {
        return i;
    }
    let i = (0..parts.len())
        .max_by_key(|i| (parts[*i].size, std::cmp::Reverse(*i)))
        .unwrap_or(0);
    notes.push(format!(
        "{} says nothing about which of the game's {} ROMs it is for: applied to {}, the biggest",
        p.kind.name(),
        parts.len(),
        parts[i].name
    ));
    i
}

/// The patch's own name, without its extension: the default tag.
pub(super) fn tag_of(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    stem.replace('_', " ").trim().to_owned()
}

/// A header and the byte order the DAT's form takes, if any.
pub(super) fn dat_form(data: &[u8]) -> Option<(&'static crate::headers::HeaderRule, Vec<u8>)> {
    let prefix = &data[..data.len().min(crate::headers::PREFIX_LEN)];
    let rule = crate::headers::detect(prefix, data.len() as u64)?;
    let body = &data[rule.len..];
    let whole = body.len() - body.len() % rule.reorder.unit();
    let mut out = Vec::with_capacity(body.len());
    rule.reorder.apply(&body[..whole], &mut out);
    out.extend_from_slice(&body[whole..]);
    Some((rule, out))
}

/// Work out applying `patch_path` to the game in library row `row_id`, the
/// result going to `games` (`Games/` or the Library folder).
pub fn plan(
    conn: &Connection,
    games: &Path,
    staging: &Path,
    row_id: &str,
    patch_path: &Path,
    relation: Option<Relation>,
    tag: Option<&str>,
) -> Result<PatchPlan, Error> {
    let bad = |m: String| Error::Cleanup(m);
    let v = view::library(conn)?;
    let row = v
        .rows
        .iter()
        .find(|r| r.id == row_id)
        .ok_or_else(|| bad(format!("no library row {row_id:?}")))?;
    if !matches!(row.status, RowStatus::Verified | RowStatus::ByYou) {
        return Err(bad(format!(
            "{} isn't identified as a game (a patch goes on a verified copy, or one you named)",
            row.title
        )));
    }
    let system = row
        .system
        .as_deref()
        .and_then(systems::by_key)
        .ok_or_else(|| bad(format!("{} has no platform", row.title)))?;
    if system.arcade || system.folders != systems::Folders::No {
        return Err(bad(format!("{} games aren't patched here", system.key)));
    }
    let p = read_patch(patch_path)?;
    // Unsaid: what the patch's names and readme say.
    let relation = relation.unwrap_or_else(|| {
        let file = patch_path.file_name().unwrap_or_default().to_string_lossy();
        let readme = p
            .readme
            .as_ref()
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned());
        guess_relation(&format!("{} {file}", p.name), readme.as_deref().unwrap_or(""))
    });
    // A PPF's own description names it best; else the patch's file name.
    let described = patch::ppf_records(&p.data)
        .map(|x| x.description)
        .filter(|d| !d.trim().is_empty());
    let tag = tag
        .map(str::to_owned)
        .or(described)
        .unwrap_or_else(|| tag_of(&p.name));
    let derived = Derived {
        relation,
        base: row.title.clone(),
        tag: (!tag.is_empty()).then_some(tag.clone()),
    };
    let title = derived.title();
    let dir = games.join(sanitize(system.key));
    let source = Source::Folder(PathBuf::new());
    let parts = match row.status {
        RowStatus::Verified => cleanup::verified_parts(conn, system.id, &row.title, &source)?,
        _ => cleanup::named_parts(conn, &row.id, &source)?,
    }
    .map(|(p, _)| p)
    .ok_or_else(|| bad(format!("{}: no complete set of files to patch", row.title)))?;
    let mut notes = Vec::new();
    let disc = system.is_disc_based;
    let (target, applies_to, (form, rom)) = if disc {
        if !matches!(p.kind, Kind::Xdelta | Kind::Ppf) {
            notes.push(format!(
                "a {} patch on a disc: applied to the first track in memory, which only works for small images",
                p.kind.name()
            ));
        }
        let tracks = track_order(&parts, &p);
        let first = tracks
            .first()
            .map(|i| &parts[*i])
            .ok_or_else(|| bad(format!("{}: no track", row.title)))?;
        let named = (p.kind == Kind::Xdelta)
            .then(|| crate::vcdiff::names(&p.data))
            .flatten()
            .map(|(_, source)| source);
        let applies_to = if parts.iter().any(|x| is_chd(&x.path)) {
            match named {
                Some(source) => format!("{source} in {} (the track the patch names)", row.title),
                None => format!("the first track of {} that takes it", row.title),
            }
        } else if tracks.len() == 1 {
            format!("{} ({})", first.name, row.title)
        } else if named.is_some_and(|s| track_of(&first.name, &s)) {
            format!("{} (the track the patch names)", first.name)
        } else {
            format!(
                "{} first, else the next track of {} that takes it",
                first.name, row.title
            )
        };
        (
            dir.join(format!("{}.chd", sanitize(&title))),
            applies_to,
            (Form::Raw, 0),
        )
    } else {
        // Each ROM as it is and in the DAT's form.
        let mut roms = Vec::new();
        for part in &parts {
            let mut data = Vec::new();
            cleanup::open_part(part, staging)?.read_to_end(&mut data)?;
            let dat = dat_form(&data).map(|(_, d)| d);
            roms.push((data, dat));
        }
        let info = patch::info(&p.data).ok_or_else(|| bad("a damaged patch".into()))?;
        let (rom, form) = match info.source {
            // BPS / UPS: the ROM, and its form, the patch was made from.
            Some((size, crc)) => {
                let fits = |d: &[u8]| size == d.len() as u64 && crc == crc32fast::hash(d);
                roms.iter()
                    .enumerate()
                    .find_map(|(i, (raw, dat))| {
                        if fits(raw) {
                            Some((i, Form::Raw))
                        } else {
                            dat.as_deref().filter(|d| fits(d)).map(|_| (i, Form::Dat))
                        }
                    })
                    .ok_or_else(|| {
                        let yours = if roms.len() == 1 {
                            format!("your {} isn't that one", row.title)
                        } else {
                            format!("none of the {} ROMs of your {} is", roms.len(), row.title)
                        };
                        bad(format!(
                            "{} was made for a ROM of {size} bytes, CRC32 {crc:08x}; {yours} \
                             (another revision or region?)",
                            p.name
                        ))
                    })?
            }
            None => {
                let rom = pick_rom(&parts, &roms, &p, &mut notes);
                let form = if roms[rom].1.is_none() {
                    Form::Raw
                } else {
                    let n = p.name.to_ascii_lowercase();
                    let headered = n.contains("header")
                        && !n.contains("headerless")
                        && !n.contains("unheader")
                        && !n.contains("no header")
                        && !n.contains("no-header");
                    if headered {
                        notes.push(
                            "its name says it is for a headered ROM: applied to the file as it is".into(),
                        );
                        Form::Raw
                    } else {
                        notes.push(format!(
                            "{} says nothing about its source: applied to the DAT's form (no copier header)",
                            p.kind.name()
                        ));
                        Form::Dat
                    }
                };
                (rom, form)
            }
        };
        let applies_to = format!(
            "{} ({})",
            parts[rom].name,
            if form == Form::Raw {
                "as it is"
            } else {
                "the DAT's form"
            }
        );
        // `rom` goes in the plan with the form.
        (
            dir.join(format!("{}.zip", sanitize(&title))),
            applies_to,
            (form, rom),
        )
    };
    if target.exists() {
        return Err(bad(format!("{} already exists", target.display())));
    }
    Ok(PatchPlan {
        row_id: row.id.clone(),
        base: row.title.clone(),
        system: system.key.to_owned(),
        patch: patch_path.to_owned(),
        patch_name: p.name,
        kind: p.kind.name(),
        relation,
        tag,
        title,
        target,
        disc,
        applies_to,
        notes,
        parts,
        form,
        rom,
    })
}
