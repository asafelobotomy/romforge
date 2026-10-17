//! The library as the app shows it: one row per game, unidentified files as
//! their own rows, and what has been gathered for each (milestone 6).
//!
//! A row is a game you have complete (house rule 8: every data ROM matched
//! by size + SHA1/MD5), keyed by system and DAT name so the same title in
//! two DATs is one row. Every other file is a row of its own: incomplete,
//! candidate, wrong system or unknown. Members of an archive that holds a
//! game (a cue that differs, a readme) stay with that game's row.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::report::{GAME_HAVE, HAD, IS_HAVE};
use crate::{Error, fetch, systems};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    /// Every data ROM matched by hash.
    Verified,
    /// Matched by hash, but the game isn't complete.
    Incomplete,
    /// Only size + CRC32 agree.
    Candidate,
    /// Matched a game of another system than the folder's.
    WrongSystem,
    /// A copy of something another file already provides.
    Duplicate,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameRow {
    /// `g:<system id>:<title>` for games, `f:<file id>` for lone files.
    pub id: String,
    pub title: String,
    pub system: Option<String>,
    pub system_name: Option<String>,
    pub status: RowStatus,
    pub cue_differs: bool,
    pub files: usize,
    pub size: u64,
    pub metadata: bool,
    /// Art kinds cached for the game (`boxart`, `title`, `snap`).
    pub art: Vec<String>,
    #[serde(skip)]
    system_id: Option<i64>,
    #[serde(skip)]
    file_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Platform {
    /// `None` for files no system could be found for.
    pub system: Option<String>,
    pub name: String,
    /// A label of up to 5 characters for the platform's icon.
    pub short: String,
    pub manufacturer: Option<String>,
    pub games: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryView {
    pub platforms: Vec<Platform>,
    pub rows: Vec<GameRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowFile {
    /// `path` or `path#member`.
    pub path: String,
    pub status: String,
    pub size: u64,
    pub sha1: String,
    pub header_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameDetail {
    pub row: GameRow,
    pub fields: BTreeMap<String, String>,
    /// kind -> cached file
    pub art: BTreeMap<String, String>,
    pub files: Vec<RowFile>,
    /// DATs listing the game.
    pub dats: Vec<String>,
    pub links: Vec<Link>,
    /// Games of the same franchise romforge has metadata for, oldest first.
    pub series: Vec<SeriesEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeriesEntry {
    pub title: String,
    pub system: String,
    pub year: Option<String>,
    /// The game being shown.
    pub current: bool,
}

struct FileRec {
    id: i64,
    path: String,
    member: String,
    status: String,
    size: u64,
}

pub fn library(conn: &Connection) -> Result<LibraryView, Error> {
    let files: Vec<FileRec> = conn
        .prepare("SELECT file_id, path, member, status, size FROM files ORDER BY path, member")?
        .query_map([], |r| {
            Ok(FileRec {
                id: r.get(0)?,
                path: r.get(1)?,
                member: r.get(2)?,
                status: r.get(3)?,
                size: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    // Complete games and the files that provide them.
    let mut games: BTreeMap<(i64, String), (bool, BTreeSet<i64>)> = BTreeMap::new();
    let mut stmt = conn.prepare(&format!(
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
         JOIN matches m ON m.rom_id = r.rom_id AND m.out_of_scope = 0 AND m.via IN ('sha1', 'md5')
         JOIN files f ON f.file_id = m.file_id AND f.status IN ('matched', 'duplicate')
         WHERE d.system_id IS NOT NULL"
    ))?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
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
    let mut owner: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    for (key, (_, ids)) in &games {
        for f in ids {
            owner.entry(*f).or_default().push(key.clone());
        }
    }
    // Members of an archive holding a game stay with that game.
    let mut by_path: HashMap<&str, Vec<(i64, String)>> = HashMap::new();
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

    // What lone files were matched as, for their row title and system.
    let mut seen_as: HashMap<i64, BTreeSet<(Option<i64>, String)>> = HashMap::new();
    for row in conn
        .prepare(
            "SELECT m.file_id, d.system_id, g.name FROM matches m
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id",
        )?
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
    let game_row = |system_id: i64, title: &str| -> GameRow {
        let sys = systems::by_id(system_id);
        let key = (system_id, title.to_owned());
        GameRow {
            id: format!("g:{system_id}:{title}"),
            title: title.to_owned(),
            system: sys.map(|s| s.key.to_owned()),
            system_name: sys.map(|s| s.name.to_owned()),
            status: RowStatus::Verified,
            cue_differs: games.get(&key).is_some_and(|g| g.0),
            files: 0,
            size: 0,
            metadata: meta.contains(&key),
            art: art.get(&key).cloned().unwrap_or_default(),
            system_id: Some(system_id),
            file_ids: Vec::new(),
        }
    };
    for f in &files {
        let homes = owner.get(&f.id).or_else(|| {
            (!f.member.is_empty())
                .then(|| by_path.get(f.path.as_str()))
                .flatten()
        });
        if let Some(homes) = homes {
            for (system_id, title) in homes {
                let row = rows
                    .entry(format!("g:{system_id}:{title}"))
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
            let status = match f.status.as_str() {
                "matched" => RowStatus::Incomplete,
                "candidate" => RowStatus::Candidate,
                "wrong_system" => RowStatus::WrongSystem,
                "duplicate" => RowStatus::Duplicate,
                _ => RowStatus::Unknown,
            };
            let sys = system_id.and_then(systems::by_id);
            let key = system_id.map(|s| (s, title.clone()));
            rows.entry(format!("f:{}", f.id)).or_insert(GameRow {
                id: format!("f:{}", f.id),
                title,
                system: sys.map(|s| s.key.to_owned()),
                system_name: sys.map(|s| s.name.to_owned()),
                status,
                cue_differs: false,
                files: 0,
                size: 0,
                metadata: key.as_ref().is_some_and(|k| meta.contains(k)),
                art: key.and_then(|k| art.get(&k).cloned()).unwrap_or_default(),
                system_id,
                file_ids: Vec::new(),
            })
        };
        row.files += 1;
        row.size += f.size;
        row.file_ids.push(f.id);
    }

    let mut rows: Vec<GameRow> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        sort_key(&a.title)
            .cmp(&sort_key(&b.title))
            .then_with(|| a.system.cmp(&b.system))
    });
    let mut counts: BTreeMap<Option<i64>, usize> = BTreeMap::new();
    for r in &rows {
        *counts.entry(r.system_id).or_default() += 1;
    }
    let mut platforms: Vec<Platform> = counts
        .into_iter()
        .map(|(id, games)| {
            let sys = id.and_then(systems::by_id);
            Platform {
                system: sys.map(|s| s.key.to_owned()),
                short: sys.map_or("?".to_owned(), |s| short_name(s.key)),
                name: sys.map_or("Unidentified".to_owned(), |s| s.name.to_owned()),
                manufacturer: sys.map(|s| s.manufacturer.to_owned()),
                games,
            }
        })
        .collect();
    platforms.sort_by(|a, b| (a.system.is_none(), &a.name).cmp(&(b.system.is_none(), &b.name)));
    Ok(LibraryView { platforms, rows })
}

/// An icon label from a system key: short keys as they are ("SNES",
/// "Lynx"), longer ones as their capitals and digits ("Game Boy Advance" ->
/// "GBA", "PlayStation 2" -> "PS2", "Commodore 64" -> "C64").
pub fn short_name(key: &str) -> String {
    if key.chars().count() <= 5 {
        return key.to_owned();
    }
    let s: String = key
        .chars()
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .collect();
    if s.chars().count() >= 2 {
        s.chars().take(5).collect()
    } else {
        key.chars()
            .filter(|c| c.is_alphanumeric())
            .take(3)
            .collect::<String>()
            .to_uppercase()
    }
}

/// Sort "Legend of Zelda, The - ..." under L, ignoring case.
fn sort_key(title: &str) -> String {
    title.to_lowercase()
}

/// The title people search for: tags removed, "Zelda, The" as "The Zelda".
pub fn search_title(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    // "Legend of Zelda, The - A Link to the Past" -> "The Legend of Zelda - A Link to the Past"
    let (head, tail) = match t.split_once(" - ") {
        Some((h, rest)) => (h.to_owned(), format!(" - {rest}")),
        None => (t.clone(), String::new()),
    };
    for article in ["The", "A", "An"] {
        if let Some(stem) = head.strip_suffix(&format!(", {article}")) {
            return format!("{article} {stem}{tail}");
        }
    }
    t
}

/// Plain search links (house rule 10, tier 1: no account, no key).
pub fn links(title: &str, system_name: Option<&str>) -> Vec<Link> {
    let q = search_title(title);
    let enc = |s: &str| fetch::encode_segment(s);
    let wiki_q = match system_name {
        Some(_) => format!("{q} video game"),
        None => q.clone(),
    };
    vec![
        Link {
            label: "MobyGames".into(),
            url: format!("https://www.mobygames.com/search/?q={}", enc(&q)),
        },
        Link {
            label: "RetroAchievements".into(),
            url: format!("https://retroachievements.org/searchresults.php?s={}", enc(&q)),
        },
        Link {
            label: "Wikipedia".into(),
            url: format!("https://en.wikipedia.org/w/index.php?search={}", enc(&wiki_q)),
        },
        Link {
            label: "IGDB".into(),
            url: format!("https://www.igdb.com/search?q={}", enc(&q)),
        },
        Link {
            label: "HowLongToBeat".into(),
            url: format!("https://howlongtobeat.com/?q={}", enc(&q)),
        },
    ]
}

/// Everything known about one row of [`library`].
pub fn detail(conn: &Connection, id: &str) -> Result<GameDetail, Error> {
    let view = library(conn)?;
    let row = view
        .rows
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| Error::Dat(format!("no library row {id:?}")))?;
    let mut fields = BTreeMap::new();
    let mut art = BTreeMap::new();
    let mut dats = Vec::new();
    if let Some(system_id) = row.system_id {
        for r in conn
            .prepare("SELECT field, value FROM game_meta WHERE system_id = ?1 AND title = ?2")?
            .query_map(params![system_id, row.title], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
        {
            let (k, v) = r?;
            fields.insert(k, v);
        }
        for r in conn
            .prepare("SELECT kind, path FROM art WHERE system_id = ?1 AND title = ?2 AND path IS NOT NULL")?
            .query_map(params![system_id, row.title], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
        {
            let (k, p) = r?;
            art.insert(k, p);
        }
        dats = conn
            .prepare("SELECT DISTINCT d.name FROM games g JOIN dats d ON d.dat_id = g.dat_id WHERE g.name = ?1 AND d.system_id = ?2 ORDER BY d.name")?
            .query_map(params![row.title, system_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
    }
    let mut stmt =
        conn.prepare("SELECT path, member, status, size, sha1, header_kind FROM files WHERE file_id = ?1")?;
    let mut files = Vec::new();
    for id in &row.file_ids {
        let f = stmt.query_row([id], |r| {
            let path: String = r.get(0)?;
            let member: String = r.get(1)?;
            Ok(RowFile {
                path: if member.is_empty() {
                    path
                } else {
                    format!("{path}#{member}")
                },
                status: r.get(2)?,
                size: r.get(3)?,
                sha1: r.get(4)?,
                header_kind: r.get(5)?,
            })
        })?;
        files.push(f);
    }
    let links = if row.system_id.is_some() && row.status != RowStatus::Unknown {
        links(&row.title, row.system_name.as_deref())
    } else {
        Vec::new()
    };
    let mut series = Vec::new();
    if let Some(franchise) = fields.get("franchise") {
        let mut stmt = conn.prepare(
            "SELECT f.system_id, f.title,
                    (SELECT value FROM game_meta y WHERE y.system_id = f.system_id
                       AND y.title = f.title AND y.field = 'releaseyear')
             FROM game_meta f WHERE f.field = 'franchise' AND f.value = ?1",
        )?;
        for r in stmt.query_map([franchise], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })? {
            let (system_id, title, year) = r?;
            series.push(SeriesEntry {
                current: Some(system_id) == row.system_id && title == row.title,
                system: systems::by_id(system_id).map_or(String::new(), |s| s.key.to_owned()),
                title,
                year,
            });
        }
        series.sort_by(|a, b| {
            (a.year.is_none(), &a.year, &a.title).cmp(&(b.year.is_none(), &b.year, &b.title))
        });
    }
    Ok(GameDetail {
        row,
        fields,
        art,
        files,
        dats,
        links,
        series,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_titles() {
        assert_eq!(
            search_title("Legend of Zelda, The - A Link to the Past (USA)"),
            "The Legend of Zelda - A Link to the Past"
        );
        assert_eq!(search_title("Super Mario World (USA) [!]"), "Super Mario World");
        assert_eq!(
            search_title("Revenge of Shinobi, The (USA, Europe)"),
            "The Revenge of Shinobi"
        );
    }

    #[test]
    fn short_names() {
        assert_eq!(short_name("SNES"), "SNES");
        assert_eq!(short_name("Game Boy Advance"), "GBA");
        assert_eq!(short_name("PlayStation 2"), "PS2");
        assert_eq!(short_name("PlayStation"), "PS");
        assert_eq!(short_name("Commodore 64"), "C64");
        assert_eq!(short_name("Genesis"), "GEN");
    }

    #[test]
    fn links_are_search_urls() {
        let l = links(
            "Super Mario World (USA)",
            Some("Super Nintendo Entertainment System"),
        );
        assert_eq!(
            l[0].url,
            "https://www.mobygames.com/search/?q=Super%20Mario%20World"
        );
        assert!(l.iter().any(|x| x.label == "RetroAchievements"));
    }
}
