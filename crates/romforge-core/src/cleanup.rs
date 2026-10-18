//! Milestone 10: cleanup & rename. Repacks each game into one file named by
//! its DAT name, with its art and details inside, under the guardrails of
//! house rule 7.
//!
//! - Cartridge games: `<dest>/<system>/<DAT name>.zip` holding the ROMs
//!   (DAT ROM names; your own file names for games you named) and a
//!   `romforge/` folder with the art and `<name>.romforge.md`.
//! - Disc games: `<dest>/<system>/<DAT name>.chd` (CD tracks, or a DVD
//!   image), with art and details in a sidecar folder of the same name (CHD
//!   can't hold extra files).
//!
//! `<dest>` is the Library folder, else the Source folder. ROM data is
//! streamed from the old file or archive straight into the new one; only
//! tracks inside a 7z (one compressed stream) are unpacked, into the staging
//! folder. Every new file is read back and its ROMs/tracks re-hashed against
//! the originals before anything else happens. Without "keep originals" the
//! originals then move to `.romforge-trash/<batch>/` next to them, and only
//! when every file in them went into this batch. Every step is journaled so
//! [`undo`] removes the new files and puts the originals back. Nothing is
//! ever overwritten.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::hash::hash_stream;
use crate::home::Home;
use crate::organize::{move_file, sanitize};
use crate::view::{self, RowStatus};
use crate::{Error, settings, systems};

/// Folder (next to the originals) where cleanup keeps them until undo.
pub const TRASH_DIR: &str = ".romforge-trash";

/// Zip members up to this size are read into memory; bigger ones are
/// unpacked to staging.
const IN_MEMORY: u64 = 64 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Zip,
    /// CD tracks (`.bin`) into a CD CHD.
    ChdCd,
    /// A DVD image (`.iso`) into a DVD CHD.
    ChdDvd,
    /// A game that is a folder (or an archive of one), every file of it
    /// into one zip (DOS).
    Folder,
    /// A game that is a folder, copied as a folder (ScummVM).
    Dir,
}

