//! ScummVM games still in the zip or 7z they came in, found by the same
//! list: the archive's index gives each member's name and size, and only
//! for an entry whose files are all there by name and size are the bytes
//! its MD5s cover read, as streams (nothing is unpacked). The archive
//! stands for the game's folder; ScummVM can't play from one, so cleanup
//! unpacks it into a folder.

use std::collections::BTreeSet;

use super::*;
use crate::scan::Stamp;

/// (member name, size) as the archive lists them.
type Members = Vec<(String, u64)>;
/// (member index, bytes, from the end) -> MD5 (`None`: unreadable).
type Md5s = HashMap<(usize, u64, bool), Option<String>>;

/// What was read of each archive, kept while its size and time stay the
/// same: the list is matched again after every scan.
static SEEN: Mutex<Option<HashMap<PathBuf, Seen>>> = Mutex::new(None);
type Seen = (Stamp, Arc<Members>, Md5s);

/// Whether a file is an archive whose members are read here.
pub fn is_archive(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("7z"))
}

/// The game of `entries` an archive holds (in any folder of it), if any:
/// of several, the one listing the most files. `by_name`: the entries
/// listing each file name.
pub(super) fn found_in(
    entries: &[Entry],
    by_name: &HashMap<&str, Vec<usize>>,
    archive: &Path,
) -> Option<Found> {
    let stamp = Stamp::of(archive)?;
    let members = {
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        let seen = seen.get_or_insert_with(HashMap::new);
        match seen.get(archive) {
            Some((s, m, _)) if *s == stamp => m.clone(),
            _ => {
                let m = Arc::new(crate::cleanup::archive_entries(archive).ok()?);
                seen.insert(archive.to_owned(), (stamp, m.clone(), HashMap::new()));
                m
            }
        }
    };
    // Folder inside the archive -> relative path (lower case) -> member.
    // As on disk, a file is also filed under the folder above its own.
    let mut dirs: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (i, (name, _)) in members.iter().enumerate() {
        let lower = name.replace('\\', "/").to_lowercase();
        let mut comps: Vec<&str> = lower.split('/').filter(|c| !c.is_empty()).collect();
        let Some(file) = comps.pop() else { continue };
        let mut rel = file.to_owned();
        for up in 0..2 {
            if up > comps.len() {
                break;
            }
            let dir = comps[..comps.len() - up].join("/");
            dirs.entry(dir).or_default().insert(rel.clone(), i);
            if up == comps.len() {
                break;
            }
            rel = format!("{}/{rel}", comps[comps.len() - 1 - up]);
        }
    }
    // Entries whose files are all there by name and size.
    let mut fitting: Vec<(usize, &BTreeMap<String, usize>)> = Vec::new();
    for files in dirs.values() {
        let mut candidates: Vec<usize> = files
            .keys()
            .filter_map(|rel| by_name.get(rel.rsplit('/').next().unwrap_or(rel)))
            .flatten()
            .copied()
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        for i in candidates {
            let fits = entries[i].files.iter().all(|s| {
                files
                    .get(&s.name)
                    .is_some_and(|&m| s.size.is_none_or(|z| z == members[m].1))
            });
            if fits {
                fitting.push((i, files));
            }
        }
    }
    if fitting.is_empty() {
        return None;
    }
    let need: BTreeSet<(usize, u64, bool)> = fitting
        .iter()
        .flat_map(|(i, files)| {
            entries[*i]
                .files
                .iter()
                .filter(|s| !s.md5.is_empty())
                .map(|s| (files[&s.name], s.bytes, s.tail))
        })
        .collect();
    let md5s = md5s(archive, stamp, &members, &need);
    let mut best: Option<usize> = None;
    for (i, files) in &fitting {
        let e = &entries[*i];
        let fits = e.files.iter().all(|s| {
            s.md5.is_empty()
                || md5s
                    .get(&(files[&s.name], s.bytes, s.tail))
                    .and_then(Option::as_deref)
                    == Some(s.md5.as_str())
        });
        if fits && best.is_none_or(|b| entries[b].files.len() < e.files.len()) {
            best = Some(*i);
        }
    }
    Some(Found {
        dir: archive.to_owned(),
        entry: best?,
        files: members.len(),
        size: members.iter().map(|(_, s)| s).sum(),
    })
}

