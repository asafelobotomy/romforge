use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{EXTENSIONS, MAX_SIZE};

/// A plain file, or a zip / 7z member, read into memory (at most
/// `MAX_SIZE` bytes).
pub(crate) fn read_entry(path: &std::path::Path, member: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    if member.is_empty() {
        let len = std::fs::metadata(path).ok()?.len();
        return (len <= MAX_SIZE).then(|| std::fs::read(path).ok()).flatten();
    }
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "zip" => {
            let mut zip =
                zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(path).ok()?)).ok()?;
            let mut m = zip.by_name(member).ok()?;
            if m.size() > MAX_SIZE {
                return None;
            }
            let mut out = Vec::with_capacity(m.size() as usize);
            // The declared size may lie: never read past the limit.
            m.by_ref().take(MAX_SIZE + 1).read_to_end(&mut out).ok()?;
            (out.len() as u64 <= MAX_SIZE).then_some(out)
        }
        "7z" => {
            let mut a = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty()).ok()?;
            let size = a.archive().files.iter().find(|e| e.name() == member)?.size();
            (size <= MAX_SIZE).then(|| a.read_file(member).ok()).flatten()
        }
        _ => None,
    }
}

/// Finds the patches beside ROMs: files of the same name with a patch
/// extension, next to it on disk or in the same archive. Each folder and
/// archive is listed once, however many ROMs are asked about.
#[derive(Default)]
pub(crate) struct PatchFinder {
    /// folder -> its patch files
    dirs: HashMap<PathBuf, Vec<PathBuf>>,
    /// archive -> its patch members
    archives: HashMap<PathBuf, Vec<String>>,
}

fn stem_of(name: &str) -> String {
    let n = name.rsplit('/').next().unwrap_or(name);
    n.rsplit_once('.').map_or(n, |(s, _)| s).to_owned()
}

/// Whether a file name has a patch extension.
pub fn is_patch_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, e)| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

impl PatchFinder {
    /// The patches for the ROM at `path` (or its `member` in that archive).
    pub(crate) fn beside(&mut self, path: &Path, member: &str) -> Vec<Vec<u8>> {
        if member.is_empty() {
            let Some(dir) = path.parent() else {
                return Vec::new();
            };
            let patches = self.dirs.entry(dir.to_path_buf()).or_insert_with(|| {
                std::fs::read_dir(dir)
                    .map(|entries| {
                        entries
                            .flatten()
                            .map(|e| e.path())
                            .filter(|p| p.file_name().is_some_and(|n| is_patch_name(&n.to_string_lossy())))
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let stem = stem_of(&path.to_string_lossy());
            return patches
                .iter()
                .filter(|p| stem_of(&p.to_string_lossy()) == stem)
                .filter_map(|p| read_entry(p, ""))
                .collect();
        }
        let names = self.archives.entry(path.to_path_buf()).or_insert_with(|| {
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            let all: Vec<String> = match ext.as_deref() {
                Some("zip") => std::fs::File::open(path)
                    .ok()
                    .and_then(|f| zip::ZipArchive::new(std::io::BufReader::new(f)).ok())
                    .map(|z| {
                        z.file_names()
                            .filter_map(|n| n.ok().map(|n| n.into_owned()))
                            .collect()
                    })
                    .unwrap_or_default(),
                Some("7z") => sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
                    .map(|a| a.archive().files.iter().map(|e| e.name().to_owned()).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            all.into_iter().filter(|n| is_patch_name(n)).collect()
        });
        let stem = stem_of(member);
        let wanted: Vec<String> = names.iter().filter(|n| stem_of(n) == stem).cloned().collect();
        wanted.iter().filter_map(|n| read_entry(path, n)).collect()
    }
}