/// One ROM of a game and where its data is now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Part {
    /// Name in the new archive.
    pub name: String,
    pub path: PathBuf,
    /// Member of `path`, or empty for a plain file.
    pub member: String,
    pub size: u64,
    /// The data's SHA1 as it is now; the new file must hold the same bytes.
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Item {
    pub row_id: String,
    pub title: String,
    pub system: String,
    pub format: Format,
    pub target: PathBuf,
    /// Folder for art and details (discs).
    pub sidecar: Option<PathBuf>,
    pub parts: Vec<Part>,
    /// Files on disk that go to the trash afterwards (without "keep
    /// originals"): every file in them is part of this batch.
    pub trash: Vec<PathBuf>,
    /// Files that stay because they hold more than this batch.
    pub kept: Vec<PathBuf>,
    /// Other copies of the same ROMs in the Source: redundant once the game
    /// is written, so trashed along with the originals.
    pub copies: Vec<Part>,
    /// A cue sheet that isn't the DAT's, beside the tracks (same archive or
    /// folder). Its pregaps go into the CHD, so it counts as taken; if it
    /// turns out not to name exactly the tracks, its file stays.
    pub sheet: Option<Part>,
    /// The game is in the RomForge folder already (at `target`): nothing
    /// is written, its `copies` outside only go to the trash.
    #[serde(default)]
    pub existing: bool,
    /// For a game that is a folder: the folder, or the archive holding it.
    /// Everything in it goes, and it goes to the trash whole.
    #[serde(default)]
    pub folder: Option<PathBuf>,
    /// For a ScummVM game: its game id, written as `<id>.scummvm` in the
    /// folder for RetroArch's core.
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupPlan {
    pub dest: PathBuf,
    pub keep_originals: bool,
    pub items: Vec<Item>,
    /// (row title, why it isn't cleaned)
    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupReport {
    pub batch_id: i64,
    pub done: usize,
    pub trashed: usize,
    /// `.m3u` playlists written for complete multi-disk sets.
    #[serde(default)]
    pub playlists: usize,
    pub failed: Vec<(String, String)>,
}

/// Which files a cleanup or import takes games from.
enum Source {
    /// The Source folder (no RomForge folder), or one folder being
    /// imported.
    Folder(PathBuf),
    /// Anywhere outside the RomForge folder.
    Outside(PathBuf),
}

impl Source {
    fn contains(&self, p: &Path) -> bool {
        match self {
            Source::Folder(dir) => p.starts_with(dir),
            Source::Outside(home) => !p.starts_with(home),
        }
    }
}

/// Work out what cleanup would do (nothing is touched). `only`: limit to
/// these library row ids. With a RomForge folder this is the import plan:
/// games outside it (or in `from` only) go to its `Games/`.
pub fn plan(
    conn: &Connection,
    only: Option<&[String]>,
    home: Option<&Home>,
    from: Option<&Path>,
) -> Result<CleanupPlan, Error> {
    let s = settings::load(conn)?;
    let (source, dest) = match home {
        Some(h) => {
            let source = match from {
                Some(f) if h.contains(f) => {
                    return Err(Error::Settings(format!(
                        "{} is inside the RomForge folder; nothing to import from it",
                        f.display()
                    )));
                }
                Some(f) => Source::Folder(f.to_owned()),
                None => Source::Outside(h.root().to_owned()),
            };
            (source, h.games())
        }
        None => {
            let source = s
                .source_dir
                .clone()
                .ok_or_else(|| Error::Settings("set a Source folder first (Settings)".into()))?;
            let dest = s.library_dir.clone().unwrap_or_else(|| source.clone());
            (Source::Folder(from.map_or(source, Path::to_owned)), dest)
        }
    };
    let mut plan = CleanupPlan {
        dest: dest.clone(),
        keep_originals: s.keep_originals,
        ..CleanupPlan::default()
    };
    let in_source = |p: &Path| source.contains(p);
    let games = home.map(Home::games);
    let mut path_of = conn.prepare("SELECT path FROM files WHERE file_id = ?1")?;

    let v = view::library(conn)?;
    // Which library rows each file belongs to: an original another game
    // still needs is never trashed (a disc track identical in two
    // regions belongs to both).
    let mut rows_of: HashMap<i64, Vec<(String, String, RowStatus)>> = HashMap::new();
    for row in &v.rows {
        for f in row.file_ids() {
            rows_of
                .entry(*f)
                .or_default()
                .push((row.id.clone(), row.title.clone(), row.status));
        }
    }
    let mut taken: BTreeSet<PathBuf> = BTreeSet::new();
    let mut items = Vec::new();
    for row in v.rows {
        if only.is_some_and(|ids| !ids.contains(&row.id)) {
            continue;
        }
        if !row.status.identified() {
            if only.is_some() {
                plan.skipped
                    .push((row.title.clone(), "identify it first (Search)".into()));
            }
            continue;
        }
        let Some(system) = row.system.as_deref().and_then(systems::by_key) else {
            continue;
        };
        if let Some(place) = &row.folder {
            match folder_item(
                conn,
                &row,
                system,
                Path::new(place),
                &dest,
                &source,
                games.as_deref(),
            )? {
                Ok(None) => {}
                Ok(Some(mut it)) => {
                    if it.target.exists() {
                        plan.skipped.push((
                            row.title.clone(),
                            format!("{} already exists", it.target.display()),
                        ));
                    } else if !taken.insert(it.target.clone()) {
                        plan.skipped
                            .push((row.title.clone(), "another game has the same name".into()));
                    } else {
                        if !s.keep_originals {
                            it.trash.push(it.folder.clone().unwrap_or_default());
                        }
                        items.push(it);
                    }
                }
                Err(why) => plan.skipped.push((row.title.clone(), why)),
            }
            continue;
        }
        let parts = match row.status {
            RowStatus::Verified => verified_parts(conn, system.id, &row.title, &source)?,
            _ => named_parts(conn, &row.id, &source)?,
        };
        let Some((parts, copies)) = parts else {
            plan.skipped
                .push((row.title.clone(), "no complete set of files to take".into()));
            continue;
        };
        // Already in Games/: the copies outside are redundant.
        if let Some(games) = &games {
            let mut there = None;
            for f in row.file_ids() {
                let p: String = path_of.query_row([f], |r| r.get(0))?;
                if Path::new(&p).starts_with(games) {
                    there = Some(PathBuf::from(p));
                    break;
                }
            }
            if let Some(target) = there {
                let outside: Vec<Part> = parts
                    .into_iter()
                    .chain(copies)
                    .filter(|p| in_source(&p.path))
                    .collect();
                if !outside.is_empty() && !s.keep_originals {
                    items.push(Item {
                        row_id: row.id.clone(),
                        title: row.title.clone(),
                        system: system.key.to_owned(),
                        format: Format::Zip,
                        target,
                        sidecar: None,
                        parts: Vec::new(),
                        trash: Vec::new(),
                        kept: Vec::new(),
                        copies: outside,
                        sheet: None,
                        existing: true,
                        folder: None,
                        code: None,
                    });
                }
                continue;
            }
        }
        if !parts.iter().any(|p| in_source(&p.path)) {
            continue; // already processed (in the Library) or outside Source
        }
        let disc = system.is_disc_based;
        let tracks: Vec<&Part> = parts
            .iter()
            .filter(|p| !p.name.to_lowercase().ends_with(".cue"))
            .collect();
        let format = if !disc {
            Format::Zip
        } else if tracks.len() == 1 && tracks[0].name.to_lowercase().ends_with(".iso") {
            Format::ChdDvd
        } else if !tracks.is_empty() && tracks.iter().all(|p| p.name.to_lowercase().ends_with(".bin")) {
            Format::ChdCd
        } else {
            plan.skipped.push((
                row.title.clone(),
                "disc files CHD can't take (only .bin tracks or one .iso)".into(),
            ));
            continue;
        };
        // ROM names come from the DAT, which is untrusted: none may climb out
        // of the archive or shadow its `romforge/` folder.
        if let Some(bad) = parts.iter().find(|p| !safe_member_name(&p.name)) {
            plan.skipped.push((
                row.title.clone(),
                format!(
                    "the DAT names a file {:?}, which can't go in an archive safely",
                    bad.name
                ),
            ));
            continue;
        }
        let name = sanitize(&row.title);
        let dir = dest.join(sanitize(system.key));
        let (target, sidecar) = match format {
            Format::Zip => (dir.join(format!("{name}.zip")), None),
            _ => (dir.join(format!("{name}.chd")), Some(dir.join(&name))),
        };
        if target.exists() || sidecar.as_ref().is_some_and(|d| d.exists()) {
            plan.skipped
                .push((row.title.clone(), format!("{} already exists", target.display())));
            continue;
        }
        if !taken.insert(target.clone()) {
            plan.skipped
                .push((row.title.clone(), "another game has the same name".into()));
            continue;
        }
        let sheet = if format == Format::ChdCd && tracks.len() == parts.len() {
            loose_cue(conn, &parts)?
        } else {
            None
        };
        if format == Format::ChdCd && tracks.len() > 1 && tracks.len() == parts.len() && sheet.is_none() {
            plan.skipped.push((
                row.title.clone(),
                "a disc of several tracks needs its cue sheet (for the pregaps); none is beside it".into(),
            ));
            continue;
        }
        items.push(Item {
            row_id: row.id.clone(),
            title: row.title.clone(),
            system: system.key.to_owned(),
            format,
            target,
            sidecar,
            parts,
            trash: Vec::new(),
            kept: Vec::new(),
            copies,
            sheet,
            existing: false,
            folder: None,
            code: None,
        });
    }

    // Originals: trashed only when everything in them is taken by the batch
    // (an archive's own member list decides, not just what was scanned).
    let mut used: HashMap<PathBuf, BTreeSet<String>> = HashMap::new();
    for it in items.iter().filter(|it| it.folder.is_none()) {
        for p in it.parts.iter().chain(&it.copies).chain(&it.sheet) {
            used.entry(p.path.clone()).or_default().insert(p.member.clone());
        }
    }
    let batch: BTreeSet<String> = items.iter().map(|it| it.row_id.clone()).collect();
    let mut files_at = conn.prepare("SELECT file_id FROM files WHERE path = ?1")?;
    for it in items.iter_mut().filter(|it| it.folder.is_none()) {
        let paths: BTreeSet<&PathBuf> = it
            .parts
            .iter()
            .chain(&it.copies)
            .chain(&it.sheet)
            .map(|p| &p.path)
            .collect();
        for p in paths {
            if !in_source(p) {
                continue;
            }
            let taken = &used[p];
            let all = if taken.contains("") {
                true // a plain file
            } else {
                archive_names(p).is_ok_and(|names| names.iter().all(|n| taken.contains(n)))
            };
            // Any of its files in a game outside the batch (a mere
            // duplicate copy doesn't count) keeps it.
            let needed_elsewhere = files_at
                .query_map([p.to_string_lossy()], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<i64>, _>>()?
                .iter()
                .flat_map(|f| rows_of.get(f).into_iter().flatten())
                .any(|(id, _, status)| !batch.contains(id) && *status != RowStatus::Duplicate);
            if all && !needed_elsewhere {
                it.trash.push(p.clone());
            } else {
                it.kept.push(p.clone());
            }
        }
    }
    // An archive shared by two games in the batch is trashed once, by the
    // last of them (after both are written).
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    for it in items.iter_mut().rev() {
        it.trash.retain(|p| seen.insert(p.clone()));
    }
    plan.items = items;
    Ok(plan)
}

/// Whether a DAT's ROM name is safe as a path inside an archive: relative,
/// no `.` or `..` parts, no backslashes or drive letters, and not in the
/// `romforge/` folder kept for art and details.
fn safe_member_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    !name.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != "..")
        && !parts[0].eq_ignore_ascii_case("romforge")
}

/// Every file in a zip or 7z archive.
fn archive_names(path: &Path) -> Result<Vec<String>, Error> {
    Ok(archive_entries(path)?.into_iter().map(|(n, _)| n).collect())
}

/// Every file in a zip or 7z archive, with its size.
fn archive_entries(path: &Path) -> Result<Vec<(String, u64)>, Error> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("zip") => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
            let mut out = Vec::new();
            for i in 0..zip.len() {
                let m = zip.by_index_raw(i)?;
                if !m.is_dir() {
                    out.push((m.name()?.into_owned(), m.size()));
                }
            }
            Ok(out)
        }
        Some("7z") => {
            let a = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
            Ok(a.archive()
                .files
                .iter()
                .filter(|e| !e.is_directory())
                .map(|e| (e.name().to_owned(), e.size()))
                .collect())
        }
        _ => Err(Error::Cleanup(format!(
            "{} isn't an archive romforge reads",
            path.display()
        ))),
    }
}

