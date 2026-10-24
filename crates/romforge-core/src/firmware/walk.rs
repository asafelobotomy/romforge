//! Every file in what the user drops: a folder (all the way down), a file,
//! or a zip / 7z (with zips and 7z inside it, one level), read as streams
//! (house rule 7: no external tools, nothing extracted to disk). The same
//! walk plans an import (hashing) and applies it (copying), so a member is
//! found again by its [`SourceId`]. An archive that is itself a file an
//! emulator loads (a MAME machine's `fm7.zip`) is also given as a whole.

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

    /// The archive it is a member of, if any.
    pub fn parent(&self) -> Option<SourceId> {
        let (_, up) = self.members.split_last()?;
        Some(SourceId {
            file: self.file.clone(),
            members: up.to_vec(),
        })
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

/// Whether an archive of this name is also given whole (as well as its
/// members).
pub type Whole<'a> = dyn Fn(&str) -> bool + 'a;

/// Walk `paths`; a part that can't be read is reported in `skipped` and
/// the rest go on.
pub fn walk(paths: &[PathBuf], whole: &Whole, each: &mut Each, skipped: &mut Vec<(String, String)>) {
    let mut w = Walker { whole, each, skipped };
    for p in paths {
        if let Err(e) = w.path(p) {
            w.skipped.push((p.display().to_string(), e.to_string()));
        }
    }
}

struct Walker<'w, 'a, 'b> {
    whole: &'w Whole<'a>,
    each: &'w mut Each<'b>,
    skipped: &'w mut Vec<(String, String)>,
}

impl Walker<'_, '_, '_> {
    fn path(&mut self, p: &Path) -> Result<(), Error> {
        if p.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(p)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            entries.sort();
            for e in entries {
                if let Err(err) = self.path(&e) {
                    self.skipped.push((e.display().to_string(), err.to_string()));
                }
            }
            return Ok(());
        }
        let id = SourceId {
            file: p.to_path_buf(),
            members: Vec::new(),
        };
        let kind = kind(&id.name());
        if matches!(kind, Kind::Zip | Kind::SevenZ) {
            let file = std::fs::File::open(p)?;
            let size = file.metadata()?.len();
            self.whole(&id, &mut BufReader::new(file), size);
        }
        match kind {
            Kind::Zip => self.zip(BufReader::new(std::fs::File::open(p)?), &id),
            Kind::SevenZ => self.seven(std::fs::File::open(p)?, &id),
            Kind::Slice => {
                self.skipped
                    .push((id.shown(), "a slice of a split zip: join the parts first".into()));
                Ok(())
            }
            Kind::Plain => {
                let file = std::fs::File::open(p)?;
                let size = file.metadata()?.len();
                (self.each)(&id, &mut BufReader::new(file), size)
            }
        }
    }

    /// `r` given whole when its name says so; a failure is reported and the
    /// walk goes on into its members.
    fn whole(&mut self, id: &SourceId, r: &mut dyn Read, size: u64) {
        if (self.whole)(&id.name())
            && let Err(e) = (self.each)(id, r, size)
        {
            self.skipped.push((id.shown(), e.to_string()));
        }
    }

    /// A nested archive's bytes: given whole when its name says so, then
    /// walked.
    fn inner(&mut self, bytes: Vec<u8>, id: &SourceId) -> Result<(), Error> {
        self.whole(id, &mut Cursor::new(&bytes), bytes.len() as u64);
        match kind(&id.name()) {
            Kind::Zip => self.zip(Cursor::new(bytes), id),
            _ => self.seven(Cursor::new(bytes), id),
        }
    }

    fn zip<R: Read + Seek>(&mut self, r: R, id: &SourceId) -> Result<(), Error> {
        let mut archive = zip::ZipArchive::new(r)?;
        for i in 0..archive.len() {
            let mut m = match archive.by_index(i) {
                Ok(m) => m,
                Err(e) => {
                    self.skipped
                        .push((format!("{} › #{i}", id.shown()), e.to_string()));
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
                self.inner(bytes, &c)
            } else {
                (self.each)(&c, &mut m, size)
            };
            if let Err(e) = result {
                self.skipped.push((c.shown(), e.to_string()));
            }
        }
        Ok(())
    }

    fn seven<R: Read + Seek>(&mut self, r: R, id: &SourceId) -> Result<(), Error> {
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
                self.inner(bytes, &c)
            } else {
                (self.each)(&c, &mut *reader, size)
            };
            // A solid archive is one stream: read each member to its end.
            std::io::copy(reader, &mut std::io::sink())?;
            if let Err(e) = result {
                self.skipped.push((c.shown(), e.to_string()));
            }
            Ok(true)
        })?;
        Ok(())
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
