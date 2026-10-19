use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::read::read_patch;
use crate::Error;
use crate::patch::{self, Kind};
use crate::view::{self, Relation};

/// A patch for a game: one a scan found and paired with it, or one lying
/// beside its files.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FoundPatch {
    /// The patch file, or the zip holding it.
    pub path: PathBuf,
    /// The patch's own name.
    pub name: String,
    pub kind: String,
    /// What its name and readme say it is.
    pub relation: Relation,
    /// How romforge knows it is for this game: "source" (the ROM it was
    /// made from, by size and CRC32), "names" (the file an xdelta names),
    /// "beside" (next to the game's file, of the same name), "title" (its
    /// name, or its readme's title line, is the game's), "readme" (its
    /// readme names the game), "folder" (in a folder of the game, not
    /// paired with any).
    pub paired_by: String,
}

/// The patches for a game: those a scan paired with it, then the others in
/// its folders that no scan paired with any game.
pub fn beside(conn: &Connection, row_id: &str) -> Result<Vec<FoundPatch>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut out = paired_with(conn, d.row.system_id, &d.row.title)?;
    let mut loose: Vec<PathBuf> = Vec::new();
    for f in &d.files {
        let path = f.path.split_once('#').map_or(f.path.as_str(), |(p, _)| p);
        let Some(dir) = Path::new(path).parent() else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if p.is_file() && patch::is_patch_name(&name) && !loose.contains(&p) {
                loose.push(p);
            }
        }
    }
    loose.sort();
    let mut known = conn.prepare_cached("SELECT title IS NOT NULL FROM patch_files WHERE path = ?1")?;
    for p in loose {
        use rusqlite::OptionalExtension;
        if out.iter().any(|x| x.path == p) {
            continue;
        }
        let paired: Option<bool> = known.query_row([p.to_string_lossy()], |r| r.get(0)).optional()?;
        if paired == Some(true) {
            continue; // another game's
        }
        let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let kind = std::fs::read(&p)
            .ok()
            .and_then(|d| patch::info(&d))
            .map_or("?", |i| i.kind.name());
        out.push(FoundPatch {
            relation: guess_relation(&name, ""),
            kind: kind.to_owned(),
            path: p,
            name,
            paired_by: "folder".into(),
        });
    }
    Ok(out)
}

