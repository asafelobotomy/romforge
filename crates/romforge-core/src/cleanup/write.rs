use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::hash::{hash_stream, hex};
use crate::organize::{move_file, sanitize};
use crate::{Error, layout};

use super::apply::journal_output;
use super::items::{archive_names, safe_member_name};
use super::parts::open_part;
use super::{Item, Part};

/// A reader that hashes what passes through it.
pub(super) struct Sha1Read<R> {
    inner: R,
    sha1: sha1::Sha1,
}

impl<R: Read> Read for Sha1Read<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use sha1::Digest as _;
        let n = self.inner.read(buf)?;
        self.sha1.update(&buf[..n]);
        Ok(n)
    }
}

/// The SHA1 of a whole stream.
pub(crate) fn sha1_of(r: impl Read) -> std::io::Result<String> {
    use sha1::Digest as _;
    let mut h = Sha1Read {
        inner: r,
        sha1: sha1::Sha1::new(),
    };
    std::io::copy(&mut h, &mut std::io::sink())?;
    Ok(hex(&h.sha1.finalize()))
}

/// Copy `r` into `w`, returning the SHA1 of what was copied.
pub(crate) fn copy_hashed(r: impl Read, w: &mut impl Write) -> std::io::Result<String> {
    use sha1::Digest as _;
    let mut h = Sha1Read {
        inner: r,
        sha1: sha1::Sha1::new(),
    };
    std::io::copy(&mut h, w)?;
    Ok(hex(&h.sha1.finalize()))
}

/// One disk of a set as a plain file in the set's folder, checked against
/// its hash; its art and details go in `romforge/<title>/` there.
pub(super) fn write_loose(
    conn: &Connection,
    batch: i64,
    item: &Item,
    extras: &[(String, Vec<u8>)],
    staging: &Path,
) -> Result<(), Error> {
    let p = item
        .parts
        .first()
        .ok_or_else(|| Error::Cleanup(format!("{}: no disk", item.title)))?;
    let dir = item.target.parent().unwrap_or(Path::new("."));
    let name = item.target.file_name().unwrap_or_default().to_string_lossy();
    let part = dir.join(layout::part(&name));
    let _ = std::fs::remove_file(&part);
    let result = (|| -> Result<(), Error> {
        let mut w = File::create_new(&part)?;
        copy_hashed(open_part(p, staging)?, &mut w)?;
        w.sync_all()?;
        if sha1_of(BufReader::new(File::open(&part)?))? != p.sha1 {
            return Err(Error::Cleanup(format!(
                "{}: {} doesn't hash as before",
                dir.display(),
                name
            )));
        }
        if item.target.exists() {
            return Err(Error::Cleanup(format!(
                "{} already exists",
                item.target.display()
            )));
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &item.target)?;
    journal_output(conn, batch, &item.target, "file")?;
    let side = dir.join(layout::EXTRAS_DIR).join(sanitize(&item.title));
    if !extras.is_empty() && !side.exists() {
        for d in [dir.join(layout::EXTRAS_DIR), side.clone()] {
            if !d.exists() {
                std::fs::create_dir(&d)?;
                journal_output(conn, batch, &d, "setdir")?;
            }
        }
        for (f, data) in extras {
            let to = side.join(f);
            std::fs::write(&to, data)?;
            journal_output(conn, batch, &to, "file")?;
        }
    }
    Ok(())
}

/// A folder game (or the archive holding it) into one zip, every file
/// The settings of every zip romforge writes: no clock (1980-01-01), the
/// same permissions and compression for every member, so the same game
/// always zips to the same bytes (as TorrentZip does; with
/// [`zip_order`]). Zip64 only for a member that needs it.
pub(crate) fn zip_options(size: u64) -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644)
        .large_file(size >= u32::MAX as u64)
}

/// The order of a zip's members: by lower-case name (then exact name).
pub(super) fn zip_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
}