/// The MD5s in `need`, from what was read before or read now.
fn md5s(archive: &Path, stamp: Stamp, members: &Members, need: &BTreeSet<(usize, u64, bool)>) -> Md5s {
    let known: Md5s = {
        let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        seen.as_ref()
            .and_then(|s| s.get(archive))
            .filter(|(s, _, _)| *s == stamp)
            .map(|(_, _, m)| m.clone())
            .unwrap_or_default()
    };
    let missing: Vec<(usize, u64, bool)> = need.iter().filter(|k| !known.contains_key(k)).copied().collect();
    if missing.is_empty() {
        return known;
    }
    let mut got = read_md5s(archive, members, &missing);
    for k in missing {
        got.entry(k).or_insert(None);
    }
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, _, m)) = seen.get_or_insert_with(HashMap::new).get_mut(archive)
        && *s == stamp
    {
        m.extend(got.iter().map(|(k, v)| (*k, v.clone())));
    }
    let mut all = known;
    all.extend(got);
    all
}

/// Read the members `need` names once each (a 7z is one stream: the
/// members before them are read through, nothing is written).
fn read_md5s(archive: &Path, members: &Members, need: &[(usize, u64, bool)]) -> Md5s {
    let mut by_member: BTreeMap<usize, Vec<(u64, bool)>> = BTreeMap::new();
    for &(m, bytes, tail) in need {
        by_member.entry(m).or_default().push((bytes, tail));
    }
    let mut out = Md5s::new();
    let mut put = |m: usize, specs: &[(u64, bool)], r: &mut dyn Read| {
        let sums = ranged_md5s(r, members[m].1, specs).ok();
        for (j, &(bytes, tail)) in specs.iter().enumerate() {
            out.insert((m, bytes, tail), sums.as_ref().map(|s| s[j].clone()));
        }
    };
    if archive.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let Ok(mut zip) = File::open(archive)
            .map_err(Error::from)
            .and_then(|f| zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(Error::from))
        else {
            return out;
        };
        for (m, specs) in &by_member {
            if let Ok(mut r) = zip.by_name(&members[*m].0) {
                put(*m, specs, &mut r);
            }
        }
        return out;
    }
    let by_name: HashMap<&str, usize> = by_member.keys().map(|&m| (members[m].0.as_str(), m)).collect();
    let mut left = by_name.len();
    if let Ok(mut a) = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty()) {
        let _ = a.for_each_entries(|entry, reader| {
            if let Some(&m) = by_name.get(entry.name()) {
                put(m, &by_member[&m], reader);
                left -= 1;
            }
            // A solid archive is one stream: each member is read to its end.
            std::io::copy(reader, &mut std::io::sink())?;
            Ok(left > 0)
        });
    }
    out
}

/// The MD5 of the first (or last) `bytes` of a stream of `size` bytes, for
/// each spec, in one pass (as [`partial_md5`] does on a file).
fn ranged_md5s(r: &mut dyn Read, size: u64, specs: &[(u64, bool)]) -> std::io::Result<Vec<String>> {
    let ranges: Vec<(u64, u64)> = specs
        .iter()
        .map(|&(bytes, tail)| {
            let n = bytes.min(size);
            if tail { (size - n, size) } else { (0, n) }
        })
        .collect();
    let end = ranges.iter().map(|r| r.1).max().unwrap_or(0);
    let mut hashers: Vec<Md5> = specs.iter().map(|_| Md5::new()).collect();
    let mut buf = vec![0u8; 64 * 1024];
    let mut at = 0u64;
    while at < end {
        let want = (end - at).min(buf.len() as u64) as usize;
        let n = r.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        for (h, &(from, to)) in hashers.iter_mut().zip(&ranges) {
            let lo = from.max(at);
            let hi = to.min(at + n as u64);
            if lo < hi {
                h.update(&buf[(lo - at) as usize..(hi - at) as usize]);
            }
        }
        at += n as u64;
    }
    Ok(hashers
        .into_iter()
        .map(|h| crate::hash::hex(&h.finalize()))
        .collect())
}