/// The patches a scan paired with a game.
pub(crate) fn paired_with(
    conn: &Connection,
    system_id: Option<i64>,
    title: &str,
) -> Result<Vec<FoundPatch>, Error> {
    let Some(system_id) = system_id else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare_cached(
        "SELECT path, name, kind, relation, paired_by FROM patch_files
         WHERE system_id = ?1 AND title = ?2 ORDER BY name, path",
    )?;
    let rows = stmt.query_map(params![system_id, title], |r| {
        Ok(FoundPatch {
            path: PathBuf::from(r.get::<_, String>(0)?),
            name: r.get(1)?,
            kind: r.get(2)?,
            relation: Relation::parse(&r.get::<_, String>(3)?).unwrap_or(Relation::Hack),
            paired_by: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The patch file a scan found at `path`.
pub(crate) fn patch_at(conn: &Connection, path: &str) -> Result<Option<FoundPatch>, Error> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT path, name, kind, relation, coalesce(paired_by, 'folder') FROM patch_files WHERE path = ?1",
            [path],
            |r| {
                Ok(FoundPatch {
                    path: PathBuf::from(r.get::<_, String>(0)?),
                    name: r.get(1)?,
                    kind: r.get(2)?,
                    relation: Relation::parse(&r.get::<_, String>(3)?).unwrap_or(Relation::Hack),
                    paired_by: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// Translation or hack, from a patch's name and readme. Names say it
/// loosely ("English", "Eng"), readmes strictly ("translation").
pub(crate) fn guess_relation(name: &str, readme: &str) -> Relation {
    let n = name.to_lowercase();
    let r = readme.to_lowercase();
    let words: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).collect();
    let by_name = [
        "english",
        "eng",
        "translation",
        "translated",
        "trad",
        "traduccion",
        "traducao",
    ]
    .iter()
    .any(|w| words.contains(w))
        || n.contains("translat")
        || n.contains("[t-")
        || n.contains("(t-")
        || n.contains("[t+");
    let by_readme = [
        "translation",
        "translated",
        "traducción",
        "tradução",
        "traduction",
        "übersetzung",
    ]
    .iter()
    .any(|w| r.contains(w));
    if by_name || by_readme {
        Relation::Translation
    } else {
        Relation::Hack
    }
}

pub(super) fn stem_lower(name: &str) -> String {
    let n = name.rsplit(['/', '\\']).next().unwrap_or(name);
    n.rsplit_once('.').map_or(n, |(s, _)| s).to_lowercase()
}

fn mtime_of(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?.modified().ok()?;
    let d = m.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_nanos()).ok()
}

/// Remember the patch files a scan of root `root_id` found (`found`: a
/// patch file, or a zip with a patch in it), reading only new or changed
/// ones; forget this root's others, except those in `kept` (files the scan
/// saw unchanged).
pub(crate) fn record(
    conn: &Connection,
    root_id: i64,
    found: &std::collections::BTreeSet<PathBuf>,
    kept: &std::collections::HashSet<String>,
) -> Result<(), Error> {
    use rusqlite::OptionalExtension;
    let mut known = conn.prepare_cached("SELECT size, mtime FROM patch_files WHERE path = ?1")?;
    for path in found {
        let key = path.to_string_lossy();
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let mtime = mtime_of(path);
        let had: Option<(u64, Option<i64>)> = known
            .query_row([&key], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if had == Some((size, mtime)) && mtime.is_some() {
            conn.execute(
                "UPDATE patch_files SET root_id = ?2 WHERE path = ?1",
                params![key, root_id],
            )?;
            continue;
        }
        // A zip of several patches, or a damaged one: not offered.
        let Ok(p) = read_patch(path) else {
            conn.execute("DELETE FROM patch_files WHERE path = ?1", [&key])?;
            continue;
        };
        let info = patch::info(&p.data);
        let readme = p
            .readme
            .as_ref()
            .map(|(_, d)| String::from_utf8_lossy(&d[..d.len().min(8192)]).into_owned())
            .unwrap_or_default();
        let readme_title = readme
            .lines()
            .map(|l| l.trim_matches(|c: char| c.is_whitespace() || "=-*#~_|".contains(c)))
            .find(|l| l.chars().filter(|c| c.is_alphanumeric()).count() >= 3)
            .map(|l| l.chars().take(120).collect::<String>())
            .unwrap_or_default();
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut hint = vec![stem_lower(&p.name)];
        if stem_lower(&file_name) != hint[0] {
            hint.push(stem_lower(&file_name));
        }
        if !readme_title.is_empty() {
            hint.push(readme_title);
        }
        let names_source = (p.kind == Kind::Xdelta)
            .then(|| crate::vcdiff::names(&p.data))
            .flatten()
            .map(|(_, s)| s);
        conn.execute(
            "INSERT INTO patch_files (path, root_id, name, kind, sha1, size, mtime, source_size, source_crc32,
                                      target_size, target_crc32, names_source, hint, relation, readme)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(path) DO UPDATE SET root_id = excluded.root_id, name = excluded.name,
               kind = excluded.kind, sha1 = excluded.sha1, size = excluded.size, mtime = excluded.mtime,
               source_size = excluded.source_size, source_crc32 = excluded.source_crc32,
               target_size = excluded.target_size, target_crc32 = excluded.target_crc32,
               names_source = excluded.names_source, hint = excluded.hint, relation = excluded.relation,
               readme = excluded.readme, system_id = NULL, title = NULL, paired_by = NULL",
            params![
                key,
                root_id,
                p.name,
                p.kind.name(),
                p.sha1,
                size,
                mtime,
                info.and_then(|i| i.source).map(|s| s.0),
                info.and_then(|i| i.source).map(|s| format!("{:08x}", s.1)),
                info.and_then(|i| i.target).map(|s| s.0),
                info.and_then(|i| i.target).map(|s| format!("{:08x}", s.1)),
                names_source,
                hint.join("\n"),
                guess_relation(&format!("{} {file_name}", p.name), &readme).as_str(),
                (!readme.is_empty()).then(|| readme.chars().take(4000).collect::<String>()),
            ],
        )?;
    }
    let ours: Vec<String> = conn
        .prepare("SELECT path FROM patch_files WHERE root_id = ?1")?
        .query_map([root_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for path in ours {
        if !found.contains(Path::new(&path)) && !kept.contains(&path) {
            conn.execute("DELETE FROM patch_files WHERE path = ?1", [&path])?;
        }
    }
    Ok(())
}
