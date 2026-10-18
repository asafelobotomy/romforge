//! Milestone 4: discs. Cue and GDI sheets, and CHD images.
//!
//! Redump lists a disc as its tracks (`.bin`, raw 2352-byte sectors) plus
//! the cue sheet. A CHD holds the same tracks, so each track is rebuilt as
//! it would be in its `.bin` and hashed as one archive member:
//! - CD frames in a CHD are 2448 bytes (2352 of sector, 96 of subcode); the
//!   subcode is dropped and each track is padded to 4 frames, which is
//!   skipped (GD-ROM CHDs give the padding per track);
//! - audio samples are stored big-endian and are swapped back;
//! - a pregap that the CHD doesn't store (`PGTYPE` not `V...`) is silence and
//!   is added back as zeros, as it is in a Redump `.bin`.
//!
//! A CHD without CD tracks (a DVD, e.g. PS2) is hashed whole, as its `.iso`.
//! Read only: nothing is extracted to disk and no external tool runs.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::hash::{Hashes, RawHasher};

const FRAME: usize = 2448;
const SECTOR: usize = 2352;
const TRACK_PADDING: u64 = 4;

/// One `FILE` of a cue sheet, or one track line of a GDI sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFile {
    pub name: String,
}

/// The files a `.cue` sheet refers to, in order.
pub fn parse_cue(text: &str) -> Vec<SheetFile> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("FILE ").or_else(|| line.strip_prefix("file ")) else {
            continue;
        };
        let rest = rest.trim();
        let name = if let Some(q) = rest.strip_prefix('"') {
            q.split('"').next().unwrap_or_default().to_owned()
        } else {
            // Unquoted: the name runs up to the file type (last word).
            match rest.rsplit_once(char::is_whitespace) {
                Some((n, _)) => n.trim().to_owned(),
                None => rest.to_owned(),
            }
        };
        if !name.is_empty() {
            out.push(SheetFile { name });
        }
    }
    out
}

/// The track files a `.gdi` sheet refers to (`n lba type size file offset`,
/// the file name possibly quoted).
pub fn parse_gdi(text: &str) -> Vec<SheetFile> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let mut rest = line.trim();
        for _ in 0..4 {
            rest = rest.trim_start();
            match rest.find(char::is_whitespace) {
                Some(i) => rest = &rest[i..],
                None => {
                    rest = "";
                    break;
                }
            }
        }
        let rest = rest.trim();
        let name = if let Some(q) = rest.strip_prefix('"') {
            q.split('"').next().unwrap_or_default()
        } else {
            rest.split_whitespace().next().unwrap_or_default()
        };
        if !name.is_empty() {
            out.push(SheetFile {
                name: name.to_owned(),
            });
        }
    }
    out
}

/// The files a sheet next to `path` refers to, resolved in its folder.
/// `None` when `path` is not a `.cue` or `.gdi`.
pub fn sheet_files(path: &Path) -> Option<std::io::Result<Vec<PathBuf>>> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let parse = match ext.as_str() {
        "cue" => parse_cue,
        "gdi" => parse_gdi,
        _ => return None,
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    Some(std::fs::read(path).map(|bytes| {
        parse(&String::from_utf8_lossy(&bytes))
            .into_iter()
            .map(|f| dir.join(f.name))
            .collect()
    }))
}

/// A track from CHD metadata (`CHTR`/`CHT2`/`CHGD`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub number: u32,
    pub kind: String,
    pub frames: u64,
    pub pregap: u64,
    pub pregap_stored: bool,
    /// Padding frames after the track; `None` means "to a multiple of 4".
    pub pad: Option<u64>,
}

impl Track {
    /// Bytes of each 2352-byte slot that belong to the `.bin`.
    fn sector_bytes(&self) -> usize {
        match self.kind.as_str() {
            "MODE1" | "MODE2_FORM1" => 2048,
            "MODE2_FORM2" => 2324,
            "MODE2" | "MODE2_FORM_MIX" => 2336,
            _ => SECTOR,
        }
    }

    fn padding(&self) -> u64 {
        self.pad
            .unwrap_or((TRACK_PADDING - self.frames % TRACK_PADDING) % TRACK_PADDING)
    }
}

/// Parse `TRACK:1 TYPE:MODE2_RAW SUBTYPE:NONE FRAMES:1234 PREGAP:150 ...`.
pub fn parse_track(meta: &str) -> Option<Track> {
    let field = |k: &str| {
        meta.split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.strip_prefix(':'))
            .map(|v| v.trim_end_matches('\0'))
    };
    Some(Track {
        number: field("TRACK")?.parse().ok()?,
        kind: field("TYPE")?.to_owned(),
        frames: field("FRAMES")?.parse().ok()?,
        pregap: field("PREGAP").and_then(|v| v.parse().ok()).unwrap_or(0),
        pregap_stored: field("PGTYPE").is_some_and(|v| v.starts_with('V')),
        pad: field("PAD").and_then(|v| v.parse().ok()),
    })
}