/// The import of a game that is a folder: every file in it (or in the
/// archive holding it) under its own path. `Ok(Err(why))` when it can't
/// go; `Ok(Ok(None))` when it is in RomForge already or outside what is
/// imported.
fn folder_item(
    conn: &Connection,
    row: &view::GameRow,
    system: &systems::System,
    place: &Path,
    dest: &Path,
    source: &Source,
    games: Option<&Path>,
) -> Result<Result<Option<Item>, String>, Error> {
    if games.is_some_and(|g| place.starts_with(g)) || !source.contains(place) {
        return Ok(Ok(None));
    }
    let mut parts = Vec::new();
    if place.is_dir() {
        for e in walkdir::WalkDir::new(place)
            .follow_links(false)
            .sort_by_file_name()
        {
            let e = e.map_err(std::io::Error::from)?;
            if !e.file_type().is_file() {
                continue;
            }
            let rel = e.path().strip_prefix(place).unwrap_or(e.path());
            parts.push(Part {
                name: rel.to_string_lossy().replace('\\', "/"),
                path: e.path().to_owned(),
                member: String::new(),
                size: e.metadata().map_err(std::io::Error::from)?.len(),
                sha1: String::new(),
            });
        }
    } else {
        for (name, size) in archive_entries(place)? {
            parts.push(Part {
                name: name.clone(),
                path: place.to_owned(),
                member: name,
                size,
                sha1: String::new(),
            });
        }
    }
    if parts.is_empty() {
        return Ok(Err("the folder is empty".into()));
    }
    if let Some(bad) = parts.iter().find(|p| !safe_member_name(&p.name)) {
        return Ok(Err(format!(
            "it holds {:?}, which can't be imported safely",
            bad.name
        )));
    }
    let name = sanitize(&row.title);
    let dir = dest.join(sanitize(system.key));
    let (format, target) = match system.folders {
        systems::Folders::Kept if place.is_dir() => (Format::Dir, dir.join(&name)),
        systems::Folders::Kept => {
            return Ok(Err("ScummVM games are imported as folders, not archives".into()));
        }
        _ => (Format::Folder, dir.join(format!("{name}.zip"))),
    };
    let code = match row.id.strip_prefix("d:").and_then(|id| id.parse::<i64>().ok()) {
        Some(id) => Some(conn.query_row(
            "SELECT code FROM folder_games WHERE folder_id = ?1",
            [id],
            |r| r.get(0),
        )?),
        None => None,
    };
    Ok(Ok(Some(Item {
        row_id: row.id.clone(),
        title: row.title.clone(),
        system: system.key.to_owned(),
        format,
        target,
        sidecar: None,
        parts,
        trash: Vec::new(),
        kept: Vec::new(),
        copies: Vec::new(),
        sheet: None,
        existing: false,
        folder: Some(place.to_owned()),
        code,
    })))
}

