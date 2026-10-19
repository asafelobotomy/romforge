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

use rusqlite::{Connection, OptionalExtension, params};

use crate::hash::{hash_stream, hex};
use crate::home::Home;
use crate::organize::{move_file, sanitize};
use crate::view::{self, RowStatus};
use crate::{Error, layout, settings, systems};

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
    /// One disk of a set, unzipped into the set's folder (floppy
    /// computers whose cores read a set's disks themselves).
    Loose,
    /// An arcade set: its zips (its own, its parent's, its BIOS's) and CHD
    /// folders, copied whole under their own names (milestone 17).
    Set,
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
    /// Each game written in its own folder ("Replace originals", or
    /// Convert), not into `dest`.
    #[serde(default)]
    pub in_place: bool,
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
pub(crate) enum Source {
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
    plan_as(conn, only, home, from, How::Cleanup)
}

/// What a plan is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    /// Rename & Cleanup (or Import): into RomForge or the Library, or in
    /// place with "Replace originals".
    Cleanup,
    /// Convert: disc images into CHD beside themselves, under their own
    /// names, wherever they are.
    Convert,
}

/// Convert's plan: every identified disc game not yet a CHD (CD tracks
/// with their cue sheet, or one DVD image), written as a CHD in its own
/// folder under its own name; the originals as Rename & Cleanup's setting
/// says (kept, or to the trash). Discs CHD can't take (GameCube and Wii's
/// RVZ, odd track files) are listed as skipped.
pub fn convert_plan(
    conn: &Connection,
    only: Option<&[String]>,
    home: Option<&Home>,
) -> Result<CleanupPlan, Error> {
    plan_as(conn, only, home, None, How::Convert)
}