/// Art and details into a zip's `romforge/` folder, in order.
pub(crate) fn write_extras(
    zip: &mut zip::ZipWriter<File>,
    extras: &[(String, Vec<u8>)],
) -> Result<(), Error> {
    let mut sorted: Vec<&(String, Vec<u8>)> = extras.iter().collect();
    sorted.sort_by(|a, b| zip_order(&a.0, &b.0));
    for (name, data) in sorted {
        zip.start_file(
            format!("{}/{name}", layout::EXTRAS_DIR),
            zip_options(data.len() as u64),
        )?;
        zip.write_all(data)?;
    }
    Ok(())
}

/// A folder game (or the archive holding it) into one zip, every file
/// under its own path, in [`zip_order`], then read back: each must hash as
/// it was read. A 7z (one compressed stream, read in its own order) is
/// unpacked into `staging` first.
pub(super) fn write_folder_zip(
    item: &Item,
    extras: &[(String, Vec<u8>)],
    staging: &Path,
    out: &Path,
) -> Result<(), Error> {
    let src = item.folder.as_deref().unwrap_or(Path::new(""));
    let source_is_zip =
        |p: &Path| !p.is_dir() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    // (name in the zip, file on disk or member of the source zip, size)
    let mut members: Vec<(String, PathBuf, u64)> = Vec::new();
    let unpacked = staging.join("folder");
    if src.is_dir() {
        members.extend(
            item.parts
                .iter()
                .map(|p| (p.name.clone(), p.path.clone(), p.size)),
        );
    } else if source_is_zip(src) {
        members.extend(
            item.parts
                .iter()
                .map(|p| (p.name.clone(), PathBuf::new(), p.size)),
        );
    } else {
        std::fs::create_dir_all(&unpacked)?;
        let mut a = sevenz_rust2::ArchiveReader::open(src, sevenz_rust2::Password::empty())?;
        a.for_each_entries(|entry, reader| {
            if !entry.is_directory() && safe_member_name(entry.name()) {
                let to = unpacked.join(entry.name());
                if let Some(d) = to.parent() {
                    std::fs::create_dir_all(d)?;
                }
                std::io::copy(reader, &mut File::create(&to)?)?;
            }
            Ok(true)
        })?;
        members.extend(
            item.parts
                .iter()
                .map(|p| (p.name.clone(), unpacked.join(&p.name), p.size)),
        );
    }
    members.sort_by(|a, b| zip_order(&a.0, &b.0));
    // The folder goes to the trash whole: it must hold just what was
    // planned, nothing added since.
    let now: BTreeSet<String> = if src.is_dir() {
        walkdir::WalkDir::new(src)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                e.path()
                    .strip_prefix(src)
                    .ok()
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
            })
            .collect()
    } else if source_is_zip(src) {
        archive_names(src)?.into_iter().collect()
    } else {
        walkdir::WalkDir::new(&unpacked)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                e.path()
                    .strip_prefix(&unpacked)
                    .ok()
                    .map(|r| r.to_string_lossy().into_owned())
            })
            .collect()
    };
    let planned: BTreeSet<String> = item.parts.iter().map(|p| p.name.clone()).collect();
    if now != planned {
        let _ = std::fs::remove_dir_all(&unpacked);
        return Err(Error::Cleanup(format!(
            "{} changed since the plan ({} files planned, {} now); plan again",
            src.display(),
            planned.len(),
            now.len()
        )));
    }
    let mut source_zip = match source_is_zip(src) {
        true => Some(zip::ZipArchive::new(BufReader::new(File::open(src)?))?),
        false => None,
    };
    let mut zip = zip::ZipWriter::new(File::create_new(out)?);
    let mut sums: Vec<(String, String)> = Vec::new();
    for (name, path, size) in &members {
        zip.start_file(name, zip_options(*size))?;
        let sum = match source_zip.as_mut() {
            Some(a) => copy_hashed(a.by_name(name)?, &mut zip)?,
            None => copy_hashed(
                BufReader::new(File::open(path).map_err(|e| {
                    Error::Cleanup(format!("{} changed since the plan: {name}: {e}", src.display()))
                })?),
                &mut zip,
            )?,
        };
        sums.push((name.clone(), sum));
    }
    write_extras(&mut zip, extras)?;
    zip.finish()?.sync_all()?;
    let _ = std::fs::remove_dir_all(&unpacked);
    let mut back = zip::ZipArchive::new(BufReader::new(File::open(out)?))?;
    for (name, sum) in &sums {
        if sha1_of(back.by_name(name)?)? != *sum {
            return Err(Error::Cleanup(format!(
                "{}: {name} doesn't hash as before",
                out.display()
            )));
        }
    }
    Ok(())
}

