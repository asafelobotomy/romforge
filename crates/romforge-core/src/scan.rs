//! Walks a folder and hashes likely ROMs (by extension, see `systems.rs`),
//! including zip/7z members and CHD tracks (see `disc.rs`), as streams.
//! Nothing is extracted to disk and no external tool is run.
//!
//! A file whose size and modification time match the last scan isn't read
//! again ([`Visit::Unchanged`]; `rehash` turns this off).
//!
//! Files and archives are hashed on several threads, largest first; members
//! of one archive are read in order on one thread (a solid 7z is a single
//! stream). Results reach the caller on the calling thread.
//!
//! [`Visit::Planned`] says how many files and archives will be read (and
//! their size on disk) before any is; [`Visit::UnitDone`] follows each.
//! [`ScanOptions::cancel`] stops it between them.

use crate::hash::{Hashes, hash_stream};
use crate::{Error, systems};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// What to hash, and how.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// Hash every file and member, not only those with a known ROM extension.
    pub all_files: bool,
    /// Hashing threads; 0 picks one per CPU, at most 8 (in a
    /// [`crate::Library`] scan, 0 means the Settings `threads`).
    pub jobs: usize,
    /// Hash every file again, even ones unchanged since the last scan.
    pub rehash: bool,
    /// Asked to stop: no file or archive is started after it is set.
    pub cancel: crate::job::Cancel,
}

/// A file to hash, or an archive whose members to hash.
enum Unit {
    File(PathBuf),
    /// An archive; `true`: every member is hashed (an arcade set's zip,
    /// whose ROMs have no telling extensions).
    Zip(PathBuf, bool),
    SevenZ(PathBuf, bool),
    /// A CHD disc image; its tracks are the members.
    Chd(PathBuf),
}

#[derive(Debug)]
pub struct Entry {
    /// The file on disk (the archive, for members).
    pub path: PathBuf,
    /// Path inside the archive, or empty for plain files.
    pub member: String,
    pub hashes: Result<Hashes, String>,
    /// Size and modification time of the file on disk when it was read.
    pub stamp: Option<Stamp>,
}

/// A file on disk as it was when hashed: unchanged size and modification
/// time mean its hashes can be kept (incremental rescans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stamp {
    pub size: u64,
    /// Nanoseconds since the Unix epoch.
    pub mtime: i64,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        let mtime = m
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        Some(Stamp { size: m.len(), mtime })
    }
}

// Nearly every visit is `Hashed`, handed straight to a callback: boxing it
// would only add an allocation per file.
#[allow(clippy::large_enum_variant)]
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
    /// A file (or archive) whose size and modification time match what was
    /// hashed before; not read again.
    Unchanged(PathBuf),
    /// Before any file is read: how many files and archives will be, and
    /// their size on disk.
    Planned {
        units: usize,
        bytes: u64,
    },
    /// One of those is done (its size on disk).
    UnitDone {
        bytes: u64,
    },
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
fn wanted(name: &str, opts: &ScanOptions) -> bool {
    // A game's details as import writes them (`.md` is a Mega Drive
    // extension too).
    // Patches are listed apart, to pair with their games.
    !name.ends_with(crate::layout::DETAILS_SUFFIX)
        && !crate::patch::is_patch_name(name)
        && (opts.all_files || systems::is_rom_extension(&ext_of(name)))
}

/// An archive member worth hashing: not one of the files cleanup and
/// import put in an archive's `romforge/` folder (art, and details as
/// `.romforge.md`, which `.md` would otherwise take for a Mega Drive ROM).
fn wanted_member(name: &str, opts: &ScanOptions) -> bool {
    !crate::layout::is_extra(name) && wanted(name, opts)
}

