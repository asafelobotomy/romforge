//! Milestone 19: apply a translation or hack to a game you have. The patch
//! is one you fetched (romforge finds no patches online by itself); the
//! result is a new file in the RomForge folder (or the Library), shown as a
//! version of the game, and the original is never touched.
//!
//! - Cartridges: IPS, BPS, UPS, PPF or xdelta applied in memory to the ROM
//!   in the form the patch wants: BPS and UPS say it (their source's size
//!   and CRC32: the file as it is, or the DAT's form without a copier
//!   header); for the others it is the DAT's form, unless an IPS's name says
//!   it is for a headered ROM. A header taken off goes back on. Written as
//!   a zip like any import.
//! - Discs: xdelta or PPF applied, streamed, to the track it was made from
//!   (or the image), the others kept, and written as a CHD: the track an
//!   xdelta names, else the first that takes it (a PPF's check block or an
//!   xdelta's checksums refuse the others). A CHD is unpacked to staging
//!   first, with a cue sheet from its own track list.
//!
//! The new file is named as a derivative of the game ("<Game> [Translation:
//! <tag>]", identified by you, never verified: rule 8, unless a hack DAT
//! lists its hash) and remembers which patch made it. Journaled as a
//! cleanup batch, so `cleanup undo` removes it.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::cleanup::{self, Format, Item, Part, Source};
use crate::organize::sanitize;
use crate::patch::{self, Kind};
use crate::view::{self, Derived, Relation, RowStatus};
use crate::{Error, layout, systems};

/// A patch read from disk: the patch itself, or the one patch in a zip
/// (with the zip's readme, kept with the result).
#[derive(Debug, Clone)]
pub struct PatchFile {
    pub name: String,
    pub kind: Kind,
    pub data: Vec<u8>,
    pub sha1: String,
    pub readme: Option<(String, Vec<u8>)>,
}

fn is_readme(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".txt", ".nfo", ".diz", ".md"].iter().any(|e| n.ends_with(e))
}

/// Read a patch file, or a zip holding exactly one.
pub fn read_patch(path: &Path) -> Result<PatchFile, Error> {
    let bad = |m: String| Error::Dat(format!("patch: {m}"));
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (name, data, readme) = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let mut z = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(path)?))?;
        let names: Vec<String> = (0..z.len())
            .filter_map(|i| {
                z.by_index_raw(i)
                    .ok()
                    .and_then(|m| m.name().ok().map(|n| n.into_owned()))
            })
            .filter(|n| !n.ends_with('/'))
            .collect();
        let patches: Vec<&String> = names.iter().filter(|n| patch::is_patch_name(n)).collect();
        let one = match patches.as_slice() {
            [one] => (*one).clone(),
            [] => return Err(bad(format!("{file_name} holds no patch romforge reads"))),
            many => {
                return Err(bad(format!(
                    "{file_name} holds {} patches ({}); unzip the one you want and pick it",
                    many.len(),
                    many.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )));
            }
        };
        let mut data = Vec::new();
        z.by_name(&one)?.take(512 << 20).read_to_end(&mut data)?;
        let readme = names
            .iter()
            .filter(|n| is_readme(n))
            .min_by_key(|n| !n.to_ascii_lowercase().contains("readme"))
            .and_then(|n| {
                let mut d = Vec::new();
                z.by_name(n).ok()?.take(1 << 20).read_to_end(&mut d).ok()?;
                Some((n.rsplit('/').next().unwrap_or(n).to_owned(), d))
            });
        (one.rsplit('/').next().unwrap_or(&one).to_owned(), data, readme)
    } else {
        (file_name, std::fs::read(path)?, None)
    };
    let kind = patch::info(&data)
        .ok_or_else(|| {
            bad(format!(
                "{name} isn't a patch romforge reads (IPS, BPS, UPS, PPF, xdelta)"
            ))
        })?
        .kind;
    let sha1 = cleanup::sha1_of(&data[..])?;
    Ok(PatchFile {
        name,
        kind,
        data,
        sha1,
        readme,
    })
}

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
    parts: Vec<Part>,
    #[serde(skip)]
    form: Form,
    /// The ROM patched (a cartridge), in `parts`.
    #[serde(skip)]
    rom: usize,
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
fn tag_of(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    stem.replace('_', " ").trim().to_owned()
}

/// A header and the byte order the DAT's form takes, if any.
fn dat_form(data: &[u8]) -> Option<(&'static crate::headers::HeaderRule, Vec<u8>)> {
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

/// What [`apply`] made.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Patched {
    pub batch_id: i64,
    pub target: PathBuf,
    pub title: String,
    /// SHA1 and size of the patched ROM (or track).
    pub sha1: String,
    pub size: u64,
}

