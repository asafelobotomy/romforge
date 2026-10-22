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

use crate::hash::Hashes;
use crate::{Error, systems};
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
    /// A folder or archive that is a game by ScummVM's list (`games`): its
    /// `files` aren't read or hashed.
    InGame {
        path: PathBuf,
        files: usize,
    },
}

fn ext_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether a (lower-case) extension is an archive's whose members are read.
pub(crate) fn is_archive(ext: &str) -> bool {
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
/// Symlinked files are hashed like normal files. `games`: folders and
/// archives already known to be ScummVM games (with their file counts),
/// passed over whole ([`Visit::InGame`]).
pub fn walk(
    root: &Path,
    skip: &[PathBuf],
    games: &[(PathBuf, usize)],
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
    let mut walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .follow_root_links(true)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !crate::layout::is_own_dir(e.file_name()));
    let game = |p: &Path| games.iter().find(|(g, _)| g == p).map(|(_, n)| *n);
    while let Some(item) = walker.next() {
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
        if let Some(files) = game(&path) {
            if item.file_type().is_dir() {
                walker.skip_current_dir();
            }
            each(Visit::InGame { path, files });
            continue;
        }
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

mod members;
use members::hash_unit;
