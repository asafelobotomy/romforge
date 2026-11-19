//! A PSP game converted to CHD, checked against what PPSSPP (libchdr) reads
//! rather than against romforge's own reader: the raw header fields
//! PPSSPP's `CHDFileBlockDevice` relies on, every sector read back, and,
//! where MAME's `chdman` is installed, its verdict on the file.
//!
//! PPSSPP takes the CHD's hunk size as its sector size and block `n` as
//! LBA `n`, so a hunk of anything but 2048 bytes reads the wrong data or
//! none. Not covered here: PPSSPP itself opening the game (see PLAN.md).

use std::path::{Path, PathBuf};
use std::process::Command;

use romforge_core::cleanup::Format;
use romforge_core::hash::hash_stream;
use romforge_core::settings::Settings;
use romforge_core::{Library, Scope};

const SECTOR: usize = 2048;

/// Sectors that compress well, badly and not at all, so a CHD has hunks
/// stored in more than one way.
fn iso(sectors: usize) -> Vec<u8> {
    let mut x = 0x2545_f491_u32;
    let mut out = Vec::with_capacity(sectors * SECTOR);
    for s in 0..sectors {
        match s % 4 {
            0 => out.extend(std::iter::repeat_n(0u8, SECTOR)),
            1 => out.extend((0..SECTOR).map(|i| (i % 16) as u8)),
            _ => out.extend((0..SECTOR).map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })),
        }
    }
    out
}

/// `iso` as `system`'s game, converted by cleanup; returns the CHD.
fn convert(dir: &Path, dat_name: &str, system: &str, iso: &[u8]) -> PathBuf {
    let (source, library) = (dir.join("source"), dir.join("library"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    std::fs::write(source.join("Game (USA).iso"), iso).unwrap();
    let x = hash_stream(iso, iso.len() as u64).unwrap().raw;
    let dat = dir.join("game.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>{dat_name}</name></header><game name="Game (USA)"><rom name="Game (USA).iso" size="{}" crc="{}" md5="{}" sha1="{}"/></game></datafile>"#,
            x.size, x.crc32, x.md5, x.sha1
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        staging_dir: Some(dir.join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let plan = lib.cleanup_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(plan.items[0].format, Format::ChdDvd);
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    library
        .canonicalize()
        .unwrap()
        .join(system)
        .join("Game (USA).chd")
}

/// The fields of a CHD v5 header, read from the bytes (libchdr's
/// `header_guess_unitbytes` and friends read these, not romforge's crate).
#[derive(Debug)]
struct Header {
    tag: [u8; 8],
    length: u32,
    version: u32,
    compressors: [[u8; 4]; 4],
    logical_bytes: u64,
    map_offset: u64,
    meta_offset: u64,
    hunk_bytes: u32,
    unit_bytes: u32,
}

fn header(chd: &Path) -> Header {
    let b = std::fs::read(chd).unwrap();
    let be32 = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    let be64 = |o: usize| u64::from_be_bytes(b[o..o + 8].try_into().unwrap());
    Header {
        tag: b[0..8].try_into().unwrap(),
        length: be32(8),
        version: be32(12),
        compressors: std::array::from_fn(|i| b[16 + 4 * i..20 + 4 * i].try_into().unwrap()),
        logical_bytes: be64(32),
        map_offset: be64(40),
        meta_offset: be64(48),
        hunk_bytes: be32(56),
        unit_bytes: be32(60),
    }
}

#[test]
fn a_psp_chd_has_the_header_ppsspp_reads() {
    let dir = tempfile::tempdir().unwrap();
    let iso = iso(96);
    let chd = convert(dir.path(), "Sony - PlayStation Portable", "PSP", &iso);
    let h = header(&chd);
    let size = std::fs::metadata(&chd).unwrap().len();

    assert_eq!(&h.tag, b"MComprHD", "{h:?}");
    assert_eq!((h.version, h.length), (5, 124), "the v5 header libchdr reads");
    assert_eq!(h.hunk_bytes, 2048, "PPSSPP's sector size is the hunk size: {h:?}");
    assert_eq!(h.unit_bytes, 2048, "{h:?}");
    assert_eq!(h.logical_bytes, iso.len() as u64, "{h:?}");
    assert_eq!(h.logical_bytes % u64::from(h.hunk_bytes), 0);
    // Codecs libchdr has (zlib, lzma, huffman, flac, the CD ones), or none.
    let known: [&[u8; 4]; 8] = [
        b"\0\0\0\0",
        b"zlib",
        b"lzma",
        b"huff",
        b"flac",
        b"cdzl",
        b"cdlz",
        b"cdfl",
    ];
    for c in h.compressors {
        assert!(known.contains(&&c), "a codec libchdr doesn't have: {h:?}");
    }
    assert!(h.map_offset > 124 && h.map_offset < size, "{h:?} in {size}");
    // Unlike chdman's, romforge's DVD CHD has no metadata (offset 0): the
    // tools above read it anyway; whether PPSSPP does is for a manual run.
    assert!(h.meta_offset == 0 || h.meta_offset < size, "{h:?} in {size}");
}

#[test]
fn every_sector_of_a_psp_chd_reads_back_as_the_iso_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let iso = iso(96);
    let chd = convert(dir.path(), "Sony - PlayStation Portable", "PSP", &iso);

    // As PPSSPP does: hunk n is sector n (LBA n), whole, in any order.
    let mut f = chd::Chd::open(std::io::BufReader::new(std::fs::File::open(&chd).unwrap()), None).unwrap();
    assert_eq!(f.header().hunk_count() as usize, iso.len() / SECTOR);
    let mut buf = f.get_hunksized_buffer();
    let mut cmp = f.get_hunksized_buffer();
    for lba in (0..iso.len() / SECTOR).rev() {
        f.hunk(lba as u32)
            .unwrap()
            .read_hunk_in(&mut cmp, &mut buf)
            .unwrap();
        assert_eq!(buf, iso[lba * SECTOR..(lba + 1) * SECTOR], "sector {lba}");
    }
}

#[test]
fn another_dvd_system_keeps_the_default_hunk_size() {
    let dir = tempfile::tempdir().unwrap();
    let chd = convert(dir.path(), "Sony - PlayStation 2", "PlayStation 2", &iso(96));
    assert_ne!(header(&chd).hunk_bytes, 2048, "only PSP's CHDs are changed");
}

/// MAME's `chdman` shares libchdr's format code with PPSSPP; skipped where
/// it isn't installed (CI: `apt install mame-tools`).
#[test]
fn chdman_accepts_a_psp_chd_and_gets_the_iso_back() {
    let chdman = |args: &[&str]| Command::new("chdman").args(args).output();
    if chdman(&["help"]).is_err() {
        eprintln!("skipped: chdman isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let iso = iso(96);
    let chd = convert(dir.path(), "Sony - PlayStation Portable", "PSP", &iso);
    let chd = chd.to_str().unwrap();

    let out = chdman(&["verify", "-i", chd]).unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = chdman(&["info", "-i", chd]).unwrap();
    let info = String::from_utf8_lossy(&out.stdout);
    assert!(
        info.contains("Hunk Size:") && info.contains("2,048 bytes"),
        "{info}"
    );
    assert!(info.contains("Version:") && info.contains("5"), "{info}");

    let back = dir.path().join("back.iso");
    let out = chdman(&["extractdvd", "-i", chd, "-o", back.to_str().unwrap()]).unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        std::fs::read(&back).unwrap() == iso,
        "chdman gets another ISO back"
    );
}