/// Apply a plan: write the new file, journal it (a cleanup batch, so
/// `cleanup undo` removes it), name it as a derivative of its game, and
/// remember the patch that made it.
pub fn apply(conn: &mut Connection, plan: &PatchPlan, staging_root: &Path) -> Result<Patched, Error> {
    let p = read_patch(&plan.patch)?;
    let system = systems::by_key(&plan.system).ok_or_else(|| Error::UnknownSystem(plan.system.clone()))?;
    let dir = plan.target.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    conn.execute(
        concat!(
            "INSERT INTO cleanup_batches (created_at, keep_originals) VALUES (",
            crate::db::now!(),
            ", 1)"
        ),
        [],
    )?;
    let batch = conn.last_insert_rowid();
    let staging = staging_root.join(format!("patch-{batch}"));
    std::fs::create_dir_all(&staging)?;
    let name = plan
        .target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let part = dir.join(layout::part(&name));
    let _ = std::fs::remove_file(&part);
    let made = if plan.disc {
        write_disc(plan, &p, &staging, &part)
    } else {
        write_cartridge(plan, &p, &staging, &part)
    };
    let _ = std::fs::remove_dir_all(&staging);
    let (sha1, size) = match made {
        Ok(x) => x,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            conn.execute("DELETE FROM cleanup_batches WHERE batch_id = ?1", [batch])?;
            return Err(e);
        }
    };
    if plan.target.exists() {
        let _ = std::fs::remove_file(&part);
        conn.execute("DELETE FROM cleanup_batches WHERE batch_id = ?1", [batch])?;
        return Err(Error::Cleanup(format!(
            "{} already exists",
            plan.target.display()
        )));
    }
    std::fs::rename(&part, &plan.target)?;
    cleanup::journal_output(conn, batch, &plan.target, "file")?;
    let tx = conn.transaction()?;
    tx.execute(
        concat!(
            "INSERT INTO manual_ids (sha1, size, system_id, title, created_at, base, relation, tag)
             VALUES (?1, ?2, ?3, ?4, ",
            crate::db::now!(),
            ", ?5, ?6, ?7)
             ON CONFLICT(sha1, size) DO UPDATE SET system_id = excluded.system_id, title = excluded.title,
               base = excluded.base, relation = excluded.relation, tag = excluded.tag, proof = NULL"
        ),
        params![
            sha1,
            size,
            system.id,
            plan.title,
            plan.base,
            plan.relation.as_str(),
            (!plan.tag.is_empty()).then_some(&plan.tag)
        ],
    )?;
    tx.execute(
        concat!(
            "INSERT INTO patched (sha1, size, base, system_id, patch_name, patch_sha1, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ",
            crate::db::now!(),
            ")
             ON CONFLICT(sha1, size) DO UPDATE SET base = excluded.base, patch_name = excluded.patch_name,
               patch_sha1 = excluded.patch_sha1, kind = excluded.kind"
        ),
        params![sha1, size, plan.base, system.id, p.name, p.sha1, p.kind.name()],
    )?;
    tx.commit()?;
    Ok(Patched {
        batch_id: batch,
        target: plan.target.clone(),
        title: plan.title.clone(),
        sha1,
        size,
    })
}

/// A cartridge: patched in memory, zipped with its details (and the
/// patch's readme) in `romforge/`.
fn write_cartridge(
    plan: &PatchPlan,
    p: &PatchFile,
    staging: &Path,
    out: &Path,
) -> Result<(String, u64), Error> {
    let rom = &plan.parts[plan.rom];
    let mut data = Vec::new();
    cleanup::open_part(rom, staging)?.read_to_end(&mut data)?;
    let patched = patch_rom(&p.data, &data, plan.form)
        .ok_or_else(|| Error::Cleanup(format!("{} doesn't apply to {}", p.name, plan.base)))?;
    if patched == data {
        return Err(Error::Cleanup(format!(
            "{} changes nothing in {}",
            p.name, plan.base
        )));
    }
    // One ROM is named after the new game; a game of several keeps the
    // DAT's names, the one patched among them.
    let member = if plan.parts.len() == 1 {
        let ext = Path::new(&rom.name)
            .extension()
            .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
        format!("{}{ext}", sanitize(&plan.title))
    } else {
        rom.name.clone()
    };
    let mut extras = vec![(
        format!("{}{}", sanitize(&plan.title), layout::DETAILS_SUFFIX),
        details(plan, p).into_bytes(),
    )];
    if let Some((n, d)) = &p.readme {
        extras.push((n.clone(), d.clone()));
    }
    let mut zip = zip::ZipWriter::new(std::fs::File::create_new(out)?);
    for (i, part) in plan.parts.iter().enumerate() {
        if i == plan.rom {
            zip.start_file(&member, cleanup::zip_options(patched.len() as u64))?;
            zip.write_all(&patched)?;
        } else {
            zip.start_file(&part.name, cleanup::zip_options(part.size))?;
            std::io::copy(&mut cleanup::open_part(part, staging)?, &mut zip)?;
        }
    }
    cleanup::write_extras(&mut zip, &extras)?;
    zip.finish()?.sync_all()?;
    let sha1 = cleanup::sha1_of(&patched[..])?;
    let mut back = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(out)?))?;
    if cleanup::sha1_of(back.by_name(&member)?)? != sha1 {
        return Err(Error::Cleanup(format!(
            "{}: doesn't read back as written",
            out.display()
        )));
    }
    Ok((sha1, patched.len() as u64))
}

/// A cartridge ROM patched in `form`: in the DAT's form, a copier header
/// taken off goes back on (a byte order stays the DAT's).
fn patch_rom(patch: &[u8], data: &[u8], form: Form) -> Option<Vec<u8>> {
    match (form, dat_form(data)) {
        (Form::Dat, Some((rule, body))) => {
            let new = patch::apply(patch, &body)?;
            Some(if rule.reorder == crate::headers::Reorder::None {
                let mut v = data[..rule.len].to_vec();
                v.extend_from_slice(&new);
                v
            } else {
                new
            })
        }
        _ => patch::apply(patch, data),
    }
}

