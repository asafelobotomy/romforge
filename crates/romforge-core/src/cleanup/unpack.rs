//! A ScummVM game still in its zip or 7z, unpacked into its folder: each
//! member streamed straight to its file (no temporary copy), then read
//! back and checked against what was streamed.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::Error;

use super::items::archive_entries;
use super::write::{copy_hashed, sha1_of};
use super::{Item, Part};

/// Unpack `archive` into `out` (which must not exist), as `item` planned:
/// the archive must hold just what was planned, nothing added since.
pub(super) fn unpack(item: &Item, archive: &Path, out: &Path) -> Result<(), Error> {
    let now: BTreeSet<String> = archive_entries(archive)?.into_iter().map(|(n, _)| n).collect();
    let planned: BTreeSet<String> = item.parts.iter().map(|p| p.member.clone()).collect();
    if now != planned {
        return Err(Error::Cleanup(format!(
            "{} changed since the plan ({} files planned, {} now); plan again",
            archive.display(),
            planned.len(),
            now.len()
        )));
    }
    std::fs::create_dir(out)?;
    let write = |p: &Part, r: &mut dyn Read| -> Result<(), Error> {
        let to = out.join(&p.name);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut w = File::create_new(&to)?;
        let sum = copy_hashed(r, &mut w)?;
        w.sync_all()?;
        if w.metadata()?.len() != p.size || sha1_of(BufReader::new(File::open(&to)?))? != sum {
            return Err(Error::Cleanup(format!(
                "{}: {} doesn't read back as unpacked",
                out.display(),
                p.name
            )));
        }
        Ok(())
    };
    if archive.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let mut zip = zip::ZipArchive::new(BufReader::new(File::open(archive)?))?;
        for p in &item.parts {
            write(p, &mut zip.by_name(&p.member)?)?;
        }
        return Ok(());
    }
    let by_member: HashMap<&str, &Part> = item.parts.iter().map(|p| (p.member.as_str(), p)).collect();
    let mut failed = None;
    let mut written = 0;
    let mut a = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())?;
    a.for_each_entries(|entry, reader| {
        match by_member.get(entry.name()) {
            Some(p) if !entry.is_directory() => match write(p, reader) {
                Ok(()) => written += 1,
                Err(e) => {
                    failed = Some(e);
                    return Ok(false);
                }
            },
            // A solid archive is one stream: each member is read to its end.
            _ => {
                std::io::copy(reader, &mut std::io::sink())?;
            }
        }
        Ok(true)
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    if written != item.parts.len() {
        return Err(Error::Cleanup(format!(
            "{}: {} of {} files unpacked",
            archive.display(),
            written,
            item.parts.len()
        )));
    }
    Ok(())
}