/// A folder game copied as a folder into `out` (which must not exist),
/// each file read back and checked; its art and details go in
/// `romforge/`, and a ScummVM game gets its `<id>.scummvm`.
pub(super) fn copy_folder(item: &Item, extras: &[(String, Vec<u8>)], out: &Path) -> Result<(), Error> {
    std::fs::create_dir(out)?;
    for p in &item.parts {
        let to = out.join(&p.name);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut w = File::create_new(&to)?;
        let sum = copy_hashed(BufReader::new(File::open(&p.path)?), &mut w)?;
        w.sync_all()?;
        if sha1_of(BufReader::new(File::open(&to)?))? != sum {
            return Err(Error::Cleanup(format!(
                "{}: {} doesn't hash as before",
                out.display(),
                p.name
            )));
        }
    }
    if !extras.is_empty() && !out.join(layout::EXTRAS_DIR).exists() {
        std::fs::create_dir(out.join(layout::EXTRAS_DIR))?;
        for (name, data) in extras {
            std::fs::write(out.join(layout::EXTRAS_DIR).join(name), data)?;
        }
    }
    if let Some(code) = &item.code {
        let has = item
            .parts
            .iter()
            .any(|p| p.name.to_ascii_lowercase().ends_with(".scummvm"));
        if !has {
            std::fs::write(out.join(format!("{}.scummvm", sanitize(code))), code)?;
        }
    }
    Ok(())
}

/// Move a file, or a folder whole (one rename on the same drive, else file
/// by file).
pub(super) fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.is_dir() {
        return move_file(from, to).map(|_| ());
    }
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} exists", to.display()),
        ));
    }
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p)?;
    }
    match std::fs::rename(from, to) {
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => crate::home::move_tree(from, to)
            .map(|_| ())
            .map_err(|e| std::io::Error::other(e.to_string())),
        r => r,
    }
}

pub(super) fn write_zip(
    item: &Item,
    extras: &[(String, Vec<u8>)],
    staging: &Path,
    out: &Path,
) -> Result<(), Error> {
    let mut zip = zip::ZipWriter::new(File::create_new(out)?);
    let mut parts: Vec<&Part> = item.parts.iter().collect();
    parts.sort_by(|a, b| zip_order(&a.name, &b.name));
    for p in parts {
        zip.start_file(&p.name, zip_options(p.size))?;
        let mut r = open_part(p, staging)?;
        std::io::copy(&mut r, &mut zip)?;
    }
    write_extras(&mut zip, extras)?;
    zip.finish()?.sync_all()?;
    // Read it back: every ROM must hash as before.
    let mut back = zip::ZipArchive::new(BufReader::new(File::open(out)?))?;
    for p in &item.parts {
        let mut m = back.by_name(&p.name)?;
        let size = m.size();
        let h = hash_stream(&mut m, size)?;
        if h.raw.sha1 != p.sha1 {
            return Err(Error::Cleanup(format!(
                "{}: {} doesn't hash as before",
                out.display(),
                p.name
            )));
        }
    }
    Ok(())
}
