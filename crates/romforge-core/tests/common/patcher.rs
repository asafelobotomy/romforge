//! Helpers from the `patcher` tests.

use std::io::Write as _;
use std::path::Path;

use chd::metadata::KnownMetadata;
use chd::write::{ChdWriteOptions, ChdWriter};
use romforge_core::Library;

pub fn sha1(d: &[u8]) -> String {
    romforge_core::hash::hash_stream(d, d.len() as u64)
        .unwrap()
        .raw
        .sha1
}

/// An IPS writing `data` at `at`.
pub fn ips(at: u32, data: &[u8]) -> Vec<u8> {
    let mut p = b"PATCH".to_vec();
    p.extend_from_slice(&at.to_be_bytes()[1..]);
    p.extend_from_slice(&(data.len() as u16).to_be_bytes());
    p.extend_from_slice(data);
    p.extend_from_slice(b"EOF");
    p
}

pub fn num(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let x = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(0x80 | x);
            return;
        }
        out.push(x);
        n -= 1;
    }
}

/// A BPS that writes `target` whole (one TargetRead), made from a source
/// of `source_size` bytes with CRC32 `source_crc`.
pub fn bps(source_size: u64, source_crc: u32, target: &[u8]) -> Vec<u8> {
    let mut p = b"BPS1".to_vec();
    num(source_size, &mut p);
    num(target.len() as u64, &mut p);
    num(0, &mut p);
    num(((target.len() as u64 - 1) << 2) | 1, &mut p);
    p.extend_from_slice(target);
    p.extend_from_slice(&source_crc.to_le_bytes());
    p.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
    let crc = crc32fast::hash(&p);
    p.extend_from_slice(&crc.to_le_bytes());
    p
}

pub fn row_id(lib: &Library, title: &str) -> String {
    lib.library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == title)
        .unwrap_or_else(|| panic!("no row {title}"))
        .id
}

pub fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// A two-track CD as a CHD (data, then audio with a 2-frame pregap not
/// stored); returns the data and audio `.bin`s as Redump has them.
pub fn write_cd(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let (data_frames, audio_frames) = (10usize, 7usize);
    let mut data_bin = pattern(1, data_frames * 2352);
    data_bin[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    data_bin[15] = 1;
    let audio_bin = pattern(2, audio_frames * 2352);
    let mut frames: Vec<u8> = Vec::new();
    let mut push = |bin: &[u8], audio: bool, n: usize| {
        for s in bin.chunks(2352) {
            if audio {
                for pair in s.chunks(2) {
                    frames.extend_from_slice(&[pair[1], pair[0]]);
                }
            } else {
                frames.extend_from_slice(s);
            }
            frames.extend_from_slice(&[0u8; 96]);
        }
        for _ in 0..(4 - n % 4) % 4 {
            frames.extend_from_slice(&[0u8; 2448]);
        }
    };
    push(&data_bin, false, data_frames);
    push(&audio_bin, true, audio_frames);
    let mut w = ChdWriter::new(std::fs::File::create(path).unwrap(), ChdWriteOptions::cd()).unwrap();
    w.write_all(&frames).unwrap();
    for meta in [
        format!(
            "TRACK:1 TYPE:MODE1_RAW SUBTYPE:NONE FRAMES:{data_frames} PREGAP:0 PGTYPE:MODE1 PGSUB:NONE POSTGAP:0\0"
        ),
        format!(
            "TRACK:2 TYPE:AUDIO SUBTYPE:NONE FRAMES:{audio_frames} PREGAP:2 PGTYPE:AUDIO PGSUB:NONE POSTGAP:0\0"
        ),
    ] {
        w.add_metadata(KnownMetadata::CdRomTrack2, meta.as_bytes(), true)
            .unwrap();
    }
    w.finish().unwrap();
    let mut audio = vec![0u8; 2 * 2352];
    audio.extend_from_slice(&audio_bin);
    (data_bin, audio)
}
