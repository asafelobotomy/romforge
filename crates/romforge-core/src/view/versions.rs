//! Grouping a game's versions, and the category of each.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension, params};

use super::patches::bare_row;
use super::{Category, GameRow, Relation, RowStatus};
use crate::Error;

/// Group the versions of each game: regions, revisions, translations and
/// hacks of one game share a group, keyed by its clone-list group, else
/// its DAT parent (`cloneof`), else its title without tags; translations
/// and hacks by their base game. The main row is the best release in the
/// Settings region order (a translation or hack only when nothing else is
/// there). Unidentified files are never grouped.
///
/// For a `listing`, patch rows join their game's group too, a game's
/// original is its main row (added as [`RowStatus::Missing`] when only
/// translations, hacks or patches of it are here), and every other version
/// gets its [`Category`] and the row it is made from.
pub(super) fn group_versions(conn: &Connection, rows: &mut Vec<GameRow>, listing: bool) -> Result<(), Error> {
    use crate::organize::{base_title, rank};
    let regions = crate::settings::load(conn)?.region_order();
    let clones = crate::organize::clone_groups(conn)?;
    let bases: HashMap<(i64, String), (String, Option<Relation>)> = conn
        .prepare("SELECT DISTINCT system_id, title, base, relation FROM manual_ids WHERE base IS NOT NULL")?
        .query_map([], |r| {
            Ok((
                (r.get(0)?, r.get(1)?),
                (
                    r.get(2)?,
                    r.get::<_, Option<String>>(3)?
                        .as_deref()
                        .and_then(Relation::parse),
                ),
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut parent_of = conn.prepare(
        "SELECT g.cloneof FROM games g JOIN dats d ON d.dat_id = g.dat_id
         WHERE g.name = ?2 AND d.system_id = ?1 AND g.cloneof IS NOT NULL AND g.cloneof <> ''
         LIMIT 1",
    )?;
    // What a translation, hack or patch row is made from.
    let base_of = |r: &GameRow| -> Option<(String, Relation)> {
        if r.status == RowStatus::Patch {
            return r.patch_of.clone();
        }
        let (base, rel) = bases.get(&(r.system_id?, r.title.clone()))?;
        (r.status == RowStatus::Derivative).then(|| (base.clone(), rel.unwrap_or(Relation::Hack)))
    };
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut priority: HashMap<usize, i64> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let identified = matches!(
            r.status,
            RowStatus::Verified | RowStatus::Incomplete | RowStatus::ByYou | RowStatus::Derivative
        ) || (listing && r.status == RowStatus::Patch);
        let (Some(sid), Some(key)) = (r.system_id, r.system.as_deref()) else {
            continue;
        };
        if !identified {
            continue;
        }
        let name = base_of(r).map(|(b, _)| b).unwrap_or_else(|| r.title.clone());
        let parent: Option<String> = parent_of.query_row(params![sid, name], |r| r.get(0)).optional()?;
        let base = base_title(parent.as_deref().unwrap_or(&name));
        let group = match clones.get(&(key.to_owned(), base.clone())) {
            Some((grp, p)) => {
                priority.insert(i, *p);
                format!("v:{sid}:c:{}", grp.to_lowercase())
            }
            None => format!("v:{sid}:t:{base}"),
        };
        groups.entry(group).or_default().push(i);
    }
    let original = |r: &GameRow| {
        matches!(
            r.status,
            RowStatus::Verified | RowStatus::Incomplete | RowStatus::ByYou
        )
    };
    for (group, mut idx) in groups {
        // Only translations, hacks or patches: their original goes on top.
        if listing && !idx.iter().any(|&i| original(&rows[i])) {
            let first = &rows[idx[0]];
            let sid = first.system_id;
            let title = idx
                .iter()
                .filter_map(|&i| base_of(&rows[i]).map(|(b, _)| b))
                .max_by_key(|t| {
                    let (retail, region, revision) = rank(t, &regions);
                    (retail, region, revision, std::cmp::Reverse(t.clone()))
                })
                .unwrap_or_else(|| first.title.clone());
            let id = format!("x:{}:{title}", sid.map_or("-".to_owned(), |s| s.to_string()));
            rows.push(bare_row(id, sid, &title, RowStatus::Missing));
            idx.push(rows.len() - 1);
        }
        if idx.len() < 2 {
            continue;
        }
        let best = *idx
            .iter()
            .max_by_key(|&&i| {
                let r = &rows[i];
                let (retail, region, revision) = rank(&r.title, &regions);
                (
                    r.status == RowStatus::Missing,
                    r.status != RowStatus::Patch,
                    r.status != RowStatus::Derivative,
                    r.status != RowStatus::Incomplete,
                    retail,
                    region,
                    -priority.get(&i).copied().unwrap_or(1),
                    revision,
                    std::cmp::Reverse(r.title.clone()),
                )
            })
            .unwrap_or(&idx[0]);
        for &i in &idx {
            let category = (listing && i != best).then(|| match base_of(&rows[i]) {
                Some((_, rel)) => match (rows[i].status, rel) {
                    (RowStatus::Patch, Relation::Translation) => Category::PatchTranslation,
                    (RowStatus::Patch, Relation::Hack) => Category::PatchHack,
                    (_, Relation::Translation) => Category::Translation,
                    (_, Relation::Hack) => Category::Hack,
                },
                None if is_edition(&rows[i].title) => Category::Edition,
                None => Category::Region,
            });
            let r = &mut rows[i];
            r.group = Some(group.clone());
            r.main = i == best;
            r.versions = if i == best { idx.len() } else { 1 };
            r.category = category;
        }
    }
    if listing {
        // The row each translation, hack and patch is made from.
        let owned: HashMap<(Option<i64>, String), String> = rows
            .iter()
            .filter(|r| matches!(r.status, RowStatus::Verified | RowStatus::ByYou))
            .map(|r| ((r.system_id, r.title.clone()), r.id.clone()))
            .collect();
        let ids: Vec<Option<String>> = rows
            .iter()
            .map(|r| base_of(r).and_then(|(b, _)| owned.get(&(r.system_id, b)).cloned()))
            .collect();
        for (r, id) in rows.iter_mut().zip(ids) {
            r.base_id = id;
        }
    }
    Ok(())
}

/// Words that make a release a special edition or re-release.
const EDITION_WORDS: &[&str] = &[
    "edition",
    "collector",
    "limited",
    "deluxe",
    "virtual console",
    "switch online",
    "greatest hits",
    "platinum",
    "player's choice",
    "players choice",
    "the best",
    "classics",
    "reprint",
];

/// A special edition, re-release, or not a retail release, by its name.
fn is_edition(title: &str) -> bool {
    let n = crate::names::parse(title);
    let t = title.to_lowercase();
    n.status.is_some() || EDITION_WORDS.iter().any(|w| t.contains(w))
}
