//! Walks a folder and hashes likely ROMs (by extension, see `systems.rs`),
//! including zip/7z members, as streams.
//! Nothing is extracted to disk and no external tool is run.
//!
//! Files and archives are hashed on several threads, largest first; members
//! of one archive are read in order on one thread (a solid 7z is a single
//! stream). Results reach the caller on the calling thread.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};

use crate::hash::{Hashes, hash_stream};
use crate::{Error, systems};

/// What to hash, and how.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// Hash every file and member, not only those with a known ROM extension.
    pub all_files: bool,
    /// Hashing threads; 0 picks one per CPU, at most 8.
    pub jobs: usize,
}

impl ScanOptions {
    fn threads(self) -> usize {
        match self.jobs {
            0 => std::thread::available_parallelism().map_or(1, |n| n.get().min(8)),
            n => n,
        }
    }
}

/// A file to hash, or an archive whose members to hash.
enum Unit {
    File(PathBuf),
    Zip(PathBuf),
    SevenZ(PathBuf),
}

#[derive(Debug)]
pub struct Entry {
    /// The file on disk (the archive, for members).
    pub path: PathBuf,
    /// Path inside the archive, or empty for plain files.
    pub member: String,
    pub hashes: Result<Hashes, String>,
}

#[derive(Debug)]
pub enum Visit {
    Hashed(Entry),
    /// A file or archive member without a ROM extension.
    Skipped {
        path: PathBuf,
        member: String,
    },
    /// A symlinked folder inside the tree; never followed.
    SkippedLink(PathBuf),
}

fn ext_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_archive(ext: &str) -> bool {
    ext == "zip" || ext == "7z"
}

/// Whether a file (or archive member) with this name should be hashed.
/// Members are never archives themselves: nested archives are not opened.
fn wanted(name: &str, opts: ScanOptions) -> bool {
    opts.all_files || systems::is_rom_extension(&ext_of(name))
}

/// Visit everything under `root`: skipped files and links in path order
/// while walking, then hashed files as threads finish them. `root` itself may be a
/// symlink; symlinked folders below it are reported and not followed, so a
/// link loop or a link to another drive cannot make a scan run away.
/// Symlinked files are hashed like normal files.
pub fn walk(
    root: &Path,
    skip: &[PathBuf],
    opts: ScanOptions,
    mut each: impl FnMut(Visit),
) -> Result<(), Error> {
    let mut units: Vec<(u64, Unit)> = Vec::new();
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .follow_root_links(true)
        .sort_by_file_name();
    for item in walker {
        let item = match item {
            Ok(item) => item,
            Err(e) => {
                each(Visit::Hashed(Entry {
                    path: e.path().map(Path::to_path_buf).unwrap_or_default(),
                    member: String::new(),
                    hashes: Err(e.to_string()),
                }));
                continue;
            }
        };
        let path = item.path().to_path_buf();
        if item.path_is_symlink() && item.depth() > 0 {
            match std::fs::metadata(&path) {
                Ok(m) if m.is_file() => {}
                Ok(m) if m.is_dir() => {
                    each(Visit::SkippedLink(path));
                    continue;
                }
                // Broken links and sockets etc.: nothing to hash.
                _ => continue,
            }
        } else if !item.file_type().is_file() {
            continue;
        }
        if skip.contains(&path) {
            continue;
        }
        let ext = ext_of(&path.to_string_lossy());
        if !is_archive(&ext) && !wanted(&path.to_string_lossy(), opts) {
            each(Visit::Skipped {
                path,
                member: String::new(),
            });
            continue;
        }
        let size = item.metadata().map_or(0, |m| m.len());
        units.push((
            size,
            match ext.as_str() {
                "zip" => Unit::Zip(path),
                "7z" => Unit::SevenZ(path),
                _ => Unit::File(path),
            },
        ));
    }

    // Largest first, so one big archive doesn't start last and run alone.
    units.sort_by_key(|u| std::cmp::Reverse(u.0));
    let threads = opts.threads().min(units.len());
    if threads <= 1 {
        for (_, unit) in units {
            hash_unit(unit, opts, &mut each);
        }
        return Ok(());
    }
    let queue = Mutex::new(units.into_iter().map(|(_, u)| u));
    let (tx, rx) = mpsc::sync_channel::<Visit>(256);
    std::thread::scope(|s| {
        for _ in 0..threads {
            let tx = tx.clone();
            let queue = &queue;
            s.spawn(move || {
                loop {
                    let next = queue.lock().unwrap_or_else(|e| e.into_inner()).next();
                    let Some(unit) = next else { break };
                    hash_unit(unit, opts, &mut |v| {
                        let _ = tx.send(v);
                    });
                }
            });
        }
        drop(tx);
        for visit in rx {
            each(visit);
        }
    });
    Ok(())
}

fn hash_unit(unit: Unit, opts: ScanOptions, each: &mut impl FnMut(Visit)) {
    let (path, result) = match unit {
        Unit::Zip(path) => {
            let r = walk_zip(&path, opts, each);
            (path, r)
        }
        Unit::SevenZ(path) => {
            let r = walk_7z(&path, opts, each);
            (path, r)
        }
        Unit::File(path) => {
            let hashes = hash_file(&path).map_err(|e| e.to_string());
            each(Visit::Hashed(Entry {
                path,
                member: String::new(),
                hashes,
            }));
            return;
        }
    };
    if let Err(e) = result {
        each(Visit::Hashed(Entry {
            path,
            member: String::new(),
            hashes: Err(e.to_string()),
        }));
    }
}

fn hash_file(path: &Path) -> std::io::Result<Hashes> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    hash_stream(BufReader::new(file), size)
}

fn walk_zip(path: &Path, opts: ScanOptions, each: &mut impl FnMut(Visit)) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    for i in 0..archive.len() {
        let mut member = archive.by_index(i)?;
        if member.is_dir() {
            continue;
        }
        let name = member.name()?.into_owned();
        if !wanted(&name, opts) {
            each(Visit::Skipped {
                path: path.to_path_buf(),
                member: name,
            });
            continue;
        }
        let size = member.size();
        let hashes = hash_stream(&mut member, size).map_err(|e| e.to_string());
        each(Visit::Hashed(Entry {
            path: path.to_path_buf(),
            member: name,
            hashes,
        }));
    }
    Ok(())
}

fn walk_7z(path: &Path, opts: ScanOptions, each: &mut impl FnMut(Visit)) -> Result<(), Error> {
    let mut archive = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
    archive.for_each_entries(|entry, reader| {
        if entry.is_directory() {
            // nothing to hash
        } else if !wanted(entry.name(), opts) {
            each(Visit::Skipped {
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
            });
        } else {
            let hashes = hash_stream(reader, entry.size()).map_err(|e| e.to_string());
            each(Visit::Hashed(Entry {
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
                hashes,
            }));
        }
        Ok(true)
    })?;
    Ok(())
}
