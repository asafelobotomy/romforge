//! A BIOS zip (MAME-style: `neogeo.zip`, `a7800.zip`) checked by its
//! members, as RetroBIOS lists them (`contents`), never by the zip's own
//! hash: zips of the same set differ in packing. A member counts when its
//! size and SHA-1 or MD5 are the listed ones, whatever it is named (house
//! rule 8; CRC32 alone never counts). Members are small, so they are hashed
//! in memory, never extracted.

use std::io::Read;
use std::path::Path;

use super::{Member, State};
use crate::hash::Digest;

/// The biggest member read (BIOS ROMs are a few MiB at most).
const MAX_MEMBER: u64 = 64 << 20;

/// Each member of the zip at `path` (name, size and hashes); `None` when it
/// isn't a zip romforge can read.
pub(super) fn zip_members(path: &Path) -> Option<Vec<(String, Digest)>> {
    let file = std::fs::File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut m = zip.by_index(i).ok()?;
        if m.is_dir() || m.size() > MAX_MEMBER {
            continue;
        }
        let full = m.name().ok()?.into_owned();
        let name = full.rsplit('/').next().unwrap_or(&full).to_owned();
        let mut buf = Vec::with_capacity(m.size() as usize);
        m.read_to_end(&mut buf).ok()?;
        let d = crate::hash::hash_stream(&buf[..], buf.len() as u64).ok()?.raw;
        out.push((name, d));
    }
    Some(out)
}

/// Whether `m` is among `members` by contents.
pub(super) fn holds(members: &[(String, Digest)], m: &Member) -> bool {
    members.iter().any(|(name, d)| {
        let hashed = !m.sha1.is_empty() || !m.md5.is_empty();
        m.size.is_none_or(|s| s == d.size)
            && if hashed {
                m.sha1.contains(&d.sha1) || m.md5.contains(&d.md5)
            } else {
                name.eq_ignore_ascii_case(&m.name)
            }
    })
}

/// A zip's state against what it should hold, and the members it lacks.
pub(super) fn check(contents: &[Member], members: &[(String, Digest)]) -> (State, Vec<String>) {
    let missing: Vec<String> = contents
        .iter()
        .filter(|m| !holds(members, m))
        .map(|m| m.name.clone())
        .collect();
    let state = if missing.is_empty() {
        if contents.iter().any(|m| !m.sha1.is_empty() || !m.md5.is_empty()) {
            State::Accepted
        } else {
            State::Present
        }
    } else if missing.len() == contents.len() {
        State::WrongHash
    } else {
        State::Incomplete
    };
    (state, missing)
}