/// The new game's details file.
fn details(plan: &PatchPlan, p: &PatchFile) -> String {
    let base_sha1: Option<String> = if plan.disc {
        plan.parts
            .iter()
            .find(|x| !x.name.to_lowercase().ends_with(".cue"))
            .map(|x| x.sha1.clone())
    } else {
        plan.parts.get(plan.rom).map(|x| x.sha1.clone())
    };
    format!(
        "# {}\n\n- Platform: {}\n- Status: {} of {}, patched by romforge (identified by you, not verified)\n\
         - Patch: {} ({}, SHA1 {})\n- Applied to: {}{}\n",
        plan.title,
        plan.system,
        plan.relation.as_str(),
        plan.base,
        p.name,
        p.kind.name(),
        p.sha1,
        plan.applies_to,
        base_sha1.map_or(String::new(), |s| format!(" (SHA1 {s})")),
    )
}

/// A track (or image) on disk to patch: a loose file as it is, else unpacked
/// into `staging`.
fn on_disk(part: &Part, staging: &Path) -> Result<PathBuf, Error> {
    if part.member.is_empty()
        && !part
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("chd"))
    {
        return Ok(part.path.clone());
    }
    let to = staging.join(format!("{}.in", part.sha1));
    if !to.exists() {
        let mut w = std::fs::File::create(&to)?;
        std::io::copy(&mut cleanup::open_part(part, staging)?, &mut w)?;
    }
    Ok(to)
}

/// A disc: its tracks (unpacked from a CHD with a cue sheet of its own, or
/// loose as they are), the first one patched, written as a CHD.
fn write_disc(plan: &PatchPlan, p: &PatchFile, staging: &Path, out: &Path) -> Result<(String, u64), Error> {
    let mut parts = plan.parts.clone();
    // Tracks in a CHD: unpacked, with a cue from its track list.
    if let Some(chd) = parts.iter().find(|x| is_chd(&x.path)).map(|x| x.path.clone()) {
        parts = unpack_chd(&chd, staging)?;
    }
    // The track the patch names first, then the others in order: one that
    // isn't the patch's source is refused by its checks.
    let order = track_order(&parts, p);
    let patched = staging.join("patched.bin");
    let mut first = None;
    let mut refused = None;
    for &i in &order {
        let source = on_disk(&parts[i], staging)?;
        let _ = std::fs::remove_file(&patched);
        match patch::apply_file(&p.data, &source, &patched) {
            Ok(()) => {
                first = Some(i);
                break;
            }
            Err(e) => {
                refused.get_or_insert(e);
                // Only formats with a check can say "not this track".
                if !matches!(p.kind, Kind::Xdelta | Kind::Ppf) {
                    break;
                }
            }
        }
    }
    let Some(first) = first else {
        return Err(refused.unwrap_or_else(|| Error::Cleanup(format!("{}: no track", plan.base))));
    };
    let sha1 = cleanup::sha1_of(std::io::BufReader::new(std::fs::File::open(&patched)?))?;
    let size = std::fs::metadata(&patched)?.len();
    if sha1 == parts[first].sha1 {
        return Err(Error::Cleanup(format!(
            "{} changes nothing in {}",
            p.name, plan.base
        )));
    }
    parts[first] = Part {
        name: parts[first].name.clone(),
        path: patched,
        member: String::new(),
        size,
        sha1: sha1.clone(),
    };
    let tracks: Vec<&Part> = parts
        .iter()
        .filter(|x| !x.name.to_lowercase().ends_with(".cue"))
        .collect();
    let format = if tracks.len() == 1 && tracks[0].name.to_lowercase().ends_with(".iso") {
        Format::ChdDvd
    } else {
        Format::ChdCd
    };
    let item = Item {
        row_id: plan.row_id.clone(),
        title: plan.title.clone(),
        system: plan.system.clone(),
        format,
        target: plan.target.clone(),
        sidecar: None,
        parts,
        trash: Vec::new(),
        kept: Vec::new(),
        copies: Vec::new(),
        sheet: None,
        existing: false,
        folder: None,
        code: None,
    };
    cleanup::write_chd(&item, staging, out)?;
    Ok((sha1, size))
}

fn is_chd(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd"))
}

/// Whether a track file is the one a patch names: the same file name, or
/// the same "(Track N)".
fn track_of(track: &str, named: &str) -> bool {
    let number = |s: &str| {
        let s = s.to_ascii_lowercase();
        let at = s.rfind("(track ")? + 7;
        s[at..].split(')').next()?.trim().parse::<u32>().ok()
    };
    track.eq_ignore_ascii_case(named) || number(track).is_some_and(|n| Some(n) == number(named))
}

/// A disc's tracks (indices into `parts`, cue sheets left out) in the order
/// to try a patch on: the one an xdelta names first, then the rest as they
/// come.
fn track_order(parts: &[Part], p: &PatchFile) -> Vec<usize> {
    let mut order: Vec<usize> = (0..parts.len())
        .filter(|i| !parts[*i].name.to_lowercase().ends_with(".cue"))
        .collect();
    if p.kind == Kind::Xdelta
        && let Some((_, source)) = crate::vcdiff::names(&p.data)
        && let Some(at) = order.iter().position(|i| track_of(&parts[*i].name, &source))
    {
        let i = order.remove(at);
        order.insert(0, i);
    }
    order
}

