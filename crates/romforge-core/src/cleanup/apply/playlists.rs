//! Playlists for multi-disk sets a cleanup wrote.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rusqlite::Connection;

use super::super::{Format, Item};
use super::journal_output;
use crate::organize::{rename_noreplace, sanitize};
use crate::{Error, layout, systems};

/// A title's disk or disc number, the set size if it says ("Disk 1 of 4",
/// TOSEC; "Disc 2", Redump), and the title without that group.
pub fn disk_of(title: &str) -> Option<(u32, Option<u32>, String)> {
    for word in ["(Disk ", "(Disc "] {
        let Some(at) = title.find(word) else { continue };
        let rest = &title[at + word.len()..];
        let end = rest.find(')')?;
        let inner = &rest[..end];
        let (n, of) = match inner.split_once(" of ") {
            Some((n, m)) => (n.trim().parse().ok()?, Some(m.trim().parse().ok()?)),
            None => (inner.trim().parse().ok()?, None),
        };
        let base = format!("{}{}", &title[..at], &rest[end + 1..]);
        let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
        return Some((n, of, base.trim().to_owned()));
    }
    None
}

/// For each multi-disk set this batch wrote a disk of, a playlist in the
/// same folder listing its disks in order, once every disk is there (all
/// "of N", else disks 1, 2, ... with no gap): `<set>.m3u` of the zips or
/// CHDs; for a set kept loose, the playlist its core reads (`.m3u` of the
/// images, or np2kai's `.cmd`, one command line), or none. An existing
/// playlist is left alone. Journaled, so undo removes it.
pub(in crate::cleanup) fn write_playlists(
    conn: &Connection,
    batch: i64,
    written: &[&Item],
) -> Result<usize, Error> {
    // (folder, set, playlist kind, loose)
    let mut sets: BTreeSet<(PathBuf, String, &'static str, bool)> = BTreeSet::new();
    for it in written {
        let Some((_, _, base)) = disk_of(&it.title) else {
            continue;
        };
        let Some(dir) = it.target.parent() else { continue };
        let kind = match it.format {
            Format::Loose => systems::by_key(&it.system)
                .and_then(|s| s.loose_sets)
                .unwrap_or(""),
            _ => "m3u",
        };
        if !kind.is_empty() {
            sets.insert((dir.to_owned(), base, kind, it.format == Format::Loose));
        }
    }
    let mut n = 0;
    for (dir, base, kind, loose) in sets {
        let list = dir.join(format!("{}.{kind}", sanitize(&base)));
        if list.exists() {
            continue;
        }
        // Every disk of the set in the folder, by disk number.
        let mut disks: BTreeMap<u32, (Option<u32>, String)> = BTreeMap::new();
        for e in std::fs::read_dir(&dir)?.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((stem, ext)) = name.rsplit_once('.') else {
                continue;
            };
            let ext = ext.to_ascii_lowercase();
            let wanted = if loose {
                !name.starts_with('.') && !["m3u", "cmd"].contains(&ext.as_str())
            } else {
                ["zip", "chd", "7z"].contains(&ext.as_str())
            };
            if !wanted || !e.path().is_file() {
                continue;
            }
            if let Some((d, of, b)) = disk_of(stem)
                && sanitize(&b) == sanitize(&base)
            {
                disks.entry(d).or_insert((of, name));
            }
        }
        let count = disks.values().find_map(|(of, _)| *of);
        let complete = disks.len() >= 2
            && disks.keys().copied().eq(1..=disks.len() as u32)
            && count.is_none_or(|c| c as usize == disks.len());
        if !complete {
            continue;
        }
        let body: String = if kind == "cmd" {
            // np2kai reads one line of at most 1023 bytes: the program,
            // then the disks (quoted, relative to the file).
            let line: String = std::iter::once("np2kai".to_owned())
                .chain(disks.values().map(|(_, f)| format!("\"{f}\"")))
                .collect::<Vec<_>>()
                .join(" ");
            if line.len() > 1023 || disks.values().any(|(_, f)| f.contains('"')) {
                continue;
            }
            line + "\n"
        } else {
            disks.values().map(|(_, f)| format!("{f}\n")).collect()
        };
        let tmp = dir.join(layout::part(&sanitize(&base)));
        std::fs::write(&tmp, body)?;
        match rename_noreplace(&tmp, &list) {
            Ok(()) => {}
            // One appeared meanwhile: it stays.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&tmp);
                continue;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
        journal_output(conn, batch, &list, "file")?;
        n += 1;
    }
    Ok(n)
}