/// Visit everything under `root`: skipped files and links in path order
/// while walking, then hashed files as threads finish them. `root` itself may be a
/// symlink; symlinked folders below it are reported and not followed, so a
/// link loop or a link to another drive cannot make a scan run away.
/// Symlinked files are hashed like normal files.
pub fn walk(
    root: &Path,
    skip: &[PathBuf],
    opts: &ScanOptions,
    // Arcade set names (lower case): an archive named after one has every
    // member hashed.
    sets: &std::collections::HashSet<String>,
    unchanged: impl Fn(&Path, Stamp) -> bool,
    mut each: impl FnMut(Visit),
) -> Result<(), Error> {
    let mut units: Vec<(u64, Option<Stamp>, Unit)> = Vec::new();
    // Cleanup's trash folders hold originals waiting for undo: not part of
    // the library.
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .follow_root_links(true)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !crate::layout::is_own_dir(e.file_name()));
    for item in walker {
        opts.cancel.check()?;
        let item = match item {
            Ok(item) => item,
            Err(e) => {
                each(Visit::Hashed(Entry {
                    stamp: None,
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
        let stamp = Stamp::of(&path);
        if let Some(s) = stamp
            && unchanged(&path, s)
        {
            each(Visit::Unchanged(path));
            continue;
        }
        units.push((
            stamp.map_or(0, |s| s.size),
            stamp,
            match ext.as_str() {
                "zip" | "7z" => {
                    let whole = path
                        .file_stem()
                        .is_some_and(|s| sets.contains(&s.to_string_lossy().to_lowercase()));
                    if ext == "zip" {
                        Unit::Zip(path, whole)
                    } else {
                        Unit::SevenZ(path, whole)
                    }
                }
                "chd" => Unit::Chd(path),
                _ => Unit::File(path),
            },
        ));
    }

    // Largest first, so one big archive doesn't start last and run alone.
    units.sort_by_key(|u| std::cmp::Reverse(u.0));
    each(Visit::Planned {
        units: units.len(),
        bytes: units.iter().map(|u| u.0).sum(),
    });
    crate::parallel::stream(
        units,
        crate::parallel::threads(opts.jobs),
        |(size, stamp, unit), send| {
            if opts.cancel.is_set() {
                return;
            }
            hash_unit(unit, stamp, opts, &mut *send);
            send(Visit::UnitDone { bytes: size });
        },
        &mut each,
    );
    opts.cancel.check()
}

fn hash_unit(unit: Unit, stamp: Option<Stamp>, opts: &ScanOptions, each: &mut dyn FnMut(Visit)) {
    // Every entry of the unit carries the file's stamp.
    let each = &mut |v: Visit| match v {
        Visit::Hashed(mut e) => {
            e.stamp = stamp;
            each(Visit::Hashed(e))
        }
        v => each(v),
    };
    let (path, result) = match unit {
        Unit::Zip(path, whole) => {
            let r = walk_zip(&path, &member_opts(opts, whole), each);
            (path, r)
        }
        Unit::SevenZ(path, whole) => {
            let r = walk_7z(&path, &member_opts(opts, whole), each);
            (path, r)
        }
        Unit::Chd(path) => {
            let r = crate::disc::chd_members(&path).map(|members| {
                for (member, hashes) in members {
                    each(Visit::Hashed(Entry {
                        stamp: None,
                        path: path.clone(),
                        member,
                        hashes: Ok(hashes),
                    }));
                }
            });
            (path, r)
        }
        Unit::File(path) => {
            let hashes = hash_file(&path).map_err(|e| e.to_string());
            each(Visit::Hashed(Entry {
                stamp: None,
                path,
                member: String::new(),
                hashes,
            }));
            return;
        }
    };
    if let Err(e) = result {
        each(Visit::Hashed(Entry {
            stamp: None,
            path,
            member: String::new(),
            hashes: Err(e.to_string()),
        }));
    }
}

/// The options for an archive's members: all of them for an arcade set.
fn member_opts(opts: &ScanOptions, whole: bool) -> ScanOptions {
    ScanOptions {
        all_files: opts.all_files || whole,
        ..opts.clone()
    }
}

fn hash_file(path: &Path) -> std::io::Result<Hashes> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hashes = hash_stream(BufReader::new(file), size)?;
    // A compressed disc image: its own header says what it holds.
    if hashes.id.is_none() {
        hashes.id = crate::ident::read_container(&mut BufReader::new(File::open(path)?));
    }
    Ok(hashes)
}

fn walk_zip(path: &Path, opts: &ScanOptions, each: &mut dyn FnMut(Visit)) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    for i in 0..archive.len() {
        // One member that can't be opened (encrypted, an unsupported
        // method) is that member's error; the rest are still read.
        let name = match archive.name_for_index(i) {
            Some(Ok(n)) => n.into_owned(),
            _ => format!("#{i}"),
        };
        let mut member = match archive.by_index(i) {
            Ok(m) => m,
            Err(e) => {
                let path = path.to_path_buf();
                each(match wanted_member(&name, opts) {
                    true => Visit::Hashed(Entry {
                        stamp: None,
                        path,
                        member: name,
                        hashes: Err(e.to_string()),
                    }),
                    false => Visit::Skipped { path, member: name },
                });
                continue;
            }
        };
        if member.is_dir() {
            continue;
        }
        let name = member.name().map_or(name, |n| n.into_owned());
        if !wanted_member(&name, opts) {
            each(Visit::Skipped {
                path: path.to_path_buf(),
                member: name,
            });
            continue;
        }
        let size = member.size();
        let hashes = hash_stream(&mut member, size).map_err(|e| e.to_string());
        each(Visit::Hashed(Entry {
            stamp: None,
            path: path.to_path_buf(),
            member: name,
            hashes,
        }));
    }
    Ok(())
}

fn walk_7z(path: &Path, opts: &ScanOptions, each: &mut dyn FnMut(Visit)) -> Result<(), Error> {
    let mut archive = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
    archive.for_each_entries(|entry, reader| {
        if entry.is_directory() {
            // nothing to hash
        } else if !wanted_member(entry.name(), opts) {
            // A solid archive is one stream: a member not read to its end
            // would leave the next one starting at the wrong bytes.
            std::io::copy(reader, &mut std::io::sink())?;
            each(Visit::Skipped {
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
            });
        } else {
            let hashes = hash_stream(&mut *reader, entry.size()).map_err(|e| e.to_string());
            if hashes.is_err() {
                let _ = std::io::copy(reader, &mut std::io::sink());
            }
            each(Visit::Hashed(Entry {
                stamp: None,
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
                hashes,
            }));
        }
        Ok(true)
    })?;
    Ok(())
}