/// A CHD's tracks written out as `.bin`s (or its `.iso`) in `staging`, with
/// a cue sheet made from its track list: every track's pregap is in its
/// `.bin`, so INDEX 01 comes after it.
fn unpack_chd(chd: &Path, staging: &Path) -> Result<Vec<Part>, Error> {
    let dir = staging.join("tracks");
    std::fs::create_dir_all(&dir)?;
    let mut out: Vec<Part> = Vec::new();
    let mut cue = String::new();
    let mut current = std::io::BufWriter::new(std::fs::File::create(dir.join("current"))?);
    let mut hasher = Sha1Sum::default();
    let mut size = 0u64;
    crate::disc::read_chd(chd, &mut |o| {
        match o {
            crate::disc::ChdOut::Data(d) => {
                current.write_all(d)?;
                hasher.update(d);
                size += d.len() as u64;
            }
            crate::disc::ChdOut::End(name, track) => {
                current.flush()?;
                let path = dir.join(&name);
                std::fs::rename(dir.join("current"), &path)?;
                current = std::io::BufWriter::new(std::fs::File::create(dir.join("current"))?);
                if let Some(t) = track {
                    let mode = match t.kind.as_str() {
                        "AUDIO" => "AUDIO",
                        "MODE1_RAW" => "MODE1/2352",
                        _ => "MODE2/2352",
                    };
                    cue += &format!("FILE \"{name}\" BINARY\n  TRACK {:02} {mode}\n", t.number);
                    if t.pregap > 0 {
                        let msf = |f: u64| format!("{:02}:{:02}:{:02}", f / 75 / 60, f / 75 % 60, f % 75);
                        cue += &format!("    INDEX 00 00:00:00\n    INDEX 01 {}\n", msf(t.pregap));
                    } else {
                        cue += "    INDEX 01 00:00:00\n";
                    }
                }
                out.push(Part {
                    name,
                    path,
                    member: String::new(),
                    size: std::mem::take(&mut size),
                    sha1: std::mem::take(&mut hasher).finish(),
                });
            }
        }
        Ok(())
    })?;
    drop(current);
    let _ = std::fs::remove_file(dir.join("current"));
    if !cue.is_empty() {
        let stem = chd.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let path = dir.join(format!("{stem}.cue"));
        std::fs::write(&path, &cue)?;
        out.push(Part {
            name: format!("{stem}.cue"),
            sha1: cleanup::sha1_of(cue.as_bytes())?,
            size: cue.len() as u64,
            path,
            member: String::new(),
        });
    }
    Ok(out)
}

/// A running SHA1.
#[derive(Default)]
struct Sha1Sum(sha1::Sha1);

impl Sha1Sum {
    fn update(&mut self, d: &[u8]) {
        use sha1::Digest as _;
        self.0.update(d);
    }

    fn finish(self) -> String {
        use sha1::Digest as _;
        crate::hash::hex(&self.0.finalize())
    }
}

/// A patch for a game: one a scan found and paired with it, or one lying
/// beside its files.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FoundPatch {
    /// The patch file, or the zip holding it.
    pub path: PathBuf,
    /// The patch's own name.
    pub name: String,
    pub kind: String,
    /// What its name and readme say it is.
    pub relation: Relation,
    /// How romforge knows it is for this game: "source" (the ROM it was
    /// made from, by size and CRC32), "names" (the file an xdelta names),
    /// "beside" (next to the game's file, of the same name), "title" (its
    /// name, or its readme's title line, is the game's), "readme" (its
    /// readme names the game), "folder" (in a folder of the game, not
    /// paired with any).
    pub paired_by: String,
}

/// The patches for a game: those a scan paired with it, then the others in
/// its folders that no scan paired with any game.
pub fn beside(conn: &Connection, row_id: &str) -> Result<Vec<FoundPatch>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut out = paired_with(conn, d.row.system_id, &d.row.title)?;
    let mut loose: Vec<PathBuf> = Vec::new();
    for f in &d.files {
        let path = f.path.split_once('#').map_or(f.path.as_str(), |(p, _)| p);
        let Some(dir) = Path::new(path).parent() else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if p.is_file() && patch::is_patch_name(&name) && !loose.contains(&p) {
                loose.push(p);
            }
        }
    }
    loose.sort();
    let mut known = conn.prepare_cached("SELECT title IS NOT NULL FROM patch_files WHERE path = ?1")?;
    for p in loose {
        use rusqlite::OptionalExtension;
        if out.iter().any(|x| x.path == p) {
            continue;
        }
        let paired: Option<bool> = known.query_row([p.to_string_lossy()], |r| r.get(0)).optional()?;
        if paired == Some(true) {
            continue; // another game's
        }
        let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let kind = std::fs::read(&p)
            .ok()
            .and_then(|d| patch::info(&d))
            .map_or("?", |i| i.kind.name());
        out.push(FoundPatch {
            relation: guess_relation(&name, ""),
            kind: kind.to_owned(),
            path: p,
            name,
            paired_by: "folder".into(),
        });
    }
    Ok(out)
}