/// The tracks of a CHD as `.bin`/`.iso` members: (member name, hashes).
pub fn chd_members(path: &Path) -> Result<Vec<(String, Hashes)>, Error> {
    let chd_err = |e: chd::Error| Error::Chd(format!("{}: {e}", path.display()));
    let file = BufReader::new(File::open(path)?);
    let mut chd = chd::Chd::open(file, None).map_err(chd_err)?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let refs: Vec<_> = chd.metadata_refs().collect::<Result<_, _>>().map_err(chd_err)?;
    let mut tracks = Vec::new();
    for r in refs {
        let m = r.read(chd.inner()).map_err(chd_err)?;
        if chd::metadata::KnownMetadata::is_cdrom(m.metatag)
            && let Some(t) = parse_track(&String::from_utf8_lossy(&m.value))
        {
            tracks.push(t);
        }
    }
    tracks.sort_by_key(|t| t.number);

    let hunk_bytes = chd.header().hunk_size() as usize;
    let hunks = chd.header().hunk_count();
    let logical = chd.header().logical_bytes();
    let mut hunk = chd.get_hunksized_buffer();
    let mut scratch = Vec::new();

    if tracks.is_empty() {
        // Not a CD: the logical data is the image.
        let mut h = RawHasher::default();
        let mut left = logical;
        for n in 0..hunks {
            chd.hunk(n)
                .and_then(|mut x| x.read_hunk_in(&mut scratch, &mut hunk))
                .map_err(chd_err)?;
            let take = (left as usize).min(hunk_bytes);
            h.update(&hunk[..take]);
            left -= take as u64;
        }
        return Ok(vec![(format!("{stem}.iso"), h.finish())]);
    }
    if !hunk_bytes.is_multiple_of(FRAME) {
        return Err(Error::Chd(format!(
            "{}: hunks of {hunk_bytes} bytes are not whole CD frames",
            path.display()
        )));
    }

    // A pregap comes from the CHD's metadata, which is untrusted: more than
    // ten minutes of silence isn't a disc, it would only spin the hasher.
    if let Some(t) = tracks
        .iter()
        .find(|t| !t.pregap_stored && t.pregap > 10 * 60 * 75)
    {
        return Err(Error::Chd(format!(
            "{}: track {} claims a pregap of {} frames",
            path.display(),
            t.number,
            t.pregap
        )));
    }
    let many = tracks.len() > 1;
    let mut out = Vec::new();
    // (track index, frames of it left to hash, padding frames left to skip)
    let mut current = 0usize;
    let mut hasher = RawHasher::default();
    let start_track = |t: &Track, h: &mut RawHasher| {
        if !t.pregap_stored && t.pregap > 0 {
            let zeros = vec![0u8; t.sector_bytes()];
            for _ in 0..t.pregap {
                h.update(&zeros);
            }
        }
        (t.frames, t.padding())
    };
    let (mut data_left, mut pad_left) = start_track(&tracks[0], &mut hasher);
    let mut swapped = vec![0u8; SECTOR];

    'hunks: for n in 0..hunks {
        chd.hunk(n)
            .and_then(|mut x| x.read_hunk_in(&mut scratch, &mut hunk))
            .map_err(chd_err)?;
        for frame in hunk.as_chunks::<FRAME>().0 {
            // Finish tracks (and their padding) that have no frames left.
            while data_left == 0 && pad_left == 0 {
                let t = &tracks[current];
                let name = if many {
                    format!("{stem} (Track {}).bin", t.number)
                } else {
                    format!("{stem}.bin")
                };
                out.push((name, std::mem::take(&mut hasher).finish()));
                current += 1;
                if current == tracks.len() {
                    break 'hunks;
                }
                (data_left, pad_left) = start_track(&tracks[current], &mut hasher);
            }
            if data_left > 0 {
                let t = &tracks[current];
                let sector = &frame[..t.sector_bytes()];
                if t.kind == "AUDIO" {
                    for (o, i) in swapped
                        .as_chunks_mut::<2>()
                        .0
                        .iter_mut()
                        .zip(sector.as_chunks::<2>().0)
                    {
                        o[0] = i[1];
                        o[1] = i[0];
                    }
                    hasher.update(&swapped[..sector.len()]);
                } else {
                    hasher.update(sector);
                }
                data_left -= 1;
            } else {
                pad_left -= 1;
            }
        }
    }
    // The last track may end exactly at the end of the data, with its
    // padding past the last hunk.
    if current < tracks.len() && data_left == 0 {
        let t = &tracks[current];
        let name = if many {
            format!("{stem} (Track {}).bin", t.number)
        } else {
            format!("{stem}.bin")
        };
        out.push((name, hasher.finish()));
        current += 1;
    }
    if current < tracks.len() {
        return Err(Error::Chd(format!(
            "{}: the image ends inside track {}",
            path.display(),
            tracks[current].number
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_files() {
        let cue = "REM x\nFILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE Game2.bin BINARY\n";
        let names: Vec<String> = parse_cue(cue).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Game (Track 1).bin", "Game2.bin"]);
    }

    #[test]
    fn gdi_files() {
        let gdi =
            "3\n1 0 4 2352 track01.bin 0\n2 600 0 2352 \"track 02.raw\" 0\n3 45000 4 2352 track03.bin 0\n";
        let names: Vec<String> = parse_gdi(gdi).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["track01.bin", "track 02.raw", "track03.bin"]);
    }

    #[test]
    fn track_metadata() {
        let t = parse_track(
            "TRACK:2 TYPE:AUDIO SUBTYPE:NONE FRAMES:1003 PREGAP:150 PGTYPE:VAUDIO PGSUB:RW POSTGAP:0\0",
        )
        .unwrap();
        assert_eq!(
            (t.number, t.kind.as_str(), t.frames, t.pregap),
            (2, "AUDIO", 1003, 150)
        );
        assert!(t.pregap_stored);
        assert_eq!(t.padding(), 1);
        let g = parse_track(
            "TRACK:3 TYPE:MODE1_RAW SUBTYPE:NONE FRAMES:500 PAD:7 PREGAP:0 PGTYPE:MODE1 PGSUB:NONE POSTGAP:0",
        )
        .unwrap();
        assert_eq!(g.padding(), 7);
        assert!(!g.pregap_stored);
        assert!(parse_track("garbage").is_none());
    }
}
