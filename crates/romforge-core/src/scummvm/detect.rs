use super::*;

/// MD5 of the first (or last) `bytes` of a file.
pub(super) fn partial_md5(path: &Path, bytes: u64, tail: bool) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    if tail {
        let len = f.metadata()?.len();
        f.seek(SeekFrom::Start(len.saturating_sub(bytes)))?;
    }
    let mut h = Md5::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut left = bytes;
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let n = f.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(crate::hash::hex(&h.finalize()))
}

/// A folder ScummVM's list names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub dir: PathBuf,
    pub entry: usize,
    pub files: usize,
    pub size: u64,
}

/// Every folder under `roots` that is one of `entries`: all its listed
/// files there, by name (any case), size and partial MD5. Of several
/// entries that fit, the one listing the most files wins (as in ScummVM).
/// A folder inside a detected one isn't looked at again.
pub fn detect(entries: &[Entry], roots: &[PathBuf]) -> Vec<Found> {
    // Last name component -> (entry, file) listing it.
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        for f in &e.files {
            let last = f.name.rsplit('/').next().unwrap_or(&f.name);
            by_name.entry(last).or_default().push(i);
        }
    }
    // Folder -> its files (relative path in lower case -> path).
    let mut dirs: BTreeMap<PathBuf, BTreeMap<String, PathBuf>> = BTreeMap::new();
    for root in roots {
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !crate::layout::is_own_dir(e.file_name()));
        for e in walker.filter_map(Result::ok) {
            if !e.file_type().is_file() {
                continue;
            }
            // Files are filed under their folder and the one above it, for
            // entries naming a file in a subfolder ("data/x.dat").
            let mut rel = e.file_name().to_string_lossy().to_lowercase();
            let mut dir = e.path().parent();
            for _ in 0..2 {
                let Some(d) = dir else { break };
                if !d.starts_with(root) {
                    break;
                }
                dirs.entry(d.to_owned())
                    .or_default()
                    .insert(rel.clone(), e.path().to_owned());
                let Some(name) = d.file_name() else { break };
                rel = format!("{}/{rel}", name.to_string_lossy().to_lowercase());
                dir = d.parent();
            }
        }
    }
    let mut md5s: HashMap<(PathBuf, u64, bool), Option<String>> = HashMap::new();
    let mut out: Vec<Found> = Vec::new();
    for (dir, files) in &dirs {
        if out.iter().any(|f| dir.starts_with(&f.dir)) {
            continue;
        }
        let mut candidates: Vec<usize> = files
            .keys()
            .filter_map(|rel| by_name.get(rel.rsplit('/').next().unwrap_or(rel)))
            .flatten()
            .copied()
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        let mut best: Option<usize> = None;
        for i in candidates {
            let e = &entries[i];
            let fits = e.files.iter().all(|spec| {
                let Some(path) = files.get(&spec.name) else {
                    return false;
                };
                let Ok(meta) = std::fs::metadata(path) else {
                    return false;
                };
                if spec.size.is_some_and(|s| s != meta.len()) {
                    return false;
                }
                spec.md5.is_empty()
                    || md5s
                        .entry((path.clone(), spec.bytes, spec.tail))
                        .or_insert_with(|| partial_md5(path, spec.bytes, spec.tail).ok())
                        .as_deref()
                        == Some(spec.md5.as_str())
            });
            if fits && best.is_none_or(|b| entries[b].files.len() < e.files.len()) {
                best = Some(i);
            }
        }
        if let Some(entry) = best {
            let (n, size) = folder_stats(dir);
            out.push(Found {
                dir: dir.clone(),
                entry,
                files: n,
                size,
            });
        }
    }
    out
}

/// How many files a folder holds, and their size.
pub(super) fn folder_stats(dir: &Path) -> (usize, u64) {
    let (mut n, mut size) = (0, 0);
    for f in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if f.file_type().is_file() {
            n += 1;
            size += f.metadata().map_or(0, |m| m.len());
        }
    }
    (n, size)
}

/// A folder's files as one SHA1: every path (lower case) and size, leaving
/// out what import adds (`romforge/`, `*.scummvm`).
pub fn fingerprint(dir: &Path) -> Option<String> {
    use sha1::{Digest as _, Sha1};
    let mut lines: Vec<String> = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !(e.depth() == 1 && e.file_name() == crate::layout::EXTRAS_DIR));
    for e in walker {
        let e = e.ok()?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(dir).ok()?.to_string_lossy().to_lowercase();
        if rel.ends_with(".scummvm") {
            continue;
        }
        lines.push(format!("{rel}\0{}", e.metadata().ok()?.len()));
    }
    if lines.is_empty() {
        return None;
    }
    lines.sort();
    let mut h = Sha1::new();
    for l in &lines {
        h.update(l.as_bytes());
        h.update(b"\n");
    }
    Some(crate::hash::hex(&h.finalize()))
}

/// What `scummvm --detect` printed for `root`: (folder, game id,
/// description). Its table is "engine:gameid  description  full path";
/// the path is found by the root it starts with.
pub fn parse_detect(out: &str, root: &Path) -> Vec<(PathBuf, String, String)> {
    let root = root.to_string_lossy();
    let mut found = Vec::new();
    for line in out.lines() {
        let Some((id, rest)) = line.split_once(' ') else {
            continue;
        };
        let Some((_, game)) = id.split_once(':') else {
            continue;
        };
        let Some(at) = rest
            .find(&format!(" {root}"))
            .or_else(|| rest.starts_with(&*root).then_some(0))
        else {
            continue;
        };
        let (desc, path) = rest.split_at(at);
        let path = path.trim_start();
        if game.is_empty() || !(path == root || path.starts_with(&format!("{root}/"))) {
            continue;
        }
        found.push((
            PathBuf::from(path.trim_end_matches('/')),
            game.to_owned(),
            desc.trim().to_owned(),
        ));
    }
    found
}
