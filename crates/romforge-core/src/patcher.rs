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
//! - Discs: xdelta or PPF applied, streamed, to the first track (or the
//!   image), the others kept, and written as a CHD. A CHD is unpacked to
//!   staging first, with a cue sheet from its own track list.
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

fn is_patch(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, e)| patch::EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
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
        let patches: Vec<&String> = names.iter().filter(|n| is_patch(n)).collect();
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
    relation: Relation,
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
    let (target, applies_to, form) = if disc {
        if !matches!(p.kind, Kind::Xdelta | Kind::Ppf) {
            notes.push(format!(
                "a {} patch on a disc: applied to the first track in memory, which only works for small images",
                p.kind.name()
            ));
        }
        let first = parts
            .iter()
            .find(|x| !x.name.to_lowercase().ends_with(".cue"))
            .ok_or_else(|| bad(format!("{}: no track", row.title)))?;
        (
            dir.join(format!("{}.chd", sanitize(&title))),
            format!("{} (the first track of {})", first.name, row.title),
            Form::Raw,
        )
    } else {
        let roms: Vec<&Part> = parts.iter().collect();
        let [rom] = roms.as_slice() else {
            return Err(bad(format!(
                "{} is made of {} ROMs; romforge patches games of one",
                row.title,
                roms.len()
            )));
        };
        let mut data = Vec::new();
        cleanup::open_part(rom, staging)?.read_to_end(&mut data)?;
        let dat = dat_form(&data);
        let info = patch::info(&p.data).ok_or_else(|| bad("a damaged patch".into()))?;
        let form = match (info.source, &dat) {
            (Some((size, crc)), _) if size == data.len() as u64 && crc == crc32fast::hash(&data) => Form::Raw,
            (Some((size, crc)), Some((_, d))) if size == d.len() as u64 && crc == crc32fast::hash(d) => {
                Form::Dat
            }
            (Some((size, crc)), _) => {
                return Err(bad(format!(
                    "{} was made for a ROM of {size} bytes, CRC32 {crc:08x}; your {} isn't that one \
                     (another revision or region?)",
                    p.name, row.title
                )));
            }
            (None, None) => Form::Raw,
            (None, Some(_)) => {
                let n = p.name.to_ascii_lowercase();
                let headered = n.contains("header")
                    && !n.contains("headerless")
                    && !n.contains("unheader")
                    && !n.contains("no header")
                    && !n.contains("no-header");
                if headered {
                    notes.push("its name says it is for a headered ROM: applied to the file as it is".into());
                    Form::Raw
                } else {
                    notes.push(format!(
                        "{} says nothing about its source: applied to the DAT's form (no copier header)",
                        p.kind.name()
                    ));
                    Form::Dat
                }
            }
        };
        (
            dir.join(format!("{}.zip", sanitize(&title))),
            format!(
                "{} ({})",
                rom.name,
                if form == Form::Raw {
                    "as it is"
                } else {
                    "the DAT's form"
                }
            ),
            form,
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
               base = excluded.base, relation = excluded.relation, tag = excluded.tag"
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
    let rom = &plan.parts[0];
    let mut data = Vec::new();
    cleanup::open_part(rom, staging)?.read_to_end(&mut data)?;
    let patched = match (plan.form, dat_form(&data)) {
        (Form::Dat, Some((rule, body))) => {
            let new = patch::apply(&p.data, &body)
                .ok_or_else(|| Error::Cleanup(format!("{} doesn't apply to {}", p.name, plan.base)))?;
            // A copier header goes back on; a byte order stays the DAT's.
            if rule.reorder == crate::headers::Reorder::None {
                let mut v = data[..rule.len].to_vec();
                v.extend_from_slice(&new);
                v
            } else {
                new
            }
        }
        _ => patch::apply(&p.data, &data)
            .ok_or_else(|| Error::Cleanup(format!("{} doesn't apply to {}", p.name, plan.base)))?,
    };
    if patched == data {
        return Err(Error::Cleanup(format!(
            "{} changes nothing in {}",
            p.name, plan.base
        )));
    }
    let ext = Path::new(&rom.name)
        .extension()
        .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
    let member = format!("{}{ext}", sanitize(&plan.title));
    let mut extras = vec![(
        format!("{}{}", sanitize(&plan.title), layout::DETAILS_SUFFIX),
        details(plan, p).into_bytes(),
    )];
    if let Some((n, d)) = &p.readme {
        extras.push((n.clone(), d.clone()));
    }
    let mut zip = zip::ZipWriter::new(std::fs::File::create_new(out)?);
    zip.start_file(&member, cleanup::zip_options(patched.len() as u64))?;
    zip.write_all(&patched)?;
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

/// The new game's details file.
fn details(plan: &PatchPlan, p: &PatchFile) -> String {
    let base_sha1: Option<String> = plan
        .parts
        .iter()
        .find(|x| !x.name.to_lowercase().ends_with(".cue"))
        .map(|x| x.sha1.clone());
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
    if let Some(chd) = parts
        .iter()
        .find(|x| x.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd")))
        .map(|x| x.path.clone())
    {
        parts = unpack_chd(&chd, staging)?;
    }
    let first = parts
        .iter()
        .position(|x| !x.name.to_lowercase().ends_with(".cue"))
        .ok_or_else(|| Error::Cleanup(format!("{}: no track", plan.base)))?;
    let source = on_disk(&parts[first], staging)?;
    let patched = staging.join("patched.bin");
    patch::apply_file(&p.data, &source, &patched)?;
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

/// Patches beside a game's files (same name, or in its folder), to offer.
pub fn beside(conn: &Connection, row_id: &str) -> Result<Vec<PathBuf>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut out: Vec<PathBuf> = Vec::new();
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
            if p.is_file() && is_patch(&name) && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
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
