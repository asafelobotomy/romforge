use std::path::Path;

use rusqlite::{Connection, params};

use super::found::stem_lower;
use super::naming::name_patched;
use crate::view::{self, RowStatus};
use crate::{Error, systems};

/// A file of the library, for pairing.
pub(super) struct LibFile {
    pub(super) path: String,
    pub(super) member: String,
    pub(super) status: String,
    pub(super) size: u64,
    pub(super) crc32: String,
    pub(super) sha1: String,
    pub(super) stripped: Option<(u64, String)>,
}

/// Pair every patch found with the game it is for, then name the files
/// that a patch beside them proves are that patch applied to a verified
/// copy of its game; re-identify when it named any. Run after identifying.
pub(crate) fn refresh(tx: &rusqlite::Transaction) -> Result<(), Error> {
    let any: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM patch_files)", [], |r| r.get(0))?;
    if !any {
        return Ok(());
    }
    pair(tx)?;
    if name_patched(tx)? > 0 {
        crate::identify::identify(tx, crate::identify::Relook::Nothing)?;
    }
    Ok(())
}

/// Pair each patch with a game: by the ROM it was made from (BPS / UPS:
/// size and CRC32, your identified copy first, else the one DAT game with
/// it), the file it names (xdelta), a game's file beside it of the same
/// name, its names (patch, zip, readme title) against your games' titles,
/// or a title of yours its readme names. Ambiguous stays unpaired: the patch is still offered in its
/// folder.
fn pair(conn: &Connection) -> Result<(), Error> {
    let rows = view::library(conn)?.rows;
    let mut game_of: std::collections::HashMap<i64, (i64, String)> = Default::default();
    let mut owned: std::collections::BTreeSet<(i64, String)> = Default::default();
    for r in &rows {
        if let (Some(sid), RowStatus::Verified | RowStatus::ByYou) = (r.system_id, r.status) {
            for id in r.file_ids() {
                game_of.insert(*id, (sid, r.title.clone()));
            }
            owned.insert((sid, r.title.clone()));
        }
    }
    let files: Vec<(i64, LibFile)> = conn
        .prepare("SELECT file_id, path, member, status, size, crc32, sha1, stripped_size, stripped_crc32 FROM files")?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                LibFile {
                    path: r.get(1)?,
                    member: r.get(2)?,
                    status: r.get(3)?,
                    size: r.get(4)?,
                    crc32: r.get(5)?,
                    sha1: r.get(6)?,
                    stripped: match (r.get::<_, Option<u64>>(7)?, r.get::<_, Option<String>>(8)?) {
                        (Some(s), Some(c)) => Some((s, c)),
                        _ => None,
                    },
                },
            ))
        })?
        .collect::<Result<_, _>>()?;
    let titles: Vec<String> = owned.iter().map(|(_, t)| t.clone()).collect();
    let mut dat_names: Option<(crate::titles::NameIndex, Vec<(i64, String)>)> = None;
    let regions = crate::settings::load(conn)?.region_order();
    let index = crate::titles::TitleIndex::new(titles.iter().cloned());
    let keys: Vec<((i64, String), crate::titles::TitleKey)> = owned
        .iter()
        .map(|g| (g.clone(), crate::titles::key(&g.1)))
        .collect();
    struct Found {
        path: String,
        name: String,
        source: Option<(u64, String)>,
        names_source: Option<String>,
        hint: String,
        readme: Option<String>,
    }
    let patches: Vec<Found> = conn
        .prepare("SELECT path, name, source_size, source_crc32, names_source, hint, readme FROM patch_files")?
        .query_map([], |r| {
            Ok(Found {
                path: r.get(0)?,
                name: r.get(1)?,
                source: match (r.get::<_, Option<u64>>(2)?, r.get::<_, Option<String>>(3)?) {
                    (Some(s), Some(c)) => Some((s, c)),
                    _ => None,
                },
                names_source: r.get(4)?,
                hint: r.get(5)?,
                readme: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let one = |set: std::collections::BTreeSet<(i64, String)>| {
        (set.len() == 1).then(|| set.into_iter().next()).flatten()
    };
    let mut dat_by_crc = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id WHERE r.size = ?1 AND r.crc32 = ?2 AND d.system_id IS NOT NULL",
    )?;
    let mut dat_by_name = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id WHERE r.name = ?1 COLLATE NOCASE AND d.system_id IS NOT NULL",
    )?;
    let mut set = conn.prepare_cached(
        "UPDATE patch_files SET system_id = ?2, title = ?3, paired_by = ?4 WHERE path = ?1",
    )?;
    for p in &patches {
        let dir = Path::new(&p.path).parent().map(Path::to_path_buf);
        let mut found: Option<((i64, String), &str)> = None;
        if let Some((size, crc)) = &p.source {
            let mine: std::collections::BTreeSet<_> = files
                .iter()
                .filter(|(_, f)| {
                    (f.size == *size && f.crc32 == *crc) || f.stripped.as_ref() == Some(&(*size, crc.clone()))
                })
                .filter_map(|(id, _)| game_of.get(id).cloned())
                .collect();
            let game = one(mine).or_else(|| {
                let dat: std::collections::BTreeSet<(i64, String)> = dat_by_crc
                    .query_map(params![size, crc], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map(|rows| rows.flatten().collect())
                    .unwrap_or_default();
                one(dat)
            });
            found = game.map(|g| (g, "source"));
        }
        if found.is_none()
            && let Some(source) = &p.names_source
        {
            let mine: std::collections::BTreeSet<_> = files
                .iter()
                .filter(|(_, f)| {
                    let base = if f.member.is_empty() { &f.path } else { &f.member };
                    base.rsplit(['/', '\\'])
                        .next()
                        .is_some_and(|b| b.eq_ignore_ascii_case(source))
                })
                .filter_map(|(id, _)| game_of.get(id).cloned())
                .collect();
            let game = one(mine).or_else(|| {
                let dat: std::collections::BTreeSet<(i64, String)> = dat_by_name
                    .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map(|rows| rows.flatten().collect())
                    .unwrap_or_default();
                one(dat)
            });
            found = game.map(|g| (g, "names"));
        }
        if found.is_none() {
            // A game's file beside it with the same name (the patch's, or
            // its zip's).
            let stems = [stem_lower(&p.name), stem_lower(&p.path)];
            let mine: std::collections::BTreeSet<_> = files
                .iter()
                .filter(|(_, f)| {
                    Path::new(&f.path).parent().map(Path::to_path_buf) == dir || f.path == p.path
                })
                .filter(|(_, f)| {
                    let name = if f.member.is_empty() { &f.path } else { &f.member };
                    stems.contains(&stem_lower(name))
                })
                .filter_map(|(id, _)| game_of.get(id).cloned())
                .collect();
            found = one(mine).map(|g| (g, "beside"));
        }
        if found.is_none() {
            for line in p.hint.lines() {
                let hits = index.find(line, 3);
                let Some((best, score)) = hits.first() else {
                    continue;
                };
                let tied = hits.get(1).is_some_and(|(_, s)| s >= score);
                if *score < 0.9 || tied {
                    continue;
                }
                let games: std::collections::BTreeSet<_> =
                    owned.iter().filter(|(_, t)| t == best).cloned().collect();
                if let Some(g) = one(games) {
                    found = Some((g, "title"));
                    break;
                }
            }
        }
        if found.is_none()
            && let Some(readme) = &p.readme
        {
            // A game's title named in its readme: the longest, if one.
            let text = crate::titles::text_words(readme);
            let mut best: (usize, std::collections::BTreeSet<(i64, String)>) = (0, Default::default());
            for (g, k) in &keys {
                let n = crate::titles::named_in(k, &text);
                if n > best.0 {
                    best = (n, Default::default());
                }
                if n > 0 && n == best.0 {
                    best.1.insert(g.clone());
                }
            }
            found = one(best.1).map(|g| (g, "readme"));
        }
        if found.is_none()
            && let Some(readme) = &p.readme
        {
            // A game you don't have: the DAT game its readme names.
            let (index, games) = dat_names.get_or_insert_with(|| dat_name_index(conn).unwrap_or_default());
            let text = crate::titles::text_words(readme);
            let hits: Vec<(i64, String)> = index.named(&text).into_iter().map(|i| games[i].clone()).collect();
            found = one_game(hits, readme, &regions).map(|g| (g, "readme"));
        }
        match found {
            Some(((sid, title), by)) => set.execute(params![p.path, sid, title, by])?,
            None => set.execute(params![p.path, None::<i64>, None::<String>, None::<String>])?,
        };
    }
    Ok(())
}

/// Every DAT game's name, indexed for [`crate::titles::NameIndex::named`],
/// with the games in its order.
fn dat_name_index(conn: &Connection) -> Result<(crate::titles::NameIndex, Vec<(i64, String)>), Error> {
    let all: Vec<(i64, String)> = conn
        .prepare(
            "SELECT DISTINCT d.system_id, g.name FROM games g JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id IS NOT NULL ORDER BY d.system_id, g.name",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let index = crate::titles::NameIndex::new(all.iter().map(|(_, t)| t.as_str()));
    Ok((index, all))
}

/// One game out of DAT games a readme names: versions of one title on one
/// platform are one game (its best release in your region order); several
/// platforms only when the readme names one of them.
fn one_game(hits: Vec<(i64, String)>, readme: &str, regions: &[String]) -> Option<(i64, String)> {
    let mut systems: Vec<i64> = hits.iter().map(|(s, _)| *s).collect();
    systems.sort_unstable();
    systems.dedup();
    if systems.len() > 1 {
        let text = readme.to_lowercase();
        let says = |words: &str| {
            let w = words.to_lowercase();
            w.len() >= 3 && text.contains(&w)
        };
        systems.retain(|id| {
            systems::by_id(*id)
                .is_some_and(|s| says(s.name) || says(s.key) || says(&crate::view::short_name(s.key)))
        });
    }
    let [sid] = systems.as_slice() else {
        return None;
    };
    let of: Vec<&String> = hits.iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect();
    let mut bases: Vec<String> = of.iter().map(|t| crate::organize::base_title(t)).collect();
    bases.sort();
    bases.dedup();
    if bases.len() != 1 {
        return None;
    }
    of.into_iter()
        .max_by_key(|t| (crate::organize::rank(t, regions), std::cmp::Reverse((*t).clone())))
        .map(|t| (*sid, t.clone()))
}