/// The patches a scan paired with a game.
pub(crate) fn paired_with(
    conn: &Connection,
    system_id: Option<i64>,
    title: &str,
) -> Result<Vec<FoundPatch>, Error> {
    let Some(system_id) = system_id else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare_cached(
        "SELECT path, name, kind, relation, paired_by FROM patch_files
         WHERE system_id = ?1 AND title = ?2 ORDER BY name, path",
    )?;
    let rows = stmt.query_map(params![system_id, title], |r| {
        Ok(FoundPatch {
            path: PathBuf::from(r.get::<_, String>(0)?),
            name: r.get(1)?,
            kind: r.get(2)?,
            relation: Relation::parse(&r.get::<_, String>(3)?).unwrap_or(Relation::Hack),
            paired_by: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The patch file a scan found at `path`.
pub(crate) fn patch_at(conn: &Connection, path: &str) -> Result<Option<FoundPatch>, Error> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT path, name, kind, relation, coalesce(paired_by, 'folder') FROM patch_files WHERE path = ?1",
            [path],
            |r| {
                Ok(FoundPatch {
                    path: PathBuf::from(r.get::<_, String>(0)?),
                    name: r.get(1)?,
                    kind: r.get(2)?,
                    relation: Relation::parse(&r.get::<_, String>(3)?).unwrap_or(Relation::Hack),
                    paired_by: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// Translation or hack, from a patch's name and readme. Names say it
/// loosely ("English", "Eng"), readmes strictly ("translation").
pub(crate) fn guess_relation(name: &str, readme: &str) -> Relation {
    let n = name.to_lowercase();
    let r = readme.to_lowercase();
    let words: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).collect();
    let by_name = [
        "english",
        "eng",
        "translation",
        "translated",
        "trad",
        "traduccion",
        "traducao",
    ]
    .iter()
    .any(|w| words.contains(w))
        || n.contains("translat")
        || n.contains("[t-")
        || n.contains("(t-")
        || n.contains("[t+");
    let by_readme = [
        "translation",
        "translated",
        "traducción",
        "tradução",
        "traduction",
        "übersetzung",
    ]
    .iter()
    .any(|w| r.contains(w));
    if by_name || by_readme {
        Relation::Translation
    } else {
        Relation::Hack
    }
}

fn stem_lower(name: &str) -> String {
    let n = name.rsplit(['/', '\\']).next().unwrap_or(name);
    n.rsplit_once('.').map_or(n, |(s, _)| s).to_lowercase()
}

fn mtime_of(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?.modified().ok()?;
    let d = m.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_nanos()).ok()
}

/// Remember the patch files a scan of root `root_id` found (`found`: a
/// patch file, or a zip with a patch in it), reading only new or changed
/// ones; forget this root's others, except those in `kept` (files the scan
/// saw unchanged).
pub(crate) fn record(
    conn: &Connection,
    root_id: i64,
    found: &std::collections::BTreeSet<PathBuf>,
    kept: &std::collections::HashSet<String>,
) -> Result<(), Error> {
    use rusqlite::OptionalExtension;
    let mut known = conn.prepare_cached("SELECT size, mtime FROM patch_files WHERE path = ?1")?;
    for path in found {
        let key = path.to_string_lossy();
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let mtime = mtime_of(path);
        let had: Option<(u64, Option<i64>)> = known
            .query_row([&key], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if had == Some((size, mtime)) && mtime.is_some() {
            conn.execute(
                "UPDATE patch_files SET root_id = ?2 WHERE path = ?1",
                params![key, root_id],
            )?;
            continue;
        }
        // A zip of several patches, or a damaged one: not offered.
        let Ok(p) = read_patch(path) else {
            conn.execute("DELETE FROM patch_files WHERE path = ?1", [&key])?;
            continue;
        };
        let info = patch::info(&p.data);
        let readme = p
            .readme
            .as_ref()
            .map(|(_, d)| String::from_utf8_lossy(&d[..d.len().min(8192)]).into_owned())
            .unwrap_or_default();
        let readme_title = readme
            .lines()
            .map(|l| l.trim_matches(|c: char| c.is_whitespace() || "=-*#~_|".contains(c)))
            .find(|l| l.chars().filter(|c| c.is_alphanumeric()).count() >= 3)
            .map(|l| l.chars().take(120).collect::<String>())
            .unwrap_or_default();
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut hint = vec![stem_lower(&p.name)];
        if stem_lower(&file_name) != hint[0] {
            hint.push(stem_lower(&file_name));
        }
        if !readme_title.is_empty() {
            hint.push(readme_title);
        }
        let names_source = (p.kind == Kind::Xdelta)
            .then(|| crate::vcdiff::names(&p.data))
            .flatten()
            .map(|(_, s)| s);
        conn.execute(
            "INSERT INTO patch_files (path, root_id, name, kind, sha1, size, mtime, source_size, source_crc32,
                                      target_size, target_crc32, names_source, hint, relation, readme)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(path) DO UPDATE SET root_id = excluded.root_id, name = excluded.name,
               kind = excluded.kind, sha1 = excluded.sha1, size = excluded.size, mtime = excluded.mtime,
               source_size = excluded.source_size, source_crc32 = excluded.source_crc32,
               target_size = excluded.target_size, target_crc32 = excluded.target_crc32,
               names_source = excluded.names_source, hint = excluded.hint, relation = excluded.relation,
               readme = excluded.readme, system_id = NULL, title = NULL, paired_by = NULL",
            params![
                key,
                root_id,
                p.name,
                p.kind.name(),
                p.sha1,
                size,
                mtime,
                info.and_then(|i| i.source).map(|s| s.0),
                info.and_then(|i| i.source).map(|s| format!("{:08x}", s.1)),
                info.and_then(|i| i.target).map(|s| s.0),
                info.and_then(|i| i.target).map(|s| format!("{:08x}", s.1)),
                names_source,
                hint.join("\n"),
                guess_relation(&format!("{} {file_name}", p.name), &readme).as_str(),
                (!readme.is_empty()).then(|| readme.chars().take(4000).collect::<String>()),
            ],
        )?;
    }
    let ours: Vec<String> = conn
        .prepare("SELECT path FROM patch_files WHERE root_id = ?1")?
        .query_map([root_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for path in ours {
        if !found.contains(Path::new(&path)) && !kept.contains(&path) {
            conn.execute("DELETE FROM patch_files WHERE path = ?1", [&path])?;
        }
    }
    Ok(())
}

/// A file of the library, for pairing.
struct LibFile {
    path: String,
    member: String,
    status: String,
    size: u64,
    crc32: String,
    sha1: String,
    stripped: Option<(u64, String)>,
}

/// Pair every patch found with the game it is for, then name the files
/// that a patch beside them proves are that patch applied to a verified
/// copy of its game; re-identify when it named any. Run after identifying.
pub(crate) fn refresh(tx: &rusqlite::Transaction) -> Result<(), Error> {
    let any: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM patch_files)", [], |r| r.get(0))?;
    if !any {
        return Ok(());
    }
    pair(tx)?;
    if name_patched(tx)? > 0 {
        crate::identify::identify(tx, crate::identify::Relook::Nothing)?;
    }
    Ok(())
}

/// Pair each patch with a game: by the ROM it was made from (BPS / UPS:
/// size and CRC32, your identified copy first, else the one DAT game with
/// it), the file it names (xdelta), a game's file beside it of the same
/// name, its names (patch, zip, readme title) against your games' titles,
/// or a title of yours its readme names. Ambiguous stays unpaired: the patch is still offered in its
/// folder.
fn pair(conn: &Connection) -> Result<(), Error> {
    let rows = view::library(conn)?.rows;
    let mut game_of: std::collections::HashMap<i64, (i64, String)> = Default::default();
    let mut owned: std::collections::BTreeSet<(i64, String)> = Default::default();
    for r in &rows {
        if let (Some(sid), RowStatus::Verified | RowStatus::ByYou) = (r.system_id, r.status) {
            for id in r.file_ids() {
                game_of.insert(*id, (sid, r.title.clone()));
            }
            owned.insert((sid, r.title.clone()));
        }
    }
    let files: Vec<(i64, LibFile)> = conn
        .prepare("SELECT file_id, path, member, status, size, crc32, sha1, stripped_size, stripped_crc32 FROM files")?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                LibFile {
                    path: r.get(1)?,
                    member: r.get(2)?,
                    status: r.get(3)?,
                    size: r.get(4)?,
                    crc32: r.get(5)?,
                    sha1: r.get(6)?,
                    stripped: match (r.get::<_, Option<u64>>(7)?, r.get::<_, Option<String>>(8)?) {
                        (Some(s), Some(c)) => Some((s, c)),
                        _ => None,
                    },
                },
            ))
        })?
        .collect::<Result<_, _>>()?;
    let titles: Vec<String> = owned.iter().map(|(_, t)| t.clone()).collect();
    let mut dat_names: Option<(crate::titles::NameIndex, Vec<(i64, String)>)> = None;
    let regions = crate::settings::load(conn)?.region_order();
    let index = crate::titles::TitleIndex::new(titles.iter().cloned());
    let keys: Vec<((i64, String), crate::titles::TitleKey)> = owned
        .iter()
        .map(|g| (g.clone(), crate::titles::key(&g.1)))
        .collect();
    struct Found {
        path: String,
        name: String,
        source: Option<(u64, String)>,
        names_source: Option<String>,
        hint: String,
        readme: Option<String>,
    }
    let patches: Vec<Found> = conn
        .prepare("SELECT path, name, source_size, source_crc32, names_source, hint, readme FROM patch_files")?
        .query_map([], |r| {
            Ok(Found {
                path: r.get(0)?,
                name: r.get(1)?,
                source: match (r.get::<_, Option<u64>>(2)?, r.get::<_, Option<String>>(3)?) {
                    (Some(s), Some(c)) => Some((s, c)),
                    _ => None,
                },
                names_source: r.get(4)?,
                hint: r.get(5)?,
                readme: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let one = |set: std::collections::BTreeSet<(i64, String)>| {
        (set.len() == 1).then(|| set.into_iter().next()).flatten()
    };
    let mut dat_by_crc = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id WHERE r.size = ?1 AND r.crc32 = ?2 AND d.system_id IS NOT NULL",
    )?;
    let mut dat_by_name = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id WHERE r.name = ?1 COLLATE NOCASE AND d.system_id IS NOT NULL",
    )?;
    let mut set = conn.prepare_cached(
        "UPDATE patch_files SET system_id = ?2, title = ?3, paired_by = ?4 WHERE path = ?1",
    )?;
    for p in &patches {
        let dir = Path::new(&p.path).parent().map(Path::to_path_buf);
        let mut found: Option<((i64, String), &str)> = None;
        if let Some((size, crc)) = &p.source {
            let mine: std::collections::BTreeSet<_> = files
                .iter()
                .filter(|(_, f)| {
                    (f.size == *size && f.crc32 == *crc) || f.stripped.as_ref() == Some(&(*size, crc.clone()))
                })
                .filter_map(|(id, _)| game_of.get(id).cloned())
                .collect();
            let game = one(mine).or_else(|| {
                let dat: std::collections::BTreeSet<(i64, String)> = dat_by_crc
                    .query_map(params![size, crc], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map(|rows| rows.flatten().collect())
                    .unwrap_or_default();
                one(dat)
            });
            found = game.map(|g| (g, "source"));
        }
        if found.is_none()
            && let Some(source) = &p.names_source
        {
            let mine: std::collections::BTreeSet<_> = files
                .iter()
                .filter(|(_, f)| {
                    let base = if f.member.is_empty() { &f.path } else { &f.member };
                    base.rsplit(['/', '\\'])
                        .next()
                        .is_some_and(|b| b.eq_ignore_ascii_case(source))
                })
                .filter_map(|(id, _)| game_of.get(id).cloned())
                .collect();
            let game = one(mine).or_else(|| {
                let dat: std::collections::BTreeSet<(i64, String)> = dat_by_name
                    .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map(|rows| rows.flatten().collect())
                    .unwrap_or_default();
                one(dat)
            });
            found = game.map(|g| (g, "names"));
        }
        if found.is_none() {
            // A game's file beside it with the same name (the patch's, or
            // its zip's).
            let stems = [stem_lower(&p.name), stem_lower(&p.path)];
            let mine: std::collections::BTreeSet<_> = files
                .iter()
                .filter(|(_, f)| {
                    Path::new(&f.path).parent().map(Path::to_path_buf) == dir || f.path == p.path
                })
                .filter(|(_, f)| {
                    let name = if f.member.is_empty() { &f.path } else { &f.member };
                    stems.contains(&stem_lower(name))
                })
                .filter_map(|(id, _)| game_of.get(id).cloned())
                .collect();
            found = one(mine).map(|g| (g, "beside"));
        }
        if found.is_none() {
            for line in p.hint.lines() {
                let hits = index.find(line, 3);
                let Some((best, score)) = hits.first() else {
                    continue;
                };
                let tied = hits.get(1).is_some_and(|(_, s)| s >= score);
                if *score < 0.9 || tied {
                    continue;
                }
                let games: std::collections::BTreeSet<_> =
                    owned.iter().filter(|(_, t)| t == best).cloned().collect();
                if let Some(g) = one(games) {
                    found = Some((g, "title"));
                    break;
                }
            }
        }
        if found.is_none()
            && let Some(readme) = &p.readme
        {
            // A game's title named in its readme: the longest, if one.
            let text = crate::titles::text_words(readme);
            let mut best: (usize, std::collections::BTreeSet<(i64, String)>) = (0, Default::default());
            for (g, k) in &keys {
                let n = crate::titles::named_in(k, &text);
                if n > best.0 {
                    best = (n, Default::default());
                }
                if n > 0 && n == best.0 {
                    best.1.insert(g.clone());
                }
            }
            found = one(best.1).map(|g| (g, "readme"));
        }
        if found.is_none()
            && let Some(readme) = &p.readme
        {
            // A game you don't have: the DAT game its readme names.
            let (index, games) = dat_names.get_or_insert_with(|| dat_name_index(conn).unwrap_or_default());
            let text = crate::titles::text_words(readme);
            let hits: Vec<(i64, String)> = index.named(&text).into_iter().map(|i| games[i].clone()).collect();
            found = one_game(hits, readme, &regions).map(|g| (g, "readme"));
        }
        match found {
            Some(((sid, title), by)) => set.execute(params![p.path, sid, title, by])?,
            None => set.execute(params![p.path, None::<i64>, None::<String>, None::<String>])?,
        };
    }
    Ok(())
}

/// Every DAT game's name, indexed for [`crate::titles::NameIndex::named`],
/// with the games in its order.
fn dat_name_index(conn: &Connection) -> Result<(crate::titles::NameIndex, Vec<(i64, String)>), Error> {
    let all: Vec<(i64, String)> = conn
        .prepare(
            "SELECT DISTINCT d.system_id, g.name FROM games g JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id IS NOT NULL ORDER BY d.system_id, g.name",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let index = crate::titles::NameIndex::new(all.iter().map(|(_, t)| t.as_str()));
    Ok((index, all))
}

/// One game out of DAT games a readme names: versions of one title on one
/// platform are one game (its best release in your region order); several
/// platforms only when the readme names one of them.
fn one_game(hits: Vec<(i64, String)>, readme: &str, regions: &[String]) -> Option<(i64, String)> {
    let mut systems: Vec<i64> = hits.iter().map(|(s, _)| *s).collect();
    systems.sort_unstable();
    systems.dedup();
    if systems.len() > 1 {
        let text = readme.to_lowercase();
        let says = |words: &str| {
            let w = words.to_lowercase();
            w.len() >= 3 && text.contains(&w)
        };
        systems.retain(|id| {
            systems::by_id(*id)
                .is_some_and(|s| says(s.name) || says(s.key) || says(&crate::view::short_name(s.key)))
        });
    }
    let [sid] = systems.as_slice() else {
        return None;
    };
    let of: Vec<&String> = hits.iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect();
    let mut bases: Vec<String> = of.iter().map(|t| crate::organize::base_title(t)).collect();
    bases.sort();
    bases.dedup();
    if bases.len() != 1 {
        return None;
    }
    of.into_iter()
        .max_by_key(|t| (crate::organize::rank(t, regions), std::cmp::Reverse((*t).clone())))
        .map(|t| (*sid, t.clone()))
}

/// Name the unknown files that a paired patch beside them (in the same
/// folder or zip) proves are that patch applied to your verified copy of
/// its game: "<Game> [Translation: <tag>]", romforge naming it, never
/// verified. Tried on files the patch could have made: by its target's size
/// and CRC32 (BPS / UPS), else of the same name as the patch. Cartridges
/// only. Returns how many files it named.
fn name_patched(conn: &Connection) -> Result<usize, Error> {
    struct Paired {
        path: String,
        system_id: i64,
        title: String,
        relation: String,
        target: Option<(u64, String)>,
        name: String,
    }
    let patches: Vec<Paired> = conn
        .prepare(
            "SELECT path, system_id, title, relation, target_size, target_crc32, name FROM patch_files
             WHERE title IS NOT NULL",
        )?
        .query_map([], |r| {
            Ok(Paired {
                path: r.get(0)?,
                system_id: r.get(1)?,
                title: r.get(2)?,
                relation: r.get(3)?,
                target: match (r.get::<_, Option<u64>>(4)?, r.get::<_, Option<String>>(5)?) {
                    (Some(s), Some(c)) => Some((s, c)),
                    _ => None,
                },
                name: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    if patches.is_empty() {
        return Ok(0);
    }
    let unknown: Vec<LibFile> = conn
        .prepare(
            "SELECT path, member, status, size, crc32, sha1 FROM files f
             WHERE status = 'unknown'
               AND NOT EXISTS (SELECT 1 FROM manual_ids m WHERE m.sha1 = f.sha1 AND m.size = f.size)
               AND NOT EXISTS (SELECT 1 FROM proofs_declined x WHERE x.sha1 = f.sha1 AND x.size = f.size)",
        )?
        .query_map([], |r| {
            Ok(LibFile {
                path: r.get(0)?,
                member: r.get(1)?,
                status: r.get(2)?,
                size: r.get(3)?,
                crc32: r.get(4)?,
                sha1: r.get(5)?,
                stripped: None,
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut named = 0;
    for p in &patches {
        let Some(system) = systems::by_id(p.system_id) else {
            continue;
        };
        if system.is_disc_based || system.arcade || system.folders != systems::Folders::No {
            continue;
        }
        let dir = Path::new(&p.path).parent();
        let stems = [stem_lower(&p.name), stem_lower(&p.path)];
        let candidates: Vec<&LibFile> = unknown
            .iter()
            .filter(|f| f.status == "unknown" && (Path::new(&f.path).parent() == dir || f.path == p.path))
            .filter(|f| match &p.target {
                Some((size, crc)) => f.size == *size && f.crc32 == *crc,
                None => {
                    let name = if f.member.is_empty() { &f.path } else { &f.member };
                    stems.contains(&stem_lower(name))
                }
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let Some((base_path, base_member)) = crate::suggest::verified_copy(conn, p.system_id, &p.title)?
        else {
            continue;
        };
        let Some(base) = patch::read_entry(Path::new(&base_path), &base_member) else {
            continue;
        };
        let Ok(pf) = read_patch(Path::new(&p.path)) else {
            continue;
        };
        let mut outs: Vec<String> = Vec::new();
        for form in [Form::Raw, Form::Dat] {
            if let Some(out) = patch_rom(&pf.data, &base, form) {
                outs.push(cleanup::sha1_of(&out[..])?);
            }
        }
        let relation = Relation::parse(&p.relation).unwrap_or(Relation::Hack);
        let described = patch::ppf_records(&pf.data)
            .map(|x| x.description)
            .filter(|d| !d.trim().is_empty());
        let tag = described.unwrap_or_else(|| tag_of(&pf.name));
        let derived = Derived {
            relation,
            base: p.title.clone(),
            tag: (!tag.is_empty()).then_some(tag),
        };
        for f in candidates {
            if !outs.contains(&f.sha1) {
                continue;
            }
            named += conn.execute(
                concat!(
                    "INSERT INTO manual_ids (sha1, size, system_id, title, created_at, base, relation, tag, proof)
                     VALUES (?1, ?2, ?3, ?4, ",
                    crate::db::now!(),
                    ", ?5, ?6, ?7, ?8) ON CONFLICT(sha1, size) DO NOTHING"
                ),
                params![
                    f.sha1,
                    f.size,
                    p.system_id,
                    derived.title(),
                    derived.base,
                    relation.as_str(),
                    derived.tag,
                    pf.name
                ],
            )?;
        }
    }
    Ok(named)
}

/// What made a patched file: the patch, and the game it was applied to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PatchedFrom {
    pub base: String,
    pub patch: String,
    pub kind: String,
    pub created_at: String,
}

/// For a file romforge patched: what made it.
pub(crate) fn made_by(conn: &Connection, sha1: &str, size: u64) -> Result<Option<PatchedFrom>, Error> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT base, patch_name, kind, created_at FROM patched WHERE sha1 = ?1 AND size = ?2",
            params![sha1, size],
            |r| {
                Ok(PatchedFrom {
                    base: r.get(0)?,
                    patch: r.get(1)?,
                    kind: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_from_patch_names() {
        assert_eq!(tag_of("Mother3_v1.3_English.ips"), "Mother3 v1.3 English");
        assert_eq!(tag_of("patch.bps"), "patch");
    }
}