/// A reader that hashes what passes through it.
struct Sha1Read<R> {
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA1 of a whole stream.
fn sha1_of(r: impl Read) -> std::io::Result<String> {
    use sha1::Digest as _;
    let mut h = Sha1Read {
        inner: r,
        sha1: sha1::Sha1::new(),
    };
    std::io::copy(&mut h, &mut std::io::sink())?;
    Ok(hex(&h.sha1.finalize()))
}

/// Copy `r` into `w`, returning the SHA1 of what was copied.
fn copy_hashed(r: impl Read, w: &mut impl Write) -> std::io::Result<String> {
    use sha1::Digest as _;
    let mut h = Sha1Read {
        inner: r,
        sha1: sha1::Sha1::new(),
    };
    std::io::copy(&mut h, w)?;
    Ok(hex(&h.sha1.finalize()))
}

/// A folder game (or the archive holding it) into one zip, every file
/// under its own path, then read back: each must hash as it was read.
fn write_folder_zip(item: &Item, extras: &[(String, Vec<u8>)], out: &Path) -> Result<(), Error> {
    let src = item.folder.as_deref().unwrap_or(Path::new(""));
    let mut zip = zip::ZipWriter::new(File::create_new(out)?);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let mut sums: Vec<(String, String)> = Vec::new();
    let ext = src.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    if src.is_dir() {
        for p in &item.parts {
            zip.start_file(&p.name, opts)?;
            sums.push((
                p.name.clone(),
                copy_hashed(BufReader::new(File::open(&p.path)?), &mut zip)?,
            ));
        }
    } else if ext.as_deref() == Some("zip") {
        let mut a = zip::ZipArchive::new(BufReader::new(File::open(src)?))?;
        for i in 0..a.len() {
            let mut m = a.by_index(i)?;
            if m.is_dir() {
                continue;
            }
            let name = m.name()?.into_owned();
            zip.start_file(&name, opts)?;
            sums.push((name, copy_hashed(&mut m, &mut zip)?));
        }
    } else {
        // A 7z is one stream: its members are copied in order, in one pass.
        let mut a = sevenz_rust2::ArchiveReader::open(src, sevenz_rust2::Password::empty())?;
        let mut failed: Option<Error> = None;
        a.for_each_entries(|entry, reader| {
            if entry.is_directory() {
                return Ok(true);
            }
            let r = (|| -> Result<String, Error> {
                zip.start_file(entry.name(), opts)?;
                Ok(copy_hashed(&mut *reader, &mut zip)?)
            })();
            match r {
                Ok(sum) => {
                    sums.push((entry.name().to_owned(), sum));
                    Ok(true)
                }
                Err(e) => {
                    failed = Some(e);
                    Ok(false)
                }
            }
        })?;
        if let Some(e) = failed {
            return Err(e);
        }
    }
    for (name, data) in extras {
        zip.start_file(format!("romforge/{name}"), opts)?;
        zip.write_all(data)?;
    }
    zip.finish()?.sync_all()?;
    if sums.len() != item.parts.len() {
        return Err(Error::Cleanup(format!(
            "{} changed since the plan ({} files, {} now)",
            src.display(),
            item.parts.len(),
            sums.len()
        )));
    }
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
fn copy_folder(item: &Item, extras: &[(String, Vec<u8>)], out: &Path) -> Result<(), Error> {
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
    if !extras.is_empty() && !out.join("romforge").exists() {
        std::fs::create_dir(out.join("romforge"))?;
        for (name, data) in extras {
            std::fs::write(out.join("romforge").join(name), data)?;
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
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
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

/// (the ROMs to write, other copies of them in Source)
type PartsAndCopies = (Vec<Part>, Vec<Part>);

/// The ROMs of a hash-verified game: one file per ROM (the copy in Source
/// first, a matched file before a duplicate), from the DAT that has them
/// all, and the other copies of them in Source. Cue sheets are taken when
/// matched; a game is complete without one (a CHD needs none).
fn verified_parts(
    conn: &Connection,
    system_id: i64,
    title: &str,
    source: &Source,
) -> Result<Option<PartsAndCopies>, Error> {
    let mut stmt = conn.prepare(
        "SELECT g.game_id, r.rom_id, r.name, f.path, f.member, f.size, f.sha1, f.status
         FROM games g JOIN dats d ON d.dat_id = g.dat_id
         JOIN roms r ON r.game_id = g.game_id
         LEFT JOIN matches m ON m.rom_id = r.rom_id AND m.out_of_scope = 0 AND m.via IN ('sha1', 'md5')
         LEFT JOIN files f ON f.file_id = m.file_id AND f.status IN ('matched', 'duplicate')
         WHERE d.system_id = ?1 AND g.name = ?2 AND coalesce(r.status, '') <> 'nodump'
         ORDER BY g.game_id, r.rom_id, f.path, f.member",
    )?;
    // game -> rom -> (DAT name, every copy with its rank: lower is better)
    type Copies = (String, Vec<(Part, u8)>);
    let mut games: BTreeMap<i64, BTreeMap<i64, Copies>> = BTreeMap::new();
    for r in stmt.query_map(params![system_id, title], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<u64>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
        ))
    })? {
        let (game, rom, name, path, member, size, sha1, status) = r?;
        let entry = games
            .entry(game)
            .or_default()
            .entry(rom)
            .or_insert((name.clone(), Vec::new()));
        if let (Some(path), Some(member), Some(size), Some(sha1), Some(status)) =
            (path, member, size, sha1, status)
        {
            let path = PathBuf::from(path);
            let rank = (!source.contains(&path)) as u8 * 2 + (status != "matched") as u8;
            let part = Part {
                name,
                path,
                member,
                size,
                sha1,
            };
            entry.1.push((part, rank));
        }
    }
    for roms in games.values() {
        let complete = roms
            .values()
            .filter(|(n, _)| !n.to_lowercase().ends_with(".cue"))
            .all(|(_, c)| !c.is_empty());
        if !complete || roms.values().all(|(_, c)| c.is_empty()) {
            continue;
        }
        let (mut parts, mut copies) = (Vec::new(), Vec::new());
        for (_, c) in roms.values() {
            let Some(best) = c
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, rank))| *rank)
                .map(|(i, _)| i)
            else {
                continue;
            };
            for (i, (p, _)) in c.iter().enumerate() {
                if i == best {
                    parts.push(p.clone());
                } else if source.contains(&p.path) {
                    copies.push(p.clone());
                }
            }
        }
        return Ok(Some((parts, copies)));
    }
    Ok(None)
}

/// The files of a game you named, under their own names.
/// The files of a game you named: one per content (other copies of it in
/// Source are `copies`, the one in Source first), tracks in natural order
/// ("Track 2" before "Track 10") as there is no DAT to give it.
fn named_parts(conn: &Connection, row_id: &str, source: &Source) -> Result<Option<PartsAndCopies>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut all: Vec<Part> = d
        .files
        .iter()
        .map(|f| {
            let (path, member) = match f.path.split_once('#') {
                Some((p, m)) => (PathBuf::from(p), m.to_owned()),
                None => (PathBuf::from(&f.path), String::new()),
            };
            let shown = if member.is_empty() {
                path.to_string_lossy().into_owned()
            } else {
                member.clone()
            };
            Part {
                name: shown.rsplit('/').next().unwrap_or(&shown).to_owned(),
                path,
                member,
                size: f.size,
                sha1: f.sha1.clone(),
            }
        })
        .collect();
    all.sort_by_key(|p| (!source.contains(&p.path), natural_key(&p.name)));
    let (mut parts, mut copies) = (Vec::new(), Vec::new());
    let mut seen = BTreeSet::new();
    for p in all {
        if seen.insert((p.sha1.clone(), p.size)) {
            parts.push(p);
        } else if source.contains(&p.path) {
            copies.push(p);
        }
    }
    parts.sort_by_key(|p| natural_key(&p.name));
    Ok((!parts.is_empty()).then_some((parts, copies)))
}

/// A name for sorting with its numbers as numbers.
fn natural_key(name: &str) -> Vec<(String, u64)> {
    let lower = name.to_lowercase();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut chars = lower.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut n = c.to_digit(10).unwrap_or(0) as u64;
            while let Some(d) = chars.peek().and_then(|d| d.to_digit(10)) {
                n = n.saturating_mul(10).saturating_add(d as u64);
                chars.next();
            }
            out.push((std::mem::take(&mut text), n));
        } else {
            text.push(c);
        }
    }
    out.push((text, 0));
    out
}

/// Open a part's data as a stream: a plain file or a zip member directly;
/// a 7z member is unpacked into `staging` first (one compressed stream).
fn open_part(part: &Part, staging: &Path) -> Result<Box<dyn Read>, Error> {
    if part.member.is_empty() {
        return Ok(Box::new(BufReader::new(File::open(&part.path)?)));
    }
    let ext = part
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("zip") => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(&part.path)?))?;
            let mut m = zip.by_name(&part.member)?;
            if m.size() <= IN_MEMORY {
                let mut buf = Vec::new();
                m.read_to_end(&mut buf)?;
                return Ok(Box::new(std::io::Cursor::new(buf)));
            }
            // A big member (a disc track) is unpacked to staging instead.
            std::fs::create_dir_all(staging)?;
            let out = staging.join(format!("{}.part", part.sha1));
            std::io::copy(&mut m, &mut File::create(&out)?)?;
            Ok(Box::new(BufReader::new(File::open(out)?)))
        }
        Some("7z") => {
            let out = staging.join(format!("{}.part", part.sha1));
            if !out.exists() {
                std::fs::create_dir_all(staging)?;
                let mut archive =
                    sevenz_rust2::ArchiveReader::open(&part.path, sevenz_rust2::Password::empty())?;
                let mut found = false;
                archive.for_each_entries(|entry, reader| {
                    if entry.name() == part.member {
                        let mut f = File::create(&out)?;
                        std::io::copy(reader, &mut f)?;
                        found = true;
                        return Ok(false);
                    }
                    Ok(true)
                })?;
                if !found {
                    return Err(Error::Cleanup(format!(
                        "{} has no {}",
                        part.path.display(),
                        part.member
                    )));
                }
            }
            Ok(Box::new(BufReader::new(File::open(out)?)))
        }
        _ => Err(Error::Cleanup(format!(
            "can't read {}#{}",
            part.path.display(),
            part.member
        ))),
    }
}