fn plan_as(
    conn: &Connection,
    only: Option<&[String]>,
    home: Option<&Home>,
    from: Option<&Path>,
    how: How,
) -> Result<CleanupPlan, Error> {
    let s = settings::load(conn)?;
    let convert = how == How::Convert;
    let in_place = convert || s.in_place;
    let (source, dest) = match home {
        // Convert takes discs wherever they are.
        _ if convert => (Source::Folder(PathBuf::from("/")), PathBuf::new()),
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
        in_place,
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
        // Convert is for discs; in place, arcade sets and folder games stay
        // as they are (they are already named and laid out by their sets).
        if convert && (!system.is_disc_based || system.arcade || row.folder.is_some()) {
            continue;
        }
        if in_place && (system.arcade || row.folder.is_some()) {
            if only.is_some() {
                plan.skipped.push((
                    row.title.clone(),
                    "an arcade set or folder game stays where it is".into(),
                ));
            }
            continue;
        }
        if system.arcade {
            match set_item(conn, &row, system, &dest, &source, games.as_deref(), &mut taken)? {
                Ok(Some(mut it)) => {
                    if !s.keep_originals {
                        it.trash = it.parts.iter().map(|p| p.path.clone()).collect();
                    }
                    items.push(it);
                }
                Ok(None) => {}
                Err(why) => plan.skipped.push((row.title.clone(), why)),
            }
            continue;
        }
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
        // Already a CHD: nothing to convert.
        if convert
            && parts
                .iter()
                .any(|p| p.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd")))
        {
            continue;
        }
        // Already in Games/: the copies outside are redundant.
        if let (Some(games), false) = (&games, in_place) {
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
        // A CHD only for a platform whose emulators read one (the systems
        // table lists `chd` then): never GameCube, Wii or Xbox.
        if disc && !system.extensions.contains(&"chd") {
            plan.skipped.push((
                row.title.clone(),
                format!("{} emulators don't read CHD; it stays as it is", system.key),
            ));
            continue;
        }
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
        // In place: beside the original (a disc's cue or image, or the
        // archive holding it). Convert keeps the original's own name.
        let here = parts
            .iter()
            .find(|p| p.name.to_lowercase().ends_with(".cue"))
            .or_else(|| tracks.first().copied())
            .or(parts.first())
            .map(|p| p.path.clone())
            .unwrap_or_default();
        let name = if convert {
            here.file_stem()
                .map_or_else(|| sanitize(&row.title), |s| s.to_string_lossy().into_owned())
        } else {
            sanitize(&row.title)
        };
        let dir = if in_place {
            here.parent().map_or_else(PathBuf::new, Path::to_path_buf)
        } else {
            dest.join(sanitize(system.key))
        };
        // A disk of a set, for a core that opens the set's disks itself.
        let set = match (format, system.loose_sets, disk_of(&row.title)) {
            (Format::Zip, Some(_), Some((_, _, base))) if parts.len() == 1 => Some(base),
            _ => None,
        };
        let format = if set.is_some() { Format::Loose } else { format };
        let (target, sidecar) = match (format, &set) {
            (Format::Loose, Some(base)) => {
                let ext = parts[0]
                    .name
                    .rsplit_once('.')
                    .map_or(String::new(), |(_, e)| format!(".{}", e.to_ascii_lowercase()));
                (dir.join(sanitize(base)).join(format!("{name}{ext}")), None)
            }
            (Format::Zip, _) => (dir.join(format!("{name}.zip")), None),
            // Convert writes the disc alone, no folder of art beside it.
            _ if convert => (dir.join(format!("{name}.chd")), None),
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
    for it in items
        .iter()
        .filter(|it| it.folder.is_none() && it.format != Format::Set)
    {
        for p in it.parts.iter().chain(&it.copies).chain(&it.sheet) {
            used.entry(p.path.clone()).or_default().insert(p.member.clone());
        }
    }
    let batch: BTreeSet<String> = items.iter().map(|it| it.row_id.clone()).collect();
    let mut files_at = conn.prepare("SELECT file_id FROM files WHERE path = ?1")?;
    for it in items
        .iter_mut()
        .filter(|it| it.folder.is_none() && it.format != Format::Set)
    {
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
        && !parts[0].eq_ignore_ascii_case(layout::EXTRAS_DIR)
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

/// The import of an arcade set: the files its ROMs and disks are in (its
/// own zip, its parent's and BIOS's, CHDs in a folder named after a set),
/// copied whole under their own names into `Games/<System>/`. A file an
/// earlier game of the batch takes, or one already there, isn't copied
/// again. `Ok(Ok(None))`: nothing outside RomForge to take.
fn set_item(
    conn: &Connection,
    row: &view::GameRow,
    system: &systems::System,
    dest: &Path,
    source: &Source,
    games: Option<&Path>,
    taken: &mut BTreeSet<PathBuf>,
) -> Result<Result<Option<Item>, String>, Error> {
    let dir = dest.join(sanitize(system.key));
    let mut stmt = conn.prepare_cached("SELECT path, member FROM files WHERE file_id = ?1")?;
    let mut parts: Vec<Part> = Vec::new();
    for id in row.file_ids() {
        let (path, member): (String, String) = stmt.query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let path = PathBuf::from(path);
        if games.is_some_and(|g| path.starts_with(g)) || !source.contains(&path) {
            continue;
        }
        let chd = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd"));
        if member.is_empty() && !chd {
            return Ok(Err(format!(
                "{} is a loose file; arcade ROMs are imported in their set's zip",
                path.display()
            )));
        }
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // A CHD lives in a folder named after its set.
        let name = match (chd, path.parent().and_then(Path::file_name)) {
            (true, Some(set)) => format!("{}/{file}", set.to_string_lossy()),
            _ => file,
        };
        if parts.iter().any(|p| p.path == path) || taken.contains(&path) || dir.join(&name).exists() {
            continue;
        }
        let size = std::fs::metadata(&path)?.len();
        parts.push(Part {
            name,
            path,
            member: String::new(),
            size,
            sha1: String::new(),
        });
    }
    if parts.is_empty() {
        return Ok(Ok(None));
    }
    taken.extend(parts.iter().map(|p| p.path.clone()));
    parts.sort_by(|a, b| zip_order(&a.name, &b.name));
    let own = parts
        .iter()
        .find(|p| {
            Path::new(&p.name)
                .file_stem()
                .is_some_and(|s| s.to_string_lossy() == row.title)
        })
        .unwrap_or(&parts[0]);
    Ok(Ok(Some(Item {
        row_id: row.id.clone(),
        title: row.label.clone().unwrap_or_else(|| row.title.clone()),
        system: system.key.to_owned(),
        format: Format::Set,
        target: dir.join(&own.name),
        sidecar: None,
        parts,
        trash: Vec::new(),
        kept: Vec::new(),
        copies: Vec::new(),
        sheet: None,
        existing: false,
        folder: None,
        code: None,
    })))
}

/// Copy an arcade set's files whole, each read back and checked.
fn write_set(conn: &Connection, batch: i64, item: &Item) -> Result<(), Error> {
    let dir = item.target.parent().unwrap_or(Path::new("."));
    for p in &item.parts {
        let to = dir.join(&p.name);
        if to.exists() {
            continue; // another game of the batch brought it
        }
        let folder = to.parent().unwrap_or(dir);
        if !folder.exists() {
            std::fs::create_dir_all(folder)?;
            if folder != dir {
                journal_output(conn, batch, folder, "setdir")?;
            }
        }
        let name = to.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let part = folder.join(layout::part(&name));
        let _ = std::fs::remove_file(&part);
        let r = (|| -> Result<(), Error> {
            let mut w = File::create_new(&part)?;
            let sum = copy_hashed(BufReader::new(File::open(&p.path)?), &mut w)?;
            w.sync_all()?;
            if sha1_of(BufReader::new(File::open(&part)?))? != sum {
                return Err(Error::Cleanup(format!(
                    "{}: doesn't read back as written",
                    to.display()
                )));
            }
            Ok(())
        })();
        if let Err(e) = r {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, &to)?;
        journal_output(conn, batch, &to, "file")?;
    }
    Ok(())
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
fn write_loose(
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
fn zip_order(a: &str, b: &str) -> std::cmp::Ordering {
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
fn write_folder_zip(
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
pub(crate) type PartsAndCopies = (Vec<Part>, Vec<Part>);

/// The ROMs of a hash-verified game: one file per ROM (the copy in Source
/// first, a matched file before a duplicate), from the DAT that has them
/// all, and the other copies of them in Source. Cue sheets are taken when
/// matched; a game is complete without one (a CHD needs none).
pub(crate) fn verified_parts(
    conn: &Connection,
    system_id: i64,
    title: &str,
    source: &Source,
) -> Result<Option<PartsAndCopies>, Error> {
    let mut stmt = conn.prepare(concat!(
        "SELECT g.game_id, r.rom_id, r.name, f.path, f.member, f.size, f.sha1, f.status, m.owns
         FROM games g JOIN dats d ON d.dat_id = g.dat_id
         JOIN roms r ON r.game_id = g.game_id
         LEFT JOIN matches m ON m.rom_id = r.rom_id AND m.out_of_scope = 0 AND m.via IN ",
        crate::report::strong!(),
        "
         LEFT JOIN files f ON f.file_id = m.file_id AND f.status IN ",
        crate::report::holding!(),
        "
         WHERE d.system_id = ?1 AND g.name = ?2 AND coalesce(r.status, '') NOT IN ('nodump', 'optional')
         ORDER BY g.game_id, r.rom_id, f.path, f.member"
    ))?;
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
            r.get::<_, Option<bool>>(8)?,
        ))
    })? {
        let (game, rom, name, path, member, size, sha1, status, owns) = r?;
        let entry = games
            .entry(game)
            .or_default()
            .entry(rom)
            .or_insert((name.clone(), Vec::new()));
        if let (Some(path), Some(member), Some(size), Some(sha1), Some(status)) =
            (path, member, size, sha1, status)
        {
            let path = PathBuf::from(path);
            // In Source first, then the copy that owns the ROM (beside the
            // game's other files), then a matched file before a duplicate.
            let rank = (!source.contains(&path)) as u8 * 4
                + (owns != Some(true)) as u8 * 2
                + (status != "matched") as u8;
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
pub(crate) fn named_parts(
    conn: &Connection,
    row_id: &str,
    source: &Source,
) -> Result<Option<PartsAndCopies>, Error> {
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
pub(crate) fn open_part(part: &Part, staging: &Path) -> Result<Box<dyn Read>, Error> {
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
        format!("{}{}", sanitize(&d.row.title), layout::DETAILS_SUFFIX),
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

/// Sectors in a CD track: data tracks start with the 12-byte sync pattern
/// and say their mode in byte 15; anything else is audio.
fn track_type(first_sector: &[u8]) -> &'static str {
    if first_sector.len() >= 16 && first_sector[..12] == crate::disc::SYNC {
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
pub(crate) fn write_chd(item: &Item, staging: &Path, out: &Path) -> Result<bool, Error> {
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
pub(crate) fn loose_cue(conn: &Connection, parts: &[Part]) -> Result<Option<Part>, Error> {
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
        concat!(
            "INSERT INTO cleanup_batches (created_at, keep_originals) VALUES (",
            crate::db::now!(),
            ", ?1)"
        ),
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
            let new_dir = !dir.exists();
            std::fs::create_dir_all(dir)?;
            if item.format == Format::Set {
                if new_dir {
                    journal_output(conn, batch_id, dir, "setdir")?;
                }
                write_set(conn, batch_id, item)?;
                return Ok(false);
            }
            if item.format == Format::Loose {
                // The set's folder is journaled before what goes in it, so
                // undo (last first) empties it before removing it.
                if new_dir {
                    journal_output(conn, batch_id, dir, "setdir")?;
                }
                let written = extras(conn, &item.row_id)
                    .and_then(|extras| write_loose(conn, batch_id, item, &extras, &staging));
                if let Err(e) = written {
                    if new_dir && std::fs::remove_dir(dir).is_ok() {
                        conn.execute(
                            "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                            params![batch_id, dir.to_string_lossy()],
                        )?;
                    }
                    return Err(e);
                }
                return Ok(false);
            }
            if item.format == Format::Dir {
                let name = item.target.file_name().unwrap_or_default().to_string_lossy();
                let part = dir.join(layout::part(&name));
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
            let part = dir.join(layout::part(&name));
            let _ = std::fs::remove_file(&part);
            let extras = extras(conn, &item.row_id)?;
            let written = match item.format {
                Format::Zip => write_zip(item, &extras, &staging, &part).map(|()| false),
                Format::Folder => write_folder_zip(item, &extras, &staging, &part).map(|()| false),
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
                let tmp = dir.join(layout::side(&name));
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
    // A batch that left nothing behind (no file, no journal row) is
    // forgotten.
    if report.done == 0 && report.trashed == 0 {
        let gone = conn.execute(
            "DELETE FROM cleanup_batches WHERE batch_id = ?1
               AND NOT EXISTS (SELECT 1 FROM cleanup_outputs WHERE batch_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM cleanup_trash WHERE batch_id = ?1)",
            [batch_id],
        )?;
        if gone > 0 {
            report.batch_id = 0;
        }
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

/// For each multi-disk set this batch wrote a disk of, a playlist in the
/// same folder listing its disks in order, once every disk is there (all
/// "of N", else disks 1, 2, ... with no gap): `<set>.m3u` of the zips or
/// CHDs; for a set kept loose, the playlist its core reads (`.m3u` of the
/// images, or np2kai's `.cmd`, one command line), or none. An existing
/// playlist is left alone. Journaled, so undo removes it.
fn write_playlists(conn: &Connection, batch: i64, written: &[&Item]) -> Result<usize, Error> {
    // (folder, set, playlist kind, loose)
    let mut sets: BTreeSet<(PathBuf, String, &'static str, bool)> = BTreeSet::new();
    for it in written {
        let Some((_, _, base)) = disk_of(&it.title) else {
            continue;
        };
        let Some(dir) = it.target.parent() else { continue };
        let kind = match it.format {
            Format::Loose => systems::by_key(&it.system)
                .and_then(|s| s.loose_sets)
                .unwrap_or(""),
            _ => "m3u",
        };
        if !kind.is_empty() {
            sets.insert((dir.to_owned(), base, kind, it.format == Format::Loose));
        }
    }
    let mut n = 0;
    for (dir, base, kind, loose) in sets {
        let list = dir.join(format!("{}.{kind}", sanitize(&base)));
        if list.exists() {
            continue;
        }
        // Every disk of the set in the folder, by disk number.
        let mut disks: BTreeMap<u32, (Option<u32>, String)> = BTreeMap::new();
        for e in std::fs::read_dir(&dir)?.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((stem, ext)) = name.rsplit_once('.') else {
                continue;
            };
            let ext = ext.to_ascii_lowercase();
            let wanted = if loose {
                !name.starts_with('.') && !["m3u", "cmd"].contains(&ext.as_str())
            } else {
                ["zip", "chd", "7z"].contains(&ext.as_str())
            };
            if !wanted || !e.path().is_file() {
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
        let body: String = if kind == "cmd" {
            // np2kai reads one line of at most 1023 bytes: the program,
            // then the disks (quoted, relative to the file).
            let line: String = std::iter::once("np2kai".to_owned())
                .chain(disks.values().map(|(_, f)| format!("\"{f}\"")))
                .collect::<Vec<_>>()
                .join(" ");
            if line.len() > 1023 || disks.values().any(|(_, f)| f.contains('"')) {
                continue;
            }
            line + "\n"
        } else {
            disks.values().map(|(_, f)| format!("{f}\n")).collect()
        };
        let tmp = dir.join(layout::part(&sanitize(&base)));
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

pub(crate) fn journal_output(conn: &Connection, batch: i64, path: &Path, kind: &str) -> Result<(), Error> {
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
            .join(layout::TRASH_DIR)
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
                "SELECT max(batch_id) FROM cleanup_batches WHERE undone_at IS NULL AND emptied_at IS NULL",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .ok_or_else(|| Error::Cleanup("nothing to undo".into()))?,
    };
    let emptied: Option<String> = conn
        .query_row(
            "SELECT emptied_at FROM cleanup_batches WHERE batch_id = ?1",
            [batch_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(when) = emptied {
        return Err(Error::Cleanup(format!(
            "batch #{batch_id}'s originals were deleted when the trash was emptied ({when}); it can't be undone"
        )));
    }
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
                    if dir.file_name().is_some_and(|n| n == layout::TRASH_DIR) {
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
        .prepare("SELECT path, kind FROM cleanup_outputs WHERE batch_id = ?1 ORDER BY rowid DESC")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (path, kind) in outputs {
        let p = Path::new(&path);
        let r = if kind == "dir" {
            std::fs::remove_dir_all(p)
        } else if kind == "setdir" {
            // A folder this batch made for a set: gone once it is empty;
            // anything put in it since stays.
            match std::fs::remove_dir(p) {
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
                r => r,
            }
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
            concat!(
                "UPDATE cleanup_batches SET undone_at = ",
                crate::db::now!(),
                " WHERE batch_id = ?1"
            ),
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
    /// When its originals were deleted from the trash: it can't be undone.
    pub emptied_at: Option<String>,
}

pub fn batches(conn: &Connection) -> Result<Vec<Batch>, Error> {
    let mut stmt = conn.prepare(
        "SELECT b.batch_id, b.created_at, b.keep_originals, b.undone_at, b.emptied_at,
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
            emptied_at: r.get(4)?,
            outputs: r.get(5)?,
            trashed: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One batch's originals waiting in the trash.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TrashBatch {
    pub batch_id: i64,
    pub created_at: String,
    pub files: usize,
    pub bytes: u64,
}

/// What emptying the trash removes (or removed).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct TrashSummary {
    pub batches: Vec<TrashBatch>,
    pub files: usize,
    pub bytes: u64,
    /// What couldn't be deleted (left in the trash).
    pub failed: Vec<String>,
}

/// Files under a path (a trashed file, or a trashed folder game), and
/// their size.
fn on_disk(p: &Path) -> (usize, u64) {
    walkdir::WalkDir::new(p)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .fold((0, 0), |(n, b), e| {
            (n + 1, b + e.metadata().map_or(0, |m| m.len()))
        })
}

/// The trash's batches (not undone, not emptied), with what each holds:
/// all of them, or those made more than `older_than_days` ago.
pub fn trash_contents(conn: &Connection, older_than_days: Option<u32>) -> Result<TrashSummary, Error> {
    let cutoff = older_than_days.map(|d| format!("-{d} days"));
    let ids: Vec<(i64, String)> = conn
        .prepare(
            "SELECT b.batch_id, b.created_at FROM cleanup_batches b
             WHERE b.undone_at IS NULL AND b.emptied_at IS NULL
               AND EXISTS (SELECT 1 FROM cleanup_trash t WHERE t.batch_id = b.batch_id)
               AND (?1 IS NULL OR b.created_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1))
             ORDER BY b.batch_id",
        )?
        .query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut out = TrashSummary::default();
    let mut paths = conn.prepare("SELECT trashed FROM cleanup_trash WHERE batch_id = ?1")?;
    for (batch_id, created_at) in ids {
        let (mut files, mut bytes) = (0, 0);
        for p in paths.query_map([batch_id], |r| r.get::<_, String>(0))? {
            let (n, b) = on_disk(Path::new(&p?));
            files += n;
            bytes += b;
        }
        out.files += files;
        out.bytes += bytes;
        out.batches.push(TrashBatch {
            batch_id,
            created_at,
            files,
            bytes,
        });
    }
    Ok(out)
}

/// Delete the originals [`trash_contents`] lists, for good. Each batch is
/// marked emptied first, so it can't be undone from then on (its new files
/// would go with nothing to put back); what can't be deleted stays listed
/// in `failed`, and in the journal.
pub fn empty_trash(conn: &Connection, older_than_days: Option<u32>) -> Result<TrashSummary, Error> {
    let mut out = trash_contents(conn, older_than_days)?;
    for b in &out.batches {
        conn.execute(
            concat!(
                "UPDATE cleanup_batches SET emptied_at = ",
                crate::db::now!(),
                " WHERE batch_id = ?1"
            ),
            [b.batch_id],
        )?;
        let rows: Vec<String> = conn
            .prepare("SELECT trashed FROM cleanup_trash WHERE batch_id = ?1")?
            .query_map([b.batch_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for at in rows {
            let p = Path::new(&at);
            let r = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
            match r {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    out.failed.push(format!("{at}: {e}"));
                    continue;
                }
            }
            conn.execute(
                "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                params![b.batch_id, at],
            )?;
            // The batch's folders in the trash go once empty.
            let mut d = p.parent();
            while let Some(dir) = d {
                if dir.file_name().is_some_and(|n| n == layout::TRASH_DIR)
                    || std::fs::remove_dir(dir).is_err()
                {
                    break;
                }
                d = dir.parent();
            }
        }
    }
    Ok(out)
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
