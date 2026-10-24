//! Milestone 23: writing a game as RVZ, Z3DS or WUA, then reading the new
//! file back in full: it must hold exactly the bytes it was made from (the
//! part's SHA-1, or each file of a title folder), or it is removed and the
//! original stays.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::Error;

use super::parts::open_part;
use super::write::sha1_of;
use super::{Format, Item};

/// Items are written several at once already: each uses two threads.
const THREADS: usize = 2;

fn failed(out: &Path, what: impl std::fmt::Display) -> Error {
    Error::Cleanup(format!("{}: {what}", out.display()))
}

/// Write `item` to `out` and check it. For a WUA, which is hashed as
/// itself, its SHA-1 and size (worked out as it is written).
pub(crate) fn write_compressed(
    item: &Item,
    staging: &Path,
    out: &Path,
) -> Result<Option<(String, u64)>, Error> {
    match item.format {
        Format::Rvz => rvz(item, staging, out).map(|()| None),
        Format::Z3ds => z3ds(item, staging, out).map(|()| None),
        Format::Wua => wua(item, out).map(Some),
        _ => Err(failed(out, "not a compressed format")),
    }
}

/// A file being written, hashed as it goes.
struct Hashed {
    file: File,
    sha: sha1::Sha1,
    size: u64,
}

impl std::io::Write for Hashed {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        sha1::Digest::update(&mut self.sha, &buf[..n]);
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

fn rvz(item: &Item, staging: &Path, out: &Path) -> Result<(), Error> {
    let part = item.parts.first().ok_or_else(|| failed(out, "no disc image"))?;
    let mut file = File::create_new(out)?;
    let open = || open_part(part, staging).map_err(|e| std::io::Error::other(e.to_string()));
    crate::rvz::write::write(open, part.size, THREADS, &mut file)?;
    file.sync_all()?;
    drop(file);
    let back = crate::rvz::Rvz::open(BufReader::new(File::open(out)?))?
        .ok_or_else(|| failed(out, "doesn't read back as RVZ"))?;
    if sha1_of(back.iso()?)? != part.sha1 {
        return Err(failed(out, "its image doesn't hash as the original"));
    }
    Ok(())
}

fn z3ds(item: &Item, staging: &Path, out: &Path) -> Result<(), Error> {
    let part = item.parts.first().ok_or_else(|| failed(out, "no 3DS file"))?;
    let mut head = vec![0u8; crate::ident::PREFIX_LEN];
    let n = crate::chunks::read_full(&mut open_part(part, staging)?, &mut head)?;
    let form = crate::ident::z3ds_form(&head[..n])
        .filter(|f| f.decrypted)
        .ok_or_else(|| failed(out, "not a decrypted 3DS file"))?;
    // A CIA's 32 MiB frames: one at a time.
    let threads = if form.frame > 1 << 20 { 1 } else { THREADS };
    let mut file = File::create_new(out)?;
    crate::z3ds::write(
        open_part(part, staging)?,
        part.size,
        form.magic,
        form.frame,
        threads,
        &mut file,
    )?;
    file.sync_all()?;
    drop(file);
    let (_, inner) = crate::z3ds::open(BufReader::new(File::open(out)?))?
        .ok_or_else(|| failed(out, "doesn't read back as Z3DS"))?;
    if sha1_of(inner)? != part.sha1 {
        return Err(failed(out, "it doesn't hash as the original"));
    }
    Ok(())
}

/// Every file under `dir`, as (path relative to it, `/`-separated; path).
fn files_under(dir: &Path) -> Result<Vec<(String, PathBuf)>, Error> {
    let mut out = Vec::new();
    for e in walkdir::WalkDir::new(dir).follow_links(false).sort_by_file_name() {
        let e = e.map_err(std::io::Error::from)?;
        if e.file_type().is_file() {
            let rel = e.path().strip_prefix(dir).unwrap_or(e.path());
            out.push((rel.to_string_lossy().replace('\\', "/"), e.path().to_owned()));
        }
    }
    Ok(out)
}

fn wua(item: &Item, out: &Path) -> Result<(String, u64), Error> {
    let dir = item
        .folder
        .as_deref()
        .ok_or_else(|| failed(out, "no title folder"))?;
    let meta = std::fs::read_to_string(dir.join("meta/meta.xml"))?;
    let top = crate::ident::wup::wua_folder(&meta).ok_or_else(|| failed(out, "no title ID in meta.xml"))?;
    let files: Vec<(String, PathBuf)> = files_under(dir)?
        .into_iter()
        .map(|(rel, path)| (format!("{top}/{rel}"), path))
        .collect();
    let mut file = Hashed {
        file: File::create_new(out)?,
        sha: sha1::Digest::new(),
        size: 0,
    };
    crate::wua::write(&files, THREADS, &mut file)?;
    file.file.sync_all()?;
    let made = (crate::hash::hex(&sha1::Digest::finalize(file.sha)), file.size);
    let mut back = crate::wua::Archive::open(BufReader::new(File::open(out)?))
        .ok_or_else(|| failed(out, "doesn't read back as a WUA"))?;
    if back.files().len() != files.len() {
        return Err(failed(out, "it holds a different number of files"));
    }
    for (name, path) in &files {
        let inside = back
            .open_file(name)
            .ok_or_else(|| failed(out, format!("{name} is missing")))?;
        if sha1_of(inside)? != sha1_of(BufReader::new(File::open(path)?))? {
            return Err(failed(out, format!("{name} doesn't hash as the original")));
        }
    }
    Ok(made)
}
