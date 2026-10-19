//! Helpers from the `organize` tests.

use std::path::Path;

use romforge_core::Library;
use romforge_core::hash::hash_stream;

use super::fixtures;

pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let ty = e.file_type().unwrap();
        let dst = to.join(e.file_name());
        if ty.is_dir() {
            copy_tree(&e.path(), &dst);
        } else if ty.is_file() {
            std::fs::copy(e.path(), dst).unwrap();
        }
    }
}

/// (relative path, size) of every file under `dir`.
pub fn listing(dir: &Path) -> Vec<(String, u64)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                out.push((rel, e.metadata().unwrap().len()));
            }
        }
    }
    let mut out = Vec::new();
    if dir.exists() {
        walk(dir, dir, &mut out);
    }
    out.sort();
    out
}

pub fn library_with_fixture_dats() -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib
}

pub fn digest(d: &[u8]) -> romforge_core::hash::Digest {
    hash_stream(d, d.len() as u64).unwrap().raw
}

pub fn rom(name: &str, d: &[u8]) -> String {
    let h = digest(d);
    format!(
        r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
        h.size, h.crc32, h.md5, h.sha1
    )
}
