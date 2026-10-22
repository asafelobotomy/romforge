//! Every row of the library: complete games, games you named, folder
//! games and lone files.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, params};

use super::folders::{folder_games, unknown_sheets};
use super::{FileRec, GameRow, RowStatus, game_id};
use crate::identify::Status;
use crate::report::{GAME_HAVE, HAD, IS_HAVE};
use crate::{Error, systems};

/// Every row of the library, before versions are grouped.
pub(super) fn rows_of(conn: &Connection) -> Result<Vec<GameRow>, Error> {
    let files: Vec<FileRec> = conn
        .prepare("SELECT file_id, path, member, status, size, sha1 FROM files ORDER BY path, member")?
        .query_map([], |r| {
            Ok(FileRec {
                id: r.get(0)?,
                path: r.get(1)?,
                member: r.get(2)?,
                status: r.get(3)?,
                size: r.get(4)?,
                sha1: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    // Complete games and the files that provide them.
    // Keyed by system (none for a DAT romforge doesn't know) and name.
    let mut games: BTreeMap<(Option<i64>, String), (bool, BTreeSet<i64>)> = BTreeMap::new();
    let mut stmt = conn.prepare(&format!(
        concat!(
            "WITH {HAD},
              games_in AS (SELECT DISTINCT r.game_id FROM had JOIN roms r ON r.rom_id = had.rom_id),
              {GAME_HAVE},
              complete AS (
                  SELECT gh.game_id, gh.sheet_have < gh.sheet_total AS cue_differs
                  FROM game_have gh WHERE {IS_HAVE})
         SELECT d.system_id, g.name, c.cue_differs, m.file_id
         FROM complete c JOIN games g ON g.game_id = c.game_id
         JOIN dats d ON d.dat_id = g.dat_id
         JOIN roms r ON r.game_id = g.game_id
         JOIN matches m ON m.rom_id = r.rom_id AND m.out_of_scope = 0 AND m.via IN ",
            crate::report::strong!(),
            "
         JOIN files f ON f.file_id = m.file_id AND f.status IN ",
            crate::report::holding!(),
            "
              AND (m.owns = 1 OR f.status = 'duplicate')"
        ),
        HAD = HAD,
        GAME_HAVE = GAME_HAVE,
        IS_HAVE = IS_HAVE,
    ))?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, Option<i64>>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })? {
        let (system, name, cue_differs, file) = row?;
        let g = games
            .entry((system, name))
            .or_insert((cue_differs, BTreeSet::new()));
        g.0 &= cue_differs;
        g.1.insert(file);
    }

    // A file belongs to every complete game it provides (two revisions with
    // identical data are two rows). Only complete games count, so a short
    // audio track shared by hundreds of discs goes with the disc you have.
    let mut owner: HashMap<i64, Vec<(Option<i64>, String)>> = HashMap::new();
    for (key, (_, ids)) in &games {
        for f in ids {
            owner.entry(*f).or_default().push(key.clone());
        }
    }
    // Rewritten cue sheets that fit a complete game's tracks join it.
    let mut cue_fits: BTreeSet<(Option<i64>, String)> = BTreeSet::new();
    for r in conn
        .prepare(
            "SELECT DISTINCT m.file_id, d.system_id, g.name FROM matches m
             JOIN files f ON f.file_id = m.file_id AND f.status = 'sheet'
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE m.via = 'sheet'",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
    {
        let (file, system, name) = r?;
        let key = (system, name);
        if games.contains_key(&key) {
            owner.entry(file).or_default().push(key.clone());
            cue_fits.insert(key);
        }
    }
    // Members of an archive holding a game stay with that game.
    let mut by_path: HashMap<&str, Vec<(Option<i64>, String)>> = HashMap::new();
    for f in &files {
        if let Some(k) = owner.get(&f.id)
            && !f.member.is_empty()
        {
            by_path.entry(f.path.as_str()).or_insert_with(|| k.clone());
        }
    }

    let meta: BTreeSet<(i64, String)> = conn
        .prepare("SELECT DISTINCT system_id, title FROM game_meta")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut art: BTreeMap<(i64, String), Vec<String>> = BTreeMap::new();
    for row in conn
        .prepare("SELECT system_id, title, kind FROM art WHERE path IS NOT NULL ORDER BY kind")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
    {
        let (s, t, k) = row?;
        art.entry((s, t)).or_default().push(k);
    }

    // What lone files were matched as, for their row title and system:
    // only the hits their status rests on (an unknown file's CRC32 hit
    // never names it).
    let mut seen_as: HashMap<i64, BTreeSet<(Option<i64>, String)>> = HashMap::new();
    for row in conn
        .prepare(&format!(
            "SELECT m.file_id, d.system_id, g.name FROM matches m
             JOIN files f ON f.file_id = m.file_id
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE {}",
            crate::report::RELEVANT_MATCH
        ))?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
    {
        let (f, s, g) = row?;
        seen_as.entry(f).or_default().insert((s, g));
    }

    let mut rows: BTreeMap<String, GameRow> = BTreeMap::new();
    let game_row = |system_id: Option<i64>, title: &str| -> GameRow {
        let sys = system_id.and_then(systems::by_id);
        let key = (system_id, title.to_owned());
        let known = system_id.map(|s| (s, title.to_owned()));
        GameRow {
            id: game_id(system_id, title),
            title: title.to_owned(),
            system: sys.map(|s| s.key.to_owned()),
            system_name: sys.map(|s| s.name.to_owned()),
            status: RowStatus::Verified,
            cue_differs: games.get(&key).is_some_and(|g| g.0),
            cue_fits: cue_fits.contains(&key),
            files: 0,
            size: 0,
            metadata: known.as_ref().is_some_and(|k| meta.contains(k)),
            art: known.and_then(|k| art.get(&k).cloned()).unwrap_or_default(),
            playable: false,
            group: None,
            main: true,
            versions: 1,
            folder: None,
            label: None,
            name: Default::default(),
            category: None,
            edition: None,
            year: None,
            pictures: BTreeMap::new(),
            box_aspect: 72,
            packed: false,
            likely_system: None,
            unmatched: false,
            base_id: None,
            system_id,
            file_ids: Vec::new(),
            patch_of: None,
        }
    };
    // Titles the owner gave files in Search (and whether as a derivative).
    let manual: HashMap<(String, u64), (i64, String, bool)> = conn
        .prepare("SELECT sha1, size, system_id, title, relation IS NOT NULL FROM manual_ids")?
        .query_map([], |r| {
            Ok(((r.get(0)?, r.get(1)?), (r.get(2)?, r.get(3)?, r.get(4)?)))
        })?
        .collect::<Result<_, _>>()?;
    for f in &files {
        if f.status == "manual"
            && let Some((system_id, title, derived)) = manual.get(&(f.sha1.clone(), f.size))
        {
            let id = format!("m:{system_id}:{title}");
            let row = rows.entry(id.clone()).or_insert_with(|| {
                let mut r = game_row(Some(*system_id), title);
                r.id = id;
                r.status = if *derived {
                    RowStatus::Derivative
                } else {
                    RowStatus::ByYou
                };
                r
            });
            row.files += 1;
            row.size += f.size;
            row.file_ids.push(f.id);
            continue;
        }
        let homes = owner.get(&f.id).or_else(|| {
            (!f.member.is_empty())
                .then(|| by_path.get(f.path.as_str()))
                .flatten()
        });
        if let Some(homes) = homes {
            for (system_id, title) in homes {
                let row = rows
                    .entry(game_id(*system_id, title))
                    .or_insert_with(|| game_row(*system_id, title));
                row.files += 1;
                row.size += f.size;
                row.file_ids.push(f.id);
            }
            continue;
        }
        let row = {
            let named: Vec<&(Option<i64>, String)> =
                seen_as.get(&f.id).map(|s| s.iter().collect()).unwrap_or_default();
            let (system_id, title) = match named.as_slice() {
                [(s, g)] => (*s, g.clone()),
                _ => {
                    let shown = if f.member.is_empty() { &f.path } else { &f.member };
                    let name = shown.rsplit('/').next().unwrap_or(shown).to_owned();
                    let systems: BTreeSet<Option<i64>> = named.iter().map(|n| n.0).collect();
                    (
                        systems.into_iter().next().flatten().filter(|_| named.len() < 2),
                        name,
                    )
                }
            };
            // Exhaustive, so a new file status can't fall into Unknown
            // unseen. A sheet only fits a game's tracks (a suggestion); a
            // file the owner named is theirs (never verified either way).
            let status = match Status::parse(&f.status) {
                Some(Status::Matched) => RowStatus::Incomplete,
                Some(Status::Candidate | Status::Sheet) => RowStatus::Candidate,
                Some(Status::WrongSystem) => RowStatus::WrongSystem,
                Some(Status::Duplicate) => RowStatus::Duplicate,
                Some(Status::Manual) => RowStatus::ByYou,
                Some(Status::Unknown) | None => RowStatus::Unknown,
            };
            let id = format!("f:{}", f.id);
            rows.entry(id.clone()).or_insert_with(|| {
                let mut r = game_row(system_id, &title);
                // A lone file is no complete game, whatever its title.
                (r.id, r.status, r.cue_differs, r.cue_fits) = (id, status, false, false);
                r
            })
        };
        row.files += 1;
        row.size += f.size;
        row.file_ids.push(f.id);
    }

    unknown_sheets(conn, &files, &mut rows)?;

    // Folders ScummVM's list names.
    for g in conn
        .prepare("SELECT folder_id, path, system_id, title, files, size FROM folder_games")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, usize>(4)?,
                r.get::<_, u64>(5)?,
            ))
        })?
    {
        let (id, path, system_id, title, n, size) = g?;
        let mut r = game_row(Some(system_id), &title);
        r.id = format!("d:{id}");
        r.status = RowStatus::Detected;
        r.files = n;
        r.size = size;
        r.folder = Some(path);
        rows.insert(r.id.clone(), r);
    }
    folder_games(conn, &files, &mut rows)?;

    let mut rows: Vec<GameRow> = rows.into_values().collect();
    let mut described = conn.prepare(
        "SELECT g.description FROM games g JOIN dats d ON d.dat_id = g.dat_id
         WHERE d.system_id = ?1 AND g.name = ?2 AND g.description IS NOT NULL ORDER BY g.game_id LIMIT 1",
    )?;
    for r in &mut rows {
        r.playable = r.status.identified();
        // ScummVM can't play a game from an archive: cleanup unpacks it.
        r.packed = r
            .system_id
            .and_then(systems::by_id)
            .is_some_and(|s| s.folders == systems::Folders::Kept)
            && r.folder
                .as_deref()
                .is_some_and(|f| crate::scummvm::is_archive(std::path::Path::new(f)));
        if r.packed {
            r.playable = false;
        }
        if r.status == RowStatus::Unknown && r.system_id.is_none() {
            r.likely_system = r
                .title
                .rsplit_once('.')
                .and_then(|(_, ext)| systems::by_extension(ext))
                .map(|s| s.key.to_owned());
        }
        // An arcade set is named by its DAT's description.
        if r.system_id.and_then(systems::by_id).is_some_and(|s| s.arcade) {
            r.label = described
                .query_row(params![r.system_id, r.title], |x| x.get(0))
                .optional()?;
        }
    }
    Ok(rows)
}
