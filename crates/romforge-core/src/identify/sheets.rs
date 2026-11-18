use super::*;
use std::collections::BTreeSet;

/// Unknown cue (or GDI) sheets whose files are exactly a game's tracks (in
/// order), every one hash-matched in the same archive or folder: status
/// `sheet`, with a match to the game's cue ROM (`via` = 'sheet', which
/// never counts as had).
pub(super) fn fit_sheets(tx: &Transaction) -> Result<(), Error> {
    let sheets: Vec<(i64, String, String, String)> = tx
        .prepare(
            "SELECT file_id, path, member, sheet_files FROM files
             WHERE status = 'unknown' AND sheet_files IS NOT NULL",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    if sheets.is_empty() {
        return Ok(());
    }
    // Hash-matched files beside it: in the same archive, or loose in the
    // same folder (a range on `path`, which the (path, member) index serves:
    // '0' comes right after '/').
    let mut beside_archive = tx.prepare(concat!(
        "SELECT file_id, member FROM files WHERE path = ?1 AND member <> ''
           AND status IN ",
        crate::report::holding!()
    ))?;
    let mut beside_folder = tx.prepare(concat!(
        "SELECT file_id, path FROM files WHERE member = '' AND status IN ",
        crate::report::holding!(),
        "
           AND path > ?1 || '/' AND path < ?1 || '0' AND instr(substr(path, length(?1) + 2), '/') = 0"
    ))?;
    // Each hash-matched file's ROMs (strong matches), with their game and
    // whether that match is in its folder's scope.
    let mut matched_of = tx.prepare(concat!(
        "SELECT m.rom_id, r.game_id, m.out_of_scope FROM matches m JOIN roms r ON r.rom_id = m.rom_id
         WHERE m.file_id = ?1 AND m.via IN ",
        crate::report::strong!()
    ))?;
    let mut roms_of = tx.prepare(
        "SELECT rom_id, name, lower(name) LIKE '%.cue' FROM roms
         WHERE game_id = ?1 AND coalesce(status, '') NOT IN ('nodump', 'optional') ORDER BY rom_id",
    )?;
    let mut insert = tx.prepare(
        "INSERT INTO matches (file_id, rom_id, via, stripped, out_of_scope) VALUES (?1, ?2, 'sheet', 0, 0)",
    )?;
    let mut set_status = tx.prepare("UPDATE files SET status = 'sheet' WHERE file_id = ?1")?;
    // What is beside the sheets of one archive or folder, worked out once
    // for all of them: the games its files match in scope, and every ROM
    // they match.
    let mut places: HashMap<(String, bool), (BTreeSet<i64>, HashSet<i64>)> = HashMap::new();
    let mut roms_cache: HashMap<i64, Vec<(i64, String, bool)>> = HashMap::new();
    for (file_id, path, member, names) in sheets {
        let Ok(names) = serde_json::from_str::<Vec<String>>(&names) else {
            continue;
        };
        let loose = member.is_empty();
        let place = if loose {
            std::path::Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            path
        };
        let (games, matched) = match places.entry((place, loose)) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let (place, _) = e.key();
                let beside: Vec<i64> = if loose {
                    beside_folder
                        .query_map([place], |r| r.get(0))?
                        .collect::<Result<_, _>>()?
                } else {
                    beside_archive
                        .query_map([place], |r| r.get(0))?
                        .collect::<Result<_, _>>()?
                };
                let (mut games, mut matched) = (BTreeSet::new(), HashSet::new());
                for f in &beside {
                    for row in matched_of.query_map([f], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, bool>(2)?))
                    })? {
                        let (rom, game, out) = row?;
                        matched.insert(rom);
                        if !out {
                            games.insert(game);
                        }
                    }
                }
                e.insert((games, matched))
            }
        };
        for game in games.iter() {
            let roms = match roms_cache.entry(*game) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => e.insert(
                    roms_of
                        .query_map([game], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                        .collect::<Result<_, _>>()?,
                ),
            };
            let tracks: Vec<&(i64, String, bool)> = roms.iter().filter(|r| !r.2).collect();
            // Its tracks, by name and in order, each matched by a file
            // beside the sheet.
            let fits = tracks.len() == names.len()
                && tracks
                    .iter()
                    .zip(&names)
                    .all(|(t, n)| t.1.eq_ignore_ascii_case(n) && matched.contains(&t.0));
            if !fits {
                continue;
            }
            for cue in roms.iter().filter(|r| r.2) {
                insert.execute(params![file_id, cue.0])?;
            }
            set_status.execute([file_id])?;
            break;
        }
    }
    Ok(())
}
