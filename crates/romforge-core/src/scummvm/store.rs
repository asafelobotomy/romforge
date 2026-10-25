use super::*;

/// Keep what ScummVM's --detect named. Returns how many folders.
pub(crate) fn remember(conn: &Connection, found: &[(PathBuf, String, String)]) -> Result<usize, Error> {
    let mut n = 0;
    // ScummVM lists a folder once per variant that fits (VGA and EGA):
    // its first is the one it would start.
    let mut seen = std::collections::BTreeSet::new();
    for (dir, code, title) in found.iter().filter(|(d, _, _)| seen.insert(d.clone())) {
        let Some(fp) = fingerprint(dir) else { continue };
        conn.execute(
            "INSERT INTO scummvm_ids (path, fingerprint, code, title) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET fingerprint = excluded.fingerprint, code = excluded.code,
               title = excluded.title",
            params![dir.to_string_lossy(), fp, code, title],
        )?;
        n += 1;
    }
    Ok(n)
}

/// The scanned folders, without those inside another.
pub(crate) fn top_roots(conn: &Connection) -> Result<Vec<PathBuf>, Error> {
    let roots: Vec<PathBuf> = conn
        .prepare("SELECT path FROM scan_roots ORDER BY length(path), path")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let mut top: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !top.iter().any(|t| r.starts_with(t)) && r.is_dir() {
            top.push(r);
        }
    }
    Ok(top)
}

/// Folders ScummVM's --detect named before, where they are now: at the
/// same place with the same files, or (copied by import, which writes a
/// `.scummvm` file) wherever a folder with that fingerprint has one.
fn asked_before(
    conn: &Connection,
    roots: &[PathBuf],
    named: &[PathBuf],
) -> Result<Vec<(PathBuf, String, String)>, Error> {
    let rows: Vec<(String, String, String, String)> = conn
        .prepare("SELECT path, fingerprint, code, title FROM scummvm_ids")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let free = |d: &Path| !named.iter().any(|n| d.starts_with(n));
    let mut out: Vec<(PathBuf, String, String)> = Vec::new();
    for (path, fp, code, title) in &rows {
        let p = PathBuf::from(path);
        if p.is_dir() && free(&p) && fingerprint(&p).as_ref() == Some(fp) {
            out.push((p, code.clone(), title.clone()));
        }
    }
    let by_fp: HashMap<&str, (&str, &str)> = rows
        .iter()
        .map(|(_, f, c, t)| (f.as_str(), (c.as_str(), t.as_str())))
        .collect();
    let mut with_id: Vec<PathBuf> = Vec::new();
    for root in roots {
        for e in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if e.file_type().is_file()
                && e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with(".scummvm")
                && let Some(d) = e.path().parent()
                && !with_id.iter().any(|w| w == d)
            {
                with_id.push(d.to_owned());
            }
        }
    }
    for d in with_id {
        if !free(&d) || out.iter().any(|(p, _, _)| *p == d) {
            continue;
        }
        if let Some(fp) = fingerprint(&d)
            && let Some((code, title)) = by_fp.get(fp.as_str())
        {
            out.push((d.clone(), (*code).to_owned(), (*title).to_owned()));
            conn.execute(
                "INSERT OR REPLACE INTO scummvm_ids (path, fingerprint, code, title) VALUES (?1, ?2, ?3, ?4)",
                params![d.to_string_lossy(), fp, code, title],
            )?;
        }
    }
    Ok(out)
}

/// A file parsed once and kept while its size and modification time stay
/// the same: the list is parsed after every scan, and it is 4 MB.
type Cached<T> = Mutex<Option<(PathBuf, crate::scan::Stamp, Arc<T>)>>;

