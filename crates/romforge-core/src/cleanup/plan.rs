use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::home::Home;
use crate::organize::sanitize;
use crate::view::{self, RowStatus};
use crate::{Error, settings, systems};

use super::apply::disk_of;
use super::chd::loose_cue;
use super::items::{folder_item, safe_member_name, set_item};
use super::parts::{named_parts, verified_parts};
use super::{CleanupPlan, Format, How, Item, Part, Source};

mod native;
mod originals;

pub(crate) use native::TITLE_FOLDER_STAYS;

pub(super) fn plan_as(
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
    let mut held_at = conn.prepare("SELECT path FROM files WHERE sha1 = ?1 AND size = ?2")?;

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
    // Rows with nothing of their own to take: (row id, its files).
    let mut covered: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for row in v.rows.clone() {
        if only.is_some_and(|ids| !ids.contains(&row.id)) {
            continue;
        }
        if !row.status.identified() {
            // A game you don't have (listed for its translations) has
            // nothing to clean up, picked or not.
            if only.is_some() && row.status != RowStatus::Missing {
                plan.skipped
                    .push((row.title.clone(), "identify it first (Search)".into()));
            }
            continue;
        }
        let Some(system) = row.system.as_deref().and_then(systems::by_key) else {
            continue;
        };
        // Convert is for discs; in place, arcade sets and folder games stay
        // put (laid out by their sets), but a packed ScummVM game unpacks.
        if convert && (!system.is_disc_based || system.arcade || row.folder.is_some()) {
            continue;
        }
        if in_place && (system.arcade || (row.folder.is_some() && !row.packed)) {
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
                // Its files go to the trash as any original does, below:
                // a BIOS or parent zip other games still need stays.
                Ok(Some(it)) => items.push(it),
                // Every file of it taken by games before it (a BIOS): it
                // counts as in the batch if they all are.
                Ok(None) => {
                    let mut paths = Vec::new();
                    for f in row.file_ids() {
                        paths.push(PathBuf::from(path_of.query_row([f], |r| r.get::<_, String>(0))?));
                    }
                    covered.push((row.id.clone(), paths));
                }
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
                in_place,
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
        // An eShop download (`title.tmd` and its encrypted contents): Cemu
        // reads the whole folder, so it stays as it is. (A decrypted title
        // folder becomes a WUA, below.)
        let mut in_folder = false;
        for f in row.file_ids() {
            let p: String = path_of.query_row([f], |r| r.get(0))?;
            in_folder |= native::is_eshop(Path::new(&p));
        }
        if in_folder {
            if only.is_some() {
                plan.skipped.push((row.title.clone(), TITLE_FOLDER_STAYS.into()));
            }
            continue;
        }
        let parts = match row.status {
            RowStatus::Verified => verified_parts(conn, system.id, &row.title, &source)?,
            _ => named_parts(conn, &v, &row.id, &source)?,
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
        // Already in Games/: the copies outside are redundant, but only when
        // every ROM (a CHD holds no cue sheet) is held there, by size and
        // SHA1; one file of the game there isn't the game.
        if let (Some(games), false) = (&games, in_place) {
            let mut there = None;
            for f in row.file_ids() {
                let p: String = path_of.query_row([f], |r| r.get(0))?;
                if Path::new(&p).starts_with(games) {
                    there = Some(PathBuf::from(p));
                    break;
                }
            }
            let mut held = |p: &Part| -> Result<bool, Error> {
                let paths = held_at
                    .query_map(rusqlite::params![p.sha1, p.size], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(paths.iter().any(|q| Path::new(q).starts_with(games)))
            };
            let mut all_held = true;
            for p in parts.iter().filter(|p| !p.name.to_lowercase().ends_with(".cue")) {
                if !held(p)? {
                    all_held = false;
                    break;
                }
            }
            if let (Some(target), true) = (there, all_held) {
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
        // table lists `chd` then); else the format they do read, if any.
        let native = if disc {
            match native::native(system, &parts) {
                Ok(n) => n,
                Err(why) => {
                    plan.skipped.push((row.title.clone(), why));
                    continue;
                }
            }
        } else {
            native::Native::Chd
        };
        let tracks: Vec<&Part> = parts
            .iter()
            .filter(|p| !p.name.to_lowercase().ends_with(".cue"))
            .collect();
        let (format, native_ext, title_folder) = native.split();
        let format = if let Some(f) = format {
            f
        } else if !disc {
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
        let here = title_folder.clone().unwrap_or_else(|| {
            parts
                .iter()
                .find(|p| p.name.to_lowercase().ends_with(".cue"))
                .or_else(|| tracks.first().copied())
                .or(parts.first())
                .map(|p| p.path.clone())
                .unwrap_or_default()
        });
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
            (Format::Rvz | Format::Z3ds | Format::Wua, _) if convert => {
                (dir.join(format!("{name}.{native_ext}")), None)
            }
            (Format::Rvz | Format::Z3ds | Format::Wua, _) => {
                (dir.join(format!("{name}.{native_ext}")), Some(dir.join(&name)))
            }
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
            // A title folder goes to the trash whole, as a folder game does.
            trash: title_folder
                .iter()
                .filter(|_| !s.keep_originals)
                .cloned()
                .collect(),
            kept: Vec::new(),
            copies,
            sheet,
            existing: false,
            folder: title_folder,
            code: None,
        });
    }

    let mut files_at = conn.prepare("SELECT file_id FROM files WHERE path = ?1")?;
    originals::mark(&mut items, &covered, &rows_of, &in_source, |p| {
        Ok(files_at
            .query_map([p.to_string_lossy()], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<i64>, _>>()?)
    })?;
    plan.items = items;
    Ok(plan)
}
