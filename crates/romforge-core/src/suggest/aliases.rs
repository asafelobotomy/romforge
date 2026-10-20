//! Other names a game goes by, from the LaunchBox database the owner
//! downloaded: "Takeshi's Challenge" is a name of "Takeshi no Chousenjou",
//! the DAT's. House rule 10: it fills only what the open sources leave
//! empty, so it is asked only when no DAT title is alike the file's name;
//! each name of the game it finds is then looked for among the DAT titles.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, params};

use crate::{Error, systems, titles};

/// How alike a name must be to count (every word of one in the other).
const ALIKE: f32 = 0.85;

pub(super) struct Aliases {
    db: Connection,
    /// Platforms -> their games' names, indexed, and name -> games.
    by_platforms: BTreeMap<Vec<&'static str>, Indexed>,
}

/// Names, indexed, and name -> the games it is a name of.
type Indexed = (titles::TitleIndex, HashMap<String, Vec<i64>>);

impl Aliases {
    /// The LaunchBox database in `dir`, if the owner downloaded it.
    pub(super) fn open(dir: &Path) -> Option<Aliases> {
        Some(Aliases {
            db: crate::extdb::open(crate::extdb::ExtDb::LaunchBox, dir)?,
            by_platforms: BTreeMap::new(),
        })
    }

    /// The LaunchBox games of `systems` one of whose names is alike `text`:
    /// (that name, every name of the game).
    pub(super) fn names_for(
        &mut self,
        systems: &[i64],
        text: &str,
    ) -> Result<Vec<(String, Vec<String>)>, Error> {
        let platforms: Vec<&'static str> = systems
            .iter()
            .filter_map(|s| systems::by_id(*s))
            .flat_map(|s| s.launchbox.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if platforms.is_empty() {
            return Ok(Vec::new());
        }
        if !self.by_platforms.contains_key(&platforms) {
            let json = serde_json::to_string(&platforms).unwrap_or_default();
            let mut games: HashMap<String, Vec<i64>> = HashMap::new();
            for r in self
                .db
                .prepare(
                    "SELECT n.key, n.id FROM names n JOIN games g ON g.id = n.id
                     WHERE g.platform IN (SELECT value FROM json_each(?1))",
                )?
                .query_map([&json], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            {
                let (key, id) = r?;
                games.entry(key).or_default().push(id);
            }
            let index = titles::TitleIndex::new(games.keys().cloned());
            self.by_platforms.insert(platforms.clone(), (index, games));
        }
        let (index, games) = &self.by_platforms[&platforms];
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for (name, score) in index.find(text, 5) {
            if score < ALIKE {
                continue;
            }
            for id in games.get(&name).into_iter().flatten() {
                if !seen.insert(*id) {
                    continue;
                }
                let all: Vec<String> = self
                    .db
                    .prepare_cached(
                        "SELECT key FROM names WHERE id = ?1 UNION SELECT name FROM games WHERE id = ?1",
                    )?
                    .query_map(params![id], |r| r.get(0))?
                    .collect::<Result<_, _>>()?;
                out.push((name.clone(), all));
            }
        }
        Ok(out)
    }
}

/// DAT titles a LaunchBox name of the game leads to, each with the name
/// alike the file's: only when no DAT title is alike the text itself.
pub(super) fn dat_titles(
    aliases: &mut Aliases,
    systems: &[i64],
    text: &str,
    names: &BTreeMap<String, Vec<i64>>,
    index: &titles::TitleIndex,
) -> Result<BTreeMap<(i64, String), String>, Error> {
    let mut out = BTreeMap::new();
    if index.find(text, 1).first().is_some_and(|(_, s)| *s >= ALIKE) {
        return Ok(out);
    }
    for (alike, all) in aliases.names_for(systems, text)? {
        for n in &all {
            for (title, score) in index.find(n, 5) {
                if score < ALIKE {
                    continue;
                }
                for sid in &names[&title] {
                    if systems.is_empty() || systems.contains(sid) {
                        out.entry((*sid, title.clone())).or_insert_with(|| alike.clone());
                    }
                }
            }
        }
    }
    Ok(out)
}