fn cached<T>(slot: &Cached<T>, path: &Path, parse: fn(&str) -> Result<T, Error>) -> Result<Arc<T>, Error> {
    let stamp = crate::scan::Stamp::of(path);
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some((p, s, v)), Some(now)) = (slot.as_ref(), stamp)
        && p == path
        && *s == now
    {
        return Ok(v.clone());
    }
    let v = Arc::new(parse(&String::from_utf8_lossy(&std::fs::read(path)?))?);
    *slot = stamp.map(|s| (path.to_owned(), s, v.clone()));
    Ok(v)
}

static LISTS: Cached<Vec<Entry>> = Mutex::new(None);

/// ScummVM's list in `list` (parsed once while the file stays the same);
/// empty when there is none.
pub(crate) fn load_list(list: Option<&Path>) -> Result<Arc<Vec<Entry>>, Error> {
    match list.filter(|p| p.is_file()) {
        Some(p) => cached(&LISTS, p, parse),
        None => Ok(Arc::default()),
    }
}

/// The ScummVM games under `root`, by the list or by what --detect named
/// before, with how many files each has: a scan passes over them whole
/// (their files are the game, named by the list, never hashed one by one).
pub(crate) fn games_under(
    conn: &Connection,
    list: Option<&Path>,
    root: &Path,
) -> Result<Vec<(PathBuf, usize)>, Error> {
    let entries = load_list(list)?;
    let roots = [root.to_owned()];
    let mut out: Vec<(PathBuf, usize)> = if entries.is_empty() {
        Vec::new()
    } else {
        detect(&entries, &roots)
            .into_iter()
            .map(|f| (f.dir, f.files))
            .collect()
    };
    let named: Vec<PathBuf> = out.iter().map(|(p, _)| p.clone()).collect();
    for (dir, _, _) in asked_before(conn, &roots, &named)? {
        if dir.starts_with(root) {
            let files = folder_stats(&dir).0;
            out.push((dir, files));
        }
    }
    Ok(out)
}

/// Work out the ScummVM games again from the list in `list` (when there
/// is one), what ScummVM's --detect named before, and every scanned
/// folder. Returns how many there are.
pub(crate) fn refresh(conn: &Connection, list: Option<&Path>) -> Result<usize, Error> {
    static NAMES: Cached<HashMap<String, String>> = Mutex::new(None);
    let entries = load_list(list)?;
    // libretro's names, when its ScummVM.dat is kept beside the list.
    let names = list
        .and_then(Path::parent)
        .map(|d| d.join(crate::layout::SCUMMVM_TITLES))
        .filter(|p| p.is_file())
        .and_then(|p| cached(&NAMES, &p, titles).ok())
        .unwrap_or_default();
    let top = top_roots(conn)?;
    let found = if entries.is_empty() {
        Vec::new()
    } else {
        detect(&entries, &top)
    };
    let system = systems::by_key(systems::key!("ScummVM")).map_or(0, |s| s.id);
    conn.execute("DELETE FROM folder_games", [])?;
    let mut insert = conn.prepare(
        "INSERT OR IGNORE INTO folder_games (path, system_id, title, code, files, size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for f in &found {
        let e = &entries[f.entry];
        let title = match names.get(&e.code) {
            Some(n) => Entry {
                title: n.clone(),
                ..e.clone()
            }
            .row_title(),
            None => e.row_title(),
        };
        insert.execute(params![
            f.dir.to_string_lossy(),
            system,
            title,
            e.code,
            f.files,
            f.size
        ])?;
    }
    let named: Vec<PathBuf> = found.iter().map(|f| f.dir.clone()).collect();
    let asked = asked_before(conn, &top, &named)?;
    for (dir, code, title) in &asked {
        let (n, size) = folder_stats(dir);
        let title = match (names.get(code), tag_of(title)) {
            (Some(name), Some(tag)) => format!("{name} {tag}"),
            (Some(name), None) => name.clone(),
            (None, _) => title.clone(),
        };
        insert.execute(params![dir.to_string_lossy(), system, title, code, n, size])?;
    }
    Ok(found.len() + asked.len())
}