/// What goes in the `romforge/` folder (or sidecar): (file name, bytes).
fn extras(conn: &Connection, row_id: &str) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut out = vec![(
        format!("{}.romforge.md", sanitize(&d.row.title)),
        details_md(&d).into_bytes(),
    )];
    for (kind, path) in &d.art {
        if let Ok(data) = std::fs::read(path) {
            let ext = crate::enrich::image_ext(&data).unwrap_or("png");
            out.push((format!("{kind}.{ext}"), data));
        }
    }
    Ok(out)
}

/// The game's details as Markdown.
pub fn details_md(d: &view::GameDetail) -> String {
    let mut s = format!("# {}\n\n", d.row.title);
    let derived;
    let status = match d.row.status {
        RowStatus::Verified => "verified by hash",
        RowStatus::ByYou => "identified by you (no hash match)",
        RowStatus::Derivative => {
            derived = d.derived.as_ref().map_or_else(
                || "a derivative, identified by you (no hash match)".to_owned(),
                |x| format!("{}, identified by you (no hash match)", x.label()),
            );
            &derived
        }
        RowStatus::Detected => "identified by ScummVM's detection list",
        _ => "not identified",
    };
    s += &format!(
        "- Platform: {}\n- Status: {status}\n",
        d.row.system_name.as_deref().unwrap_or("?")
    );
    if !d.dats.is_empty() {
        s += &format!("- DAT: {}\n", d.dats.join(", "));
    }
    let show: [(&str, &str); 12] = [
        ("Released", "releaseyear"),
        ("Genre", "genre"),
        ("Players", "users"),
        ("Developer", "developer"),
        ("Publisher", "publisher"),
        ("Country", "country"),
        ("Director", "director"),
        ("Designer", "designer"),
        ("Composer", "composer"),
        ("Series", "series"),
        ("Franchise", "franchise"),
        ("Serial", "serial"),
    ];
    for (label, key) in show {
        if let Some(v) = d.fields.get(key) {
            s += &format!("- {label}: {v}\n");
        }
    }
    if let Some(desc) = d.fields.get("description") {
        s += &format!("\n{desc}\n");
        if let Some(db) = d
            .field_sources
            .get("description")
            .and_then(|k| crate::extdb::ExtDb::parse(k))
        {
            s += &format!("\n_Overview from {}._\n", db.name());
        }
    }
    s += "\n## Files\n\n| File | Size | SHA1 |\n|---|---:|---|\n";
    for f in &d.files {
        let name = f.path.rsplit(['/', '#']).next().unwrap_or(&f.path);
        s += &format!("| {name} | {} | `{}` |\n", f.size, f.sha1);
    }
    let direct: Vec<&view::Link> = d.links.iter().filter(|l| l.direct).collect();
    if !direct.is_empty() {
        s += "\n## Links\n\n";
        for l in direct {
            s += &format!("- [{}]({})\n", l.label, l.url);
        }
    }
    s += "\n_Written by romforge._\n";
    s
}

