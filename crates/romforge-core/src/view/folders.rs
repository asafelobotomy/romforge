//! Games that are whole folders (DOS, ScummVM), and unknown disc sheets
//! with the tracks they name.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::Connection;

use super::{FileRec, GameRow, RowStatus};
use crate::{Error, systems};

/// Whether a file is a DOS program, which stands for the folder it is in.
pub(crate) fn is_program(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, e)| ["exe", "com", "bat"].contains(&e.to_ascii_lowercase().as_str()))
}

/// Whether `f` (a file, or an archive's member) lies in `place`: the
/// archive itself, or a file below the folder.
fn inside(place: &str, f: &FileRec) -> bool {
    f.path == place
        || f.path
            .strip_prefix(place)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Games of a system whose games are folders (`System::folders`): a DOS
/// program you named (or confirmed from DOS.dat) stands for the archive
/// it is in, else its folder, unless that folder is a scanned folder
/// itself or holds another identified game. Unidentified files in a
/// game's folder (and every file in a folder ScummVM's list named) join
/// its row instead of being rows of their own.
pub(super) fn folder_games(
    conn: &Connection,
    files: &[FileRec],
    rows: &mut BTreeMap<String, GameRow>,
) -> Result<(), Error> {
    let by_id: HashMap<i64, &FileRec> = files.iter().map(|f| (f.id, f)).collect();
    let roots: BTreeSet<String> = conn
        .prepare("SELECT path FROM scan_roots")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut places: Vec<(String, String)> = Vec::new();
    for r in rows.values() {
        if let Some(p) = &r.folder {
            places.push((r.id.clone(), p.clone()));
            continue;
        }
        let Some(sys) = r.system_id.and_then(systems::by_id) else {
            continue;
        };
        if sys.folders == systems::Folders::No
            || !matches!(
                r.status,
                RowStatus::ByYou | RowStatus::Derivative | RowStatus::Verified
            )
        {
            continue;
        }
        let recs: Vec<&FileRec> = r.file_ids.iter().filter_map(|i| by_id.get(i).copied()).collect();
        let place_of = |f: &FileRec| -> Option<String> {
            if f.member.is_empty() {
                f.path.rsplit_once('/').map(|(d, _)| d.to_owned())
            } else {
                Some(f.path.clone())
            }
        };
        let Some(place) = recs.first().and_then(|f| place_of(f)) else {
            continue;
        };
        let programs = recs
            .iter()
            .all(|f| is_program(if f.member.is_empty() { &f.path } else { &f.member }));
        if !programs || roots.contains(&place) || recs.iter().any(|f| place_of(f).as_ref() != Some(&place)) {
            continue;
        }
        let crowded = rows.values().any(|o| {
            o.id != r.id
                && o.status.identified()
                && o.file_ids
                    .iter()
                    .any(|i| by_id.get(i).is_some_and(|f| inside(&place, f)))
        });
        if !crowded {
            places.push((r.id.clone(), place));
        }
    }
    for (owner, place) in places {
        let joining: Vec<String> = rows
            .iter()
            .filter(|(id, r)| {
                **id != owner
                    && !r.status.identified()
                    && !r.file_ids.is_empty()
                    && r.file_ids
                        .iter()
                        .all(|i| by_id.get(i).is_some_and(|f| inside(&place, f)))
            })
            .map(|(id, _)| id.clone())
            .collect();
        let mut taken = Vec::new();
        for id in joining {
            if let Some(r) = rows.remove(&id) {
                taken.push(r);
            }
        }
        let Some(o) = rows.get_mut(&owner) else { continue };
        let detected = o.status == RowStatus::Detected;
        for r in taken {
            // A detected folder's size and count are its own (from disk).
            if !detected {
                o.files += r.files;
                o.size += r.size;
            }
            o.file_ids.extend(r.file_ids);
        }
        o.folder = Some(place);
    }
    Ok(())
}

/// An unknown cue (or GDI) sheet and the unknown tracks it names beside it
/// are one row (a patched disc, say), so Search names them together.
pub(super) fn unknown_sheets(
    conn: &Connection,
    files: &[FileRec],
    rows: &mut BTreeMap<String, GameRow>,
) -> Result<(), Error> {
    let place = |f: &FileRec| -> (String, String) {
        let (path, name) = if f.member.is_empty() {
            ("", f.path.as_str())
        } else {
            (f.path.as_str(), f.member.as_str())
        };
        let (dir, base) = name.rsplit_once('/').unwrap_or(("", name));
        (format!("{path}#{dir}"), base.to_lowercase())
    };
    let by_id: HashMap<i64, &FileRec> = files.iter().map(|f| (f.id, f)).collect();
    let sheets: Vec<(i64, String)> = conn
        .prepare(
            "SELECT file_id, sheet_files FROM files WHERE status = 'unknown' AND sheet_files IS NOT NULL",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, listed) in sheets {
        let (Some(sheet), Ok(listed)) = (by_id.get(&id), serde_json::from_str::<Vec<String>>(&listed)) else {
            continue;
        };
        let (at, _) = place(sheet);
        let wanted: BTreeSet<String> = listed.iter().map(|n| n.to_lowercase()).collect();
        let tracks: Vec<String> = rows
            .iter()
            .filter(|(rid, r)| {
                **rid != format!("f:{id}")
                    && r.status == RowStatus::Unknown
                    && r.file_ids.len() == 1
                    && by_id
                        .get(&r.file_ids[0])
                        .is_some_and(|f| place(f).0 == at && wanted.contains(&place(f).1))
            })
            .map(|(rid, _)| rid.clone())
            .collect();
        if tracks.is_empty()
            || !rows
                .get(&format!("f:{id}"))
                .is_some_and(|r| r.status == RowStatus::Unknown)
        {
            continue;
        }
        for t in tracks {
            let Some(track) = rows.remove(&t) else { continue };
            let Some(row) = rows.get_mut(&format!("f:{id}")) else {
                continue;
            };
            row.files += track.files;
            row.size += track.size;
            row.file_ids.extend(track.file_ids);
        }
    }
    Ok(())
}
