use super::*;

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
    // same folder.
    let mut beside_archive = tx.prepare(concat!(
        "SELECT file_id, member FROM files WHERE path = ?1 AND member <> ''
           AND status IN ",
        crate::report::holding!()
    ))?;
    let mut beside_folder = tx.prepare(concat!(
        "SELECT file_id, path FROM files WHERE member = '' AND status IN ",
        crate::report::holding!(),
        "
           AND substr(path, 1, length(?1) + 1) = ?1 || '/' AND instr(substr(path, length(?1) + 2), '/') = 0"
    ))?;
    let mut games_of = tx.prepare(concat!(
        "SELECT DISTINCT r.game_id FROM matches m JOIN roms r ON r.rom_id = m.rom_id
         WHERE m.file_id = ?1 AND m.via IN ",
        crate::report::strong!(),
        " AND m.out_of_scope = 0"
    ))?;
    let mut roms_of = tx.prepare(
        "SELECT rom_id, name, lower(name) LIKE '%.cue' FROM roms
         WHERE game_id = ?1 AND coalesce(status, '') NOT IN ('nodump', 'optional') ORDER BY rom_id",
    )?;
    let mut matched_by = tx.prepare(concat!(
        "SELECT 1 FROM matches WHERE rom_id = ?1 AND file_id = ?2 AND via IN ",
        crate::report::strong!()
    ))?;
    let mut insert = tx.prepare(
        "INSERT INTO matches (file_id, rom_id, via, stripped, out_of_scope) VALUES (?1, ?2, 'sheet', 0, 0)",
    )?;
    let mut set_status = tx.prepare("UPDATE files SET status = 'sheet' WHERE file_id = ?1")?;
    for (file_id, path, member, names) in sheets {
        let Ok(names) = serde_json::from_str::<Vec<String>>(&names) else {
            continue;
        };
        let beside: Vec<i64> = if member.is_empty() {
            let dir = std::path::Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            beside_folder
                .query_map([&dir], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        } else {
            beside_archive
                .query_map([&path], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        let mut games = std::collections::BTreeSet::new();
        for f in &beside {
            for g in games_of.query_map([f], |r| r.get::<_, i64>(0))? {
                games.insert(g?);
            }
        }
        for game in games {
            let roms: Vec<(i64, String, bool)> = roms_of
                .query_map([game], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?;
            let tracks: Vec<&(i64, String, bool)> = roms.iter().filter(|r| !r.2).collect();
            let mut fits = tracks.len() == names.len()
                && tracks
                    .iter()
                    .zip(&names)
                    .all(|(t, n)| t.1.eq_ignore_ascii_case(n));
            // Every track matched by a file beside the sheet.
            for t in &tracks {
                if !fits {
                    break;
                }
                let mut found = false;
                for f in &beside {
                    if matched_by.exists(params![t.0, f])? {
                        found = true;
                        break;
                    }
                }
                fits = found;
            }
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
