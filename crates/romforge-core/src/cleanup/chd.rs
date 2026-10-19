use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::Error;

use super::parts::open_part;
use super::{Format, Item, Part};

/// Sectors in a CD track: data tracks start with the 12-byte sync pattern
/// and say their mode in byte 15; anything else is audio.
pub(super) fn track_type(first_sector: &[u8]) -> &'static str {
    if first_sector.len() >= 16 && first_sector[..12] == crate::disc::SYNC {
        match first_sector[15] {
            1 => "MODE1_RAW",
            _ => "MODE2_RAW",
        }
    } else {
        "AUDIO"
    }
}

/// Pregap frames per track from a cue sheet (INDEX 00 to INDEX 01 within a
/// track's FILE), by track order.
pub(super) fn cue_pregaps(cue: &str) -> Vec<u64> {
    let msf = |s: &str| -> Option<u64> {
        let v: Vec<u64> = s.split(':').filter_map(|x| x.parse().ok()).collect();
        (v.len() == 3).then(|| (v[0] * 60 + v[1]) * 75 + v[2])
    };
    let mut out: Vec<u64> = Vec::new();
    let mut idx0: Option<u64> = None;
    for line in cue.lines().map(str::trim) {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["TRACK", ..] => {
                out.push(0);
                idx0 = None;
            }
            ["INDEX", "00", t] => idx0 = msf(t),
            ["INDEX", "01", t] => {
                if let (Some(a), Some(b), Some(last)) = (idx0, msf(t), out.last_mut()) {
                    *last = b.saturating_sub(a);
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether [`Item::sheet`] was taken into the CHD.
pub(crate) fn write_chd(item: &Item, staging: &Path, out: &Path) -> Result<bool, Error> {
    use chd::metadata::KnownMetadata;
    use chd::write::{ChdWriteOptions, ChdWriter};
    let cerr = |e: chd::Error| Error::Chd(format!("{}: {e}", out.display()));
    let tracks: Vec<&Part> = item
        .parts
        .iter()
        .filter(|p| !p.name.to_lowercase().ends_with(".cue"))
        .collect();
    let file = File::create_new(out)?;
    let mut took_sheet = false;
    if item.format == Format::ChdDvd {
        let p = tracks[0];
        let opts = ChdWriteOptions::dvd().logical_bytes(p.size);
        let mut w = ChdWriter::new(file, opts).map_err(cerr)?;
        std::io::copy(&mut open_part(p, staging)?, &mut w)?;
        w.finish().map_err(cerr)?.sync_all()?;
    } else {
        // Pregaps from the game's own cue sheet when there is one.
        let (cue, took) = own_cue(item, staging);
        took_sheet = took;
        // Several tracks need their pregaps from a cue: without one the CHD
        // would put every track's INDEX 01 in the wrong place.
        if cue.is_none() && tracks.len() > 1 {
            drop(file);
            let _ = std::fs::remove_file(out);
            return Err(Error::Cleanup(format!(
                "{}: no cue sheet that fits its {} tracks, so their pregaps are unknown",
                item.title,
                tracks.len()
            )));
        }
        let pregaps = cue.map(|c| cue_pregaps(&c)).unwrap_or_default();
        let mut w = ChdWriter::new(file, ChdWriteOptions::cd()).map_err(cerr)?;
        let mut meta = Vec::new();
        for (i, p) in tracks.iter().enumerate() {
            if p.size % 2352 != 0 {
                return Err(Error::Cleanup(format!(
                    "{} isn't whole 2352-byte sectors",
                    p.name
                )));
            }
            let frames = p.size / 2352;
            let mut r = open_part(p, staging)?;
            let mut sector = vec![0u8; 2352];
            let mut kind = "AUDIO";
            for f in 0..frames {
                r.read_exact(&mut sector)?;
                if f == 0 {
                    kind = track_type(&sector);
                }
                if kind == "AUDIO" {
                    for pair in sector.as_chunks_mut::<2>().0 {
                        pair.swap(0, 1);
                    }
                }
                w.write_all(&sector)?;
                w.write_all(&[0u8; 96])?;
            }
            for _ in 0..(4 - frames % 4) % 4 {
                w.write_all(&[0u8; 2448])?;
            }
            let pregap = pregaps.get(i).copied().unwrap_or(0).min(frames);
            let pgtype = if pregap > 0 {
                format!("V{kind}")
            } else {
                kind.to_owned()
            };
            meta.push(format!(
                "TRACK:{} TYPE:{kind} SUBTYPE:NONE FRAMES:{frames} PREGAP:{pregap} PGTYPE:{pgtype} PGSUB:NONE POSTGAP:0\0",
                i + 1
            ));
        }
        for m in meta {
            w.add_metadata(KnownMetadata::CdRomTrack2, m.as_bytes(), true)
                .map_err(cerr)?;
        }
        w.finish().map_err(cerr)?.sync_all()?;
    }
    // Read it back: each track must hash as its file did.
    let back = crate::disc::chd_members(out)?;
    if back.len() != tracks.len() {
        return Err(Error::Cleanup(format!(
            "{}: {} tracks read back, {} written",
            out.display(),
            back.len(),
            tracks.len()
        )));
    }
    for ((_, h), p) in back.iter().zip(&tracks) {
        if h.raw.sha1 != p.sha1 {
            return Err(Error::Cleanup(format!(
                "{}: {} doesn't hash as before",
                out.display(),
                p.name
            )));
        }
    }
    Ok(took_sheet)
}

/// The one `.cue` in the same archive or folder as a disc's tracks, when
/// the DAT's cue wasn't found ([`Item::sheet`]).
pub(crate) fn loose_cue(conn: &Connection, parts: &[Part]) -> Result<Option<Part>, Error> {
    let Some(first) = parts.first() else {
        return Ok(None);
    };
    let dir = first
        .path
        .parent()
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .into_owned();
    let found: Vec<Part> = conn
        .prepare(
            "SELECT path, member, size, sha1 FROM files WHERE path = ?1 AND lower(member) LIKE '%.cue'
             UNION SELECT path, member, size, sha1 FROM files WHERE member = '' AND lower(path) LIKE '%.cue'
               AND substr(path, 1, length(?2) + 1) = ?2 || '/' AND instr(substr(path, length(?2) + 2), '/') = 0",
        )?
        .query_map(params![first.path.to_string_lossy(), dir], |r| {
            let (path, member): (String, String) = (r.get(0)?, r.get(1)?);
            let n = if member.is_empty() { &path } else { &member };
            Ok(Part {
                name: n.rsplit('/').next().unwrap_or(n).to_owned(),
                path: PathBuf::from(&path),
                member,
                size: r.get(2)?,
                sha1: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(match <[Part; 1]>::try_from(found) {
        Ok([one]) => Some(one),
        Err(_) => None,
    })
}

/// The game's own cue sheet: one among its parts, else [`Item::sheet`] when
/// its FILE lines name exactly the tracks. `(text, the sheet was taken)`.
pub(super) fn own_cue(item: &Item, staging: &Path) -> (Option<String>, bool) {
    let read = |p: &Part| -> Option<String> {
        let mut s = String::new();
        open_part(p, staging)
            .ok()?
            .take(1 << 20)
            .read_to_string(&mut s)
            .ok()?;
        Some(s)
    };
    if let Some(p) = item
        .parts
        .iter()
        .find(|p| p.name.to_lowercase().ends_with(".cue"))
    {
        return (read(p), false);
    }
    let Some(sheet) = &item.sheet else {
        return (None, false);
    };
    let tracks: Vec<&str> = item
        .parts
        .iter()
        .map(|p| {
            let n = if p.member.is_empty() {
                p.path.to_str().unwrap_or_default()
            } else {
                &p.member
            };
            n.rsplit('/').next().unwrap_or(n)
        })
        .collect();
    let Some(text) = read(sheet) else {
        return (None, false);
    };
    let names: Vec<String> = crate::disc::parse_cue(&text)
        .into_iter()
        .map(|f| f.name.rsplit(['/', '\\']).next().unwrap_or(&f.name).to_owned())
        .collect();
    // One TRACK per FILE: a single .bin holding several tracks would lose
    // its layout in a CHD built track by track.
    let track_lines = text
        .lines()
        .filter(|l| l.trim_start().to_ascii_uppercase().starts_with("TRACK "))
        .count();
    if names == tracks && track_lines == names.len() {
        (Some(text), true)
    } else {
        (None, false)
    }
}
