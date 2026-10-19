//! Every file in what the user drops: a folder (all the way down), a file,
//! or a zip / 7z (with zips and 7z inside it, one level), read as streams
//! (house rule 7: no external tools, nothing extracted to disk). The same
//! walk plans an import (hashing) and applies it (copying), so a member is
//! found again by its [`SourceId`].

use std::io::{BufReader, Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use crate::Error;

/// A nested archive larger than this is not opened (it has to be read
/// into memory); BIOS files are small.
const MAX_NESTED: u64 = 256 << 20;

/// Where a file is: on disk, or inside an archive (members, outermost
/// first).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub struct SourceId {
    pub file: PathBuf,
    pub members: Vec<String>,
}

impl SourceId {
    /// As the user sees it: `pack.zip › bios/scph5501.bin`.
    pub fn shown(&self) -> String {
        std::iter::once(self.file.display().to_string())
            .chain(self.members.iter().cloned())
            .collect::<Vec<_>>()
            .join(" › ")
    }

    /// The file's own name, without folders.
    pub fn name(&self) -> String {
        match self.members.last() {
            Some(m) => m.rsplit('/').next().unwrap_or(m).to_owned(),
            None => self
                .file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

/// What a walk calls back with: each file's id, a reader over it, its
/// size.
pub type Each<'a> = dyn FnMut(&SourceId, &mut dyn Read, u64) -> Result<(), Error> + 'a;

/// Walk `paths`; a part that can't be read is reported in `skipped` and
/// the rest go on.
pub fn walk(paths: &[PathBuf], each: &mut Each, skipped: &mut Vec<(String, String)>) {
    for p in paths {
        if let Err(e) = walk_path(p, each, skipped) {
            skipped.push((p.display().to_string(), e.to_string()));
        }
    }
}

fn walk_path(p: &Path, each: &mut Each, skipped: &mut Vec<(String, String)>) -> Result<(), Error> {
    if p.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(p)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for e in entries {
            if let Err(err) = walk_path(&e, each, skipped) {
                skipped.push((e.display().to_string(), err.to_string()));
            }
        }
        return Ok(());
    }
    let id = SourceId {
        file: p.to_path_buf(),
        members: Vec::new(),
    };
    match kind(&id.name()) {
        Kind::Zip => zip(BufReader::new(std::fs::File::open(p)?), &id, each, skipped),
        Kind::SevenZ => seven(std::fs::File::open(p)?, &id, each, skipped),
        Kind::Slice => {
            skipped.push((id.shown(), "a slice of a split zip: join the parts first".into()));
            Ok(())
        }
        Kind::Plain => {
            let file = std::fs::File::open(p)?;
            let size = file.metadata()?.len();
            each(&id, &mut BufReader::new(file), size)
        }
    }
}

enum Kind {
    Zip,
    SevenZ,
    /// `.zip.001`: none opens on its own.
    Slice,
    Plain,
}

fn kind(name: &str) -> Kind {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        Kind::Zip
    } else if lower.ends_with(".7z") {
        Kind::SevenZ
    } else if lower
        .rsplit_once(".zip.")
        .is_some_and(|(_, n)| n.bytes().all(|b| b.is_ascii_digit()))
    {
        Kind::Slice
    } else {
        Kind::Plain
    }
}

fn child(id: &SourceId, member: &str) -> SourceId {
    let mut c = id.clone();
    c.members.push(member.to_owned());
    c
}

/// A member that is itself an archive, read into memory and walked, when
/// `id` is the outer file (one level only) and it is small enough.
fn nested(id: &SourceId, name: &str, size: u64) -> bool {
    id.members.is_empty() && size <= MAX_NESTED && matches!(kind(name), Kind::Zip | Kind::SevenZ)
}

fn inner(
    bytes: Vec<u8>,
    id: &SourceId,
    each: &mut Each,
    skipped: &mut Vec<(String, String)>,
) -> Result<(), Error> {
    match kind(&id.name()) {
        Kind::Zip => zip(Cursor::new(bytes), id, each, skipped),
        _ => seven(Cursor::new(bytes), id, each, skipped),
    }
}

fn zip<R: Read + Seek>(
    r: R,
    id: &SourceId,
    each: &mut Each,
    skipped: &mut Vec<(String, String)>,
) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(r)?;
    for i in 0..archive.len() {
        let mut m = match archive.by_index(i) {
            Ok(m) => m,
            Err(e) => {
                skipped.push((format!("{} › #{i}", id.shown()), e.to_string()));
                continue;
            }
        };
        if m.is_dir() {
            continue;
        }
        let name = m
            .name()
            .map(|n| n.into_owned())
            .unwrap_or_else(|_| format!("#{i}"));
        let c = child(id, &name);
        let size = m.size();
        let result = if nested(id, &name, size) {
            let mut bytes = Vec::with_capacity(size as usize);
            m.read_to_end(&mut bytes)?;
            inner(bytes, &c, each, skipped)
        } else {
            each(&c, &mut m, size)
        };
        if let Err(e) = result {
            skipped.push((c.shown(), e.to_string()));
        }
    }
    Ok(())
}

fn seven<R: Read + Seek>(
    r: R,
    id: &SourceId,
    each: &mut Each,
    skipped: &mut Vec<(String, String)>,
) -> Result<(), Error> {
    let mut archive = sevenz_rust2::ArchiveReader::new(r, sevenz_rust2::Password::empty())?;
    archive.for_each_entries(|entry, reader| {
        if entry.is_directory() {
            return Ok(true);
        }
        let c = child(id, entry.name());
        let size = entry.size();
        let result = if nested(id, entry.name(), size) {
            let mut bytes = Vec::with_capacity(size as usize);
            reader.read_to_end(&mut bytes)?;
            inner(bytes, &c, each, skipped)
        } else {
            each(&c, &mut *reader, size)
        };
        // A solid archive is one stream: read each member to its end.
        std::io::copy(reader, &mut std::io::sink())?;
        if let Err(e) = result {
            skipped.push((c.shown(), e.to_string()));
        }
        Ok(true)
    })?;
    Ok(())
}