fn write_zip(item: &Item, extras: &[(String, Vec<u8>)], staging: &Path, out: &Path) -> Result<(), Error> {
    let mut zip = zip::ZipWriter::new(File::create_new(out)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for p in &item.parts {
        zip.start_file(&p.name, opts)?;
        let mut r = open_part(p, staging)?;
        std::io::copy(&mut r, &mut zip)?;
    }
    for (name, data) in extras {
        zip.start_file(format!("romforge/{name}"), opts)?;
        zip.write_all(data)?;
    }
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

/// Sectors in a CD track: data tracks start with the 12-byte sync pattern
/// and say their mode in byte 15; anything else is audio.
fn track_type(first_sector: &[u8]) -> &'static str {
    const SYNC: [u8; 12] = [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];
    if first_sector.len() >= 16 && first_sector[..12] == SYNC {
        match first_sector[15] {
            1 => "MODE1_RAW",
            _ => "MODE2_RAW",
        }
    } else {
        "AUDIO"
    }
}

/// Pregap frames per track from a cue sheet (INDEX 00 to INDEX 01 within a
/// track's FILE), by track order.
fn cue_pregaps(cue: &str) -> Vec<u64> {
    let msf = |s: &str| -> Option<u64> {
        let v: Vec<u64> = s.split(':').filter_map(|x| x.parse().ok()).collect();
        (v.len() == 3).then(|| (v[0] * 60 + v[1]) * 75 + v[2])
    };
    let mut out: Vec<u64> = Vec::new();
    let mut idx0: Option<u64> = None;
    for line in cue.lines().map(str::trim) {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["TRACK", ..] => {
                out.push(0);
                idx0 = None;
            }
            ["INDEX", "00", t] => idx0 = msf(t),
            ["INDEX", "01", t] => {
                if let (Some(a), Some(b), Some(last)) = (idx0, msf(t), out.last_mut()) {
                    *last = b.saturating_sub(a);
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether [`Item::sheet`] was taken into the CHD.
fn write_chd(item: &Item, staging: &Path, out: &Path) -> Result<bool, Error> {
    use chd::metadata::KnownMetadata;
    use chd::write::{ChdWriteOptions, ChdWriter};
    let cerr = |e: chd::Error| Error::Chd(format!("{}: {e}", out.display()));
    let tracks: Vec<&Part> = item
        .parts
        .iter()
        .filter(|p| !p.name.to_lowercase().ends_with(".cue"))
        .collect();
    let file = File::create_new(out)?;
    let mut took_sheet = false;
    if item.format == Format::ChdDvd {
        let p = tracks[0];
        let opts = ChdWriteOptions::dvd().logical_bytes(p.size);
        let mut w = ChdWriter::new(file, opts).map_err(cerr)?;
        std::io::copy(&mut open_part(p, staging)?, &mut w)?;
        w.finish().map_err(cerr)?.sync_all()?;
    } else {
        // Pregaps from the game's own cue sheet when there is one.
        let (cue, took) = own_cue(item, staging);
        took_sheet = took;
        // Several tracks need their pregaps from a cue: without one the CHD
        // would put every track's INDEX 01 in the wrong place.
        if cue.is_none() && tracks.len() > 1 {
            drop(file);
            let _ = std::fs::remove_file(out);
            return Err(Error::Cleanup(format!(
                "{}: no cue sheet that fits its {} tracks, so their pregaps are unknown",
                item.title,
                tracks.len()
            )));
        }
        let pregaps = cue.map(|c| cue_pregaps(&c)).unwrap_or_default();
        let mut w = ChdWriter::new(file, ChdWriteOptions::cd()).map_err(cerr)?;
        let mut meta = Vec::new();
        for (i, p) in tracks.iter().enumerate() {
            if p.size % 2352 != 0 {
                return Err(Error::Cleanup(format!(
                    "{} isn't whole 2352-byte sectors",
                    p.name
                )));
            }
            let frames = p.size / 2352;
            let mut r = open_part(p, staging)?;
            let mut sector = vec![0u8; 2352];
            let mut kind = "AUDIO";
            for f in 0..frames {
                r.read_exact(&mut sector)?;
                if f == 0 {
                    kind = track_type(&sector);
                }
                if kind == "AUDIO" {
                    for pair in sector.as_chunks_mut::<2>().0 {
                        pair.swap(0, 1);
                    }
                }
                w.write_all(&sector)?;
                w.write_all(&[0u8; 96])?;
            }
            for _ in 0..(4 - frames % 4) % 4 {
                w.write_all(&[0u8; 2448])?;
            }
            let pregap = pregaps.get(i).copied().unwrap_or(0).min(frames);
            let pgtype = if pregap > 0 {
                format!("V{kind}")
            } else {
                kind.to_owned()
            };
            meta.push(format!(
                "TRACK:{} TYPE:{kind} SUBTYPE:NONE FRAMES:{frames} PREGAP:{pregap} PGTYPE:{pgtype} PGSUB:NONE POSTGAP:0\0",
                i + 1
            ));
        }
        for m in meta {
            w.add_metadata(KnownMetadata::CdRomTrack2, m.as_bytes(), true)
                .map_err(cerr)?;
        }
        w.finish().map_err(cerr)?.sync_all()?;
    }
    // Read it back: each track must hash as its file did.
    let back = crate::disc::chd_members(out)?;
    if back.len() != tracks.len() {
        return Err(Error::Cleanup(format!(
            "{}: {} tracks read back, {} written",
            out.display(),
            back.len(),
            tracks.len()
        )));
    }
    for ((_, h), p) in back.iter().zip(&tracks) {
        if h.raw.sha1 != p.sha1 {
            return Err(Error::Cleanup(format!(
                "{}: {} doesn't hash as before",
                out.display(),
                p.name
            )));
        }
    }
    Ok(took_sheet)
}

/// The one `.cue` in the same archive or folder as a disc's tracks, when
/// the DAT's cue wasn't found ([`Item::sheet`]).
fn loose_cue(conn: &Connection, parts: &[Part]) -> Result<Option<Part>, Error> {
    let Some(first) = parts.first() else {
        return Ok(None);
    };
    let dir = first
        .path
        .parent()
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .into_owned();
    let found: Vec<Part> = conn
        .prepare(
            "SELECT path, member, size, sha1 FROM files WHERE path = ?1 AND lower(member) LIKE '%.cue'
             UNION SELECT path, member, size, sha1 FROM files WHERE member = '' AND lower(path) LIKE '%.cue'
               AND substr(path, 1, length(?2) + 1) = ?2 || '/' AND instr(substr(path, length(?2) + 2), '/') = 0",
        )?
        .query_map(params![first.path.to_string_lossy(), dir], |r| {
            let (path, member): (String, String) = (r.get(0)?, r.get(1)?);
            let n = if member.is_empty() { &path } else { &member };
            Ok(Part {
                name: n.rsplit('/').next().unwrap_or(n).to_owned(),
                path: PathBuf::from(&path),
                member,
                size: r.get(2)?,
                sha1: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(match <[Part; 1]>::try_from(found) {
        Ok([one]) => Some(one),
        Err(_) => None,
    })
}

/// The game's own cue sheet: one among its parts, else [`Item::sheet`] when
/// its FILE lines name exactly the tracks. `(text, the sheet was taken)`.
fn own_cue(item: &Item, staging: &Path) -> (Option<String>, bool) {
    let read = |p: &Part| -> Option<String> {
        let mut s = String::new();
        open_part(p, staging)
            .ok()?
            .take(1 << 20)
            .read_to_string(&mut s)
            .ok()?;
        Some(s)
    };
    if let Some(p) = item
        .parts
        .iter()
        .find(|p| p.name.to_lowercase().ends_with(".cue"))
    {
        return (read(p), false);
    }
    let Some(sheet) = &item.sheet else {
        return (None, false);
    };
    let tracks: Vec<&str> = item
        .parts
        .iter()
        .map(|p| {
            let n = if p.member.is_empty() {
                p.path.to_str().unwrap_or_default()
            } else {
                &p.member
            };
            n.rsplit('/').next().unwrap_or(n)
        })
        .collect();
    let Some(text) = read(sheet) else {
        return (None, false);
    };
    let names: Vec<String> = crate::disc::parse_cue(&text)
        .into_iter()
        .map(|f| f.name.rsplit(['/', '\\']).next().unwrap_or(&f.name).to_owned())
        .collect();
    // One TRACK per FILE: a single .bin holding several tracks would lose
    // its layout in a CHD built track by track.
    let track_lines = text
        .lines()
        .filter(|l| l.trim_start().to_ascii_uppercase().starts_with("TRACK "))
        .count();
    if names == tracks && track_lines == names.len() {
        (Some(text), true)
    } else {
        (None, false)
    }
}

/// Carry out a plan. Each game is written next to its target, read back
/// and checked, then moved into place; originals go to the trash only after
/// that (and not at all with "keep originals"). Journaled for [`undo`].
pub fn apply(
    conn: &mut Connection,
    plan: &CleanupPlan,
    home: Option<&Home>,
    mut progress: impl FnMut(usize, &str),
) -> Result<CleanupReport, Error> {
    let s = settings::load(conn)?;
    conn.execute(
        "INSERT INTO cleanup_batches (created_at, keep_originals) VALUES (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?1)",
        [plan.keep_originals],
    )?;
    let batch_id = conn.last_insert_rowid();
    let mut report = CleanupReport {
        batch_id,
        ..CleanupReport::default()
    };
    let staging_root = home
        .map_or_else(|| s.staging(), Home::staging)
        .join(format!("batch-{batch_id}"));
    let mut ok: Vec<&Item> = Vec::new();
    let mut failed_paths: BTreeSet<PathBuf> = BTreeSet::new();
    for (n, item) in plan.items.iter().enumerate() {
        progress(n, &item.title);
        if item.existing {
            // Nothing to write; its copies go with the originals below.
            ok.push(item);
            continue;
        }
        let staging = staging_root.join(n.to_string());
        let result = (|| -> Result<bool, Error> {
            if item.target.exists() {
                return Err(Error::Cleanup(format!(
                    "{} already exists",
                    item.target.display()
                )));
            }
            let dir = item.target.parent().unwrap_or(Path::new("."));
            std::fs::create_dir_all(dir)?;
            if item.format == Format::Dir {
                let name = item.target.file_name().unwrap_or_default().to_string_lossy();
                let part = dir.join(format!(".{name}.romforge-part"));
                let _ = std::fs::remove_dir_all(&part);
                let extras = extras(conn, &item.row_id)?;
                if let Err(e) = copy_folder(item, &extras, &part) {
                    let _ = std::fs::remove_dir_all(&part);
                    return Err(e);
                }
                if item.target.exists() {
                    let _ = std::fs::remove_dir_all(&part);
                    return Err(Error::Cleanup(format!(
                        "{} already exists",
                        item.target.display()
                    )));
                }
                std::fs::rename(&part, &item.target)?;
                journal_output(conn, batch_id, &item.target, "dir")?;
                return Ok(false);
            }
            let name = item
                .target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let part = dir.join(format!(".{name}.romforge-part"));
            let _ = std::fs::remove_file(&part);
            let extras = extras(conn, &item.row_id)?;
            let written = match item.format {
                Format::Zip => write_zip(item, &extras, &staging, &part).map(|()| false),
                Format::Folder => write_folder_zip(item, &extras, &part).map(|()| false),
                _ => write_chd(item, &staging, &part),
            };
            let took_sheet = match written {
                Ok(t) => t,
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    return Err(e);
                }
            };
            // The sidecar is written to a temporary folder and renamed too.
            if let Some(side) = &item.sidecar {
                let tmp = dir.join(format!(".{name}.romforge-side"));
                let _ = std::fs::remove_dir_all(&tmp);
                std::fs::create_dir_all(&tmp)?;
                for (f, data) in &extras {
                    std::fs::write(tmp.join(f), data)?;
                }
                if side.exists() {
                    let _ = std::fs::remove_dir_all(&tmp);
                    let _ = std::fs::remove_file(&part);
                    return Err(Error::Cleanup(format!("{} already exists", side.display())));
                }
                std::fs::rename(&tmp, side)?;
                journal_output(conn, batch_id, side, "dir")?;
            }
            if item.target.exists() {
                let _ = std::fs::remove_file(&part);
                return Err(Error::Cleanup(format!(
                    "{} already exists",
                    item.target.display()
                )));
            }
            std::fs::rename(&part, &item.target)?;
            journal_output(conn, batch_id, &item.target, "file")?;
            Ok(took_sheet)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        match result {
            Ok(took_sheet) => {
                report.done += 1;
                ok.push(item);
                // A cue sheet that didn't fit the tracks keeps its file.
                if let (false, Some(sheet)) = (took_sheet, &item.sheet) {
                    failed_paths.insert(sheet.path.clone());
                }
            }
            Err(e) => {
                report.failed.push((item.title.clone(), e.to_string()));
                failed_paths.extend(item.parts.iter().map(|p| p.path.clone()));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&staging_root);
    let written: Vec<&Item> = ok.iter().copied().filter(|i| !i.existing).collect();
    match write_playlists(conn, batch_id, &written) {
        Ok(n) => report.playlists = n,
        Err(e) => report.failed.push(("playlists".into(), e.to_string())),
    }
    // Originals last, once every game in them is safely written.
    if !plan.keep_originals {
        for item in ok {
            for original in item.trash.iter().filter(|p| !failed_paths.contains(*p)) {
                match trash(conn, batch_id, original, home) {
                    Ok(()) => report.trashed += 1,
                    Err(e) => report
                        .failed
                        .push((item.title.clone(), format!("{}: {e}", original.display()))),
                }
            }
        }
    }
    if report.done == 0 && report.trashed == 0 {
        conn.execute("DELETE FROM cleanup_batches WHERE batch_id = ?1", [batch_id])?;
        report.batch_id = 0;
    }
    Ok(report)
}

/// A title's disk or disc number, the set size if it says ("Disk 1 of 4",
/// TOSEC; "Disc 2", Redump), and the title without that group.
pub fn disk_of(title: &str) -> Option<(u32, Option<u32>, String)> {
    for word in ["(Disk ", "(Disc "] {
        let Some(at) = title.find(word) else { continue };
        let rest = &title[at + word.len()..];
        let end = rest.find(')')?;
        let inner = &rest[..end];
        let (n, of) = match inner.split_once(" of ") {
            Some((n, m)) => (n.trim().parse().ok()?, Some(m.trim().parse().ok()?)),
            None => (inner.trim().parse().ok()?, None),
        };
        let base = format!("{}{}", &title[..at], &rest[end + 1..]);
        let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
        return Some((n, of, base.trim().to_owned()));
    }
    None
}

/// For each multi-disk set this batch wrote a disk of, a `<set>.m3u` in
/// the same folder listing its disks in order, once every disk is there
/// (all "of N", else disks 1, 2, ... with no gap). An existing playlist
/// is left alone. Journaled, so undo removes it.
fn write_playlists(conn: &Connection, batch: i64, written: &[&Item]) -> Result<usize, Error> {
    let mut sets: BTreeSet<(PathBuf, String)> = BTreeSet::new();
    for it in written {
        if let (Some((_, _, base)), Some(dir)) = (disk_of(&it.title), it.target.parent()) {
            sets.insert((dir.to_owned(), base));
        }
    }
    let mut n = 0;
    for (dir, base) in sets {
        let list = dir.join(format!("{}.m3u", sanitize(&base)));
        if list.exists() {
            continue;
        }
        // Every game file of the set in the folder, by disk number.
        let mut disks: BTreeMap<u32, (Option<u32>, String)> = BTreeMap::new();
        for e in std::fs::read_dir(&dir)?.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((stem, ext)) = name.rsplit_once('.') else {
                continue;
            };
            if !["zip", "chd", "7z"].contains(&ext.to_ascii_lowercase().as_str()) || !e.path().is_file() {
                continue;
            }
            if let Some((d, of, b)) = disk_of(stem)
                && sanitize(&b) == sanitize(&base)
            {
                disks.entry(d).or_insert((of, name));
            }
        }
        let count = disks.values().find_map(|(of, _)| *of);
        let complete = disks.len() >= 2
            && disks.keys().copied().eq(1..=disks.len() as u32)
            && count.is_none_or(|c| c as usize == disks.len());
        if !complete {
            continue;
        }
        let body: String = disks.values().map(|(_, f)| format!("{f}\n")).collect();
        let tmp = dir.join(format!(".{}.romforge-part", sanitize(&base)));
        std::fs::write(&tmp, body)?;
        if list.exists() {
            let _ = std::fs::remove_file(&tmp);
            continue;
        }
        std::fs::rename(&tmp, &list)?;
        journal_output(conn, batch, &list, "file")?;
        n += 1;
    }
    Ok(n)
}

fn journal_output(conn: &Connection, batch: i64, path: &Path, kind: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO cleanup_outputs (batch_id, path, kind) VALUES (?1, ?2, ?3)",
        params![batch, path.to_string_lossy(), kind],
    )?;
    Ok(())
}

/// `RomForge/.romforge-trash/<batch>/<the original's full path>`, or
/// without a RomForge folder `<folder of original>/.romforge-trash/<batch>/<name>`.
fn trash(conn: &Connection, batch: i64, original: &Path, home: Option<&Home>) -> Result<(), Error> {
    let to = match home {
        Some(h) => {
            let rel: PathBuf = original
                .components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .collect();
            h.trash().join(batch.to_string()).join(rel)
        }
        None => original
            .parent()
            .unwrap_or(Path::new("."))
            .join(TRASH_DIR)
            .join(batch.to_string())
            .join(original.file_name().unwrap_or_default()),
    };
    // Journaled before it moves: a crash in between leaves a row undo can
    // act on, never a trashed file nothing knows about.
    conn.execute(
        "INSERT INTO cleanup_trash (batch_id, original, trashed) VALUES (?1, ?2, ?3)",
        params![batch, original.to_string_lossy(), to.to_string_lossy()],
    )?;
    if let Err(e) = move_path(original, &to) {
        conn.execute(
            "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
            params![batch, to.to_string_lossy()],
        )?;
        return Err(e.into());
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupUndone {
    pub batch_id: i64,
    pub removed: usize,
    pub restored: usize,
    pub failed: Vec<String>,
}

/// Remove a batch's new files and put its originals back (the latest batch
/// by default). An original whose place is taken stays in the trash and is
/// reported.
pub fn undo(conn: &mut Connection, batch: Option<i64>) -> Result<CleanupUndone, Error> {
    let batch_id: i64 = match batch {
        Some(b) => b,
        None => conn
            .query_row(
                "SELECT max(batch_id) FROM cleanup_batches WHERE undone_at IS NULL",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .ok_or_else(|| Error::Cleanup("nothing to undo".into()))?,
    };
    let mut u = CleanupUndone {
        batch_id,
        ..CleanupUndone::default()
    };
    let trashed: Vec<(String, String)> = conn
        .prepare("SELECT original, trashed FROM cleanup_trash WHERE batch_id = ?1")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (original, at) in trashed {
        // Journaled but never moved (a crash in between): nothing to do.
        if !Path::new(&at).exists() && Path::new(&original).exists() {
            conn.execute(
                "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                params![batch_id, at],
            )?;
            continue;
        }
        match move_path(Path::new(&at), Path::new(&original)) {
            Ok(_) => {
                conn.execute(
                    "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                    params![batch_id, at],
                )?;
                u.restored += 1;
                // Empty trash folders go too.
                let mut d = Path::new(&at).parent();
                while let Some(dir) = d {
                    if std::fs::remove_dir(dir).is_err() {
                        break;
                    }
                    d = dir.parent();
                    if dir.file_name().is_some_and(|n| n == TRASH_DIR) {
                        break;
                    }
                }
            }
            Err(e) => u.failed.push(format!("{original}: {e}")),
        }
    }
    // While an original is still in the trash, the new files stay: they
    // may be the only other copy. Undo can be run again.
    if !u.failed.is_empty() {
        u.failed
            .push("the new files were kept, since not every original came back".into());
        return Ok(u);
    }
    let outputs: Vec<(String, String)> = conn
        .prepare("SELECT path, kind FROM cleanup_outputs WHERE batch_id = ?1")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (path, kind) in outputs {
        let p = Path::new(&path);
        let r = if kind == "dir" {
            std::fs::remove_dir_all(p)
        } else {
            std::fs::remove_file(p)
        };
        match r {
            Ok(()) => {
                conn.execute(
                    "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                    params![batch_id, path],
                )?;
                u.removed += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                conn.execute(
                    "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                    params![batch_id, path],
                )?;
            }
            Err(e) => u.failed.push(format!("{path}: {e}")),
        }
    }
    if u.failed.is_empty() {
        conn.execute(
            "UPDATE cleanup_batches SET undone_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE batch_id = ?1",
            [batch_id],
        )?;
    }
    Ok(u)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Batch {
    pub batch_id: i64,
    pub created_at: String,
    pub keep_originals: bool,
    pub outputs: i64,
    pub trashed: i64,
    pub undone_at: Option<String>,
}

pub fn batches(conn: &Connection) -> Result<Vec<Batch>, Error> {
    let mut stmt = conn.prepare(
        "SELECT b.batch_id, b.created_at, b.keep_originals, b.undone_at,
                (SELECT count(*) FROM cleanup_outputs o WHERE o.batch_id = b.batch_id AND o.kind = 'file'),
                (SELECT count(*) FROM cleanup_trash t WHERE t.batch_id = b.batch_id)
         FROM cleanup_batches b ORDER BY b.batch_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Batch {
            batch_id: r.get(0)?,
            created_at: r.get(1)?,
            keep_originals: r.get(2)?,
            undone_at: r.get(3)?,
            outputs: r.get(4)?,
            trashed: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn unsafe_member_names() {
        use super::safe_member_name as ok;
        assert!(ok("Game (USA).sfc") && ok("Disk 1/Game.adf"));
        for bad in [
            "../evil.sfc",
            "/etc/passwd",
            "a/../b",
            "C:\\x",
            "a\\b",
            "romforge/x.png",
            "",
            "a//b",
            "./a",
        ] {
            assert!(!ok(bad), "{bad}");
        }
    }

    #[test]
    fn disk_numbers() {
        assert_eq!(
            disk_of("Ys (1987)(Nihon Falcom)(Disk 2 of 3)"),
            Some((2, Some(3), "Ys (1987)(Nihon Falcom)".into()))
        );
        assert_eq!(
            disk_of("Final Fantasy VII (USA) (Disc 1)"),
            Some((1, None, "Final Fantasy VII (USA)".into()))
        );
        assert_eq!(disk_of("Game (Disk A)"), None);
        assert_eq!(disk_of("Game (USA)"), None);
    }

    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec![
            "Disc (Track 10).bin",
            "Disc (Track 2).bin",
            "Disc (Track 1).bin",
            "Disc.cue",
        ];
        v.sort_by_key(|n| natural_key(n));
        assert_eq!(
            v,
            [
                "Disc (Track 1).bin",
                "Disc (Track 2).bin",
                "Disc (Track 10).bin",
                "Disc.cue"
            ]
        );
    }

    #[test]
    fn cue_pregaps_by_track() {
        let cue = "FILE \"a (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"a (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n";
        assert_eq!(cue_pregaps(cue), [0, 150]);
    }

    #[test]
    fn track_types_from_sectors() {
        let mut s = vec![0u8; 2352];
        assert_eq!(track_type(&s), "AUDIO");
        s[1..11].fill(255);
        s[15] = 2;
        assert_eq!(track_type(&s), "MODE2_RAW");
        s[15] = 1;
        assert_eq!(track_type(&s), "MODE1_RAW");
    }
}
