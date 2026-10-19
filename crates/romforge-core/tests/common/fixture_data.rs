//! Helpers from the `fixtures` tests.

use romforge_core::hash::hash_stream;

/// Deterministic filler bytes that never start with a header magic.
pub fn body(seed: &str, len: usize) -> Vec<u8> {
    let mut x: u64 = seed.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    });
    let mut out: Vec<u8> = (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    out[0] = 0xff;
    out
}

pub fn header(magic_at: usize, magic: &[u8], len: usize) -> Vec<u8> {
    let mut h = vec![0u8; len];
    h[magic_at..magic_at + magic.len()].copy_from_slice(magic);
    h
}

/// Files a scan must skip by default (no ROM extension). `#` = archive member.
pub const NON_ROMS: &[(&str, &[u8])] = &[
    ("extras/Episode 01.mkv", b"\x1a\x45\xdf\xa3 not really a video"),
    ("extras/readme.nfo", b"release notes"),
    (
        "archives/Test GBA.zip#CDRomance.url",
        b"[InternetShortcut]\nURL=https://example.invalid/\n",
    ),
    ("archives/Mixed.7z#notes.txt", b"some notes"),
];

/// A symlinked folder inside the tree, pointing at a sibling. Never followed.
pub const LINKED_DIR: (&str, &str) = ("linked-gb", "gb");

pub struct Fixture {
    /// Path under fixtures/roms; `archive.zip#member` for archive members.
    pub path: &'static str,
    pub header: Vec<u8>,
    pub body: Vec<u8>,
}

/// One DAT game: name, body, and which hashes the DAT lists.
pub struct DatGame {
    pub name: &'static str,
    pub roms: Vec<(&'static str, Vec<u8>, Hashes)>,
}

#[derive(Clone, Copy)]
pub enum Hashes {
    All,
    Md5Crc,
    CrcOnly,
}

pub struct DatFile {
    pub file: &'static str,
    pub name: &'static str,
    pub header_hint: Option<&'static str>,
    pub games: Vec<DatGame>,
}

pub fn nes_header(nes2: bool) -> Vec<u8> {
    let mut h = header(0, b"NES\x1a", 16);
    h[4] = 1; // 16 KiB PRG units, not that anything reads it
    if nes2 {
        h[7] = 0x08;
    }
    h
}

pub fn layout() -> (Vec<Fixture>, Vec<DatFile>) {
    let nes1 = body("nes1", 1024);
    let nes2 = body("nes2", 1024);
    let nes7z = body("nes7z", 1024);
    let fds = body("fds", 1024);
    let lynx = body("lynx", 1024);
    let lynx_raw = body("lynx-raw", 1088);
    let a78 = body("a7800", 1024);
    let snes_copier = body("snes-copier", 2048);
    let snes_clean = body("snes-clean", 2048);
    let gb = body("gb", 1024);
    let crc_only = body("crc-only", 1024);
    let shared = body("shared", 1024);
    let md5_only = body("md5-only", 1024);
    let gba = body("gba", 1024);
    let md = body("md", 1024);

    let f = |path, header: Vec<u8>, body: &Vec<u8>| Fixture {
        path,
        header,
        body: body.clone(),
    };
    let fixtures = vec![
        f("nes/Test NES (iNES).nes", nes_header(false), &nes1),
        f("nes/Test NES 2.0.nes", nes_header(true), &nes2),
        f("fds/Test FDS.fds", header(0, b"FDS\x1a", 16), &fds),
        f("lynx/Test Lynx (headered).lnx", header(0, b"LYNX", 64), &lynx),
        f("lynx/Test Lynx (no header).lyx", vec![], &lynx_raw),
        f("a7800/Test 7800.a78", header(1, b"ATARI7800", 128), &a78),
        f("snes/Test SNES (copier).smc", vec![0; 512], &snes_copier),
        f("snes/Test SNES (clean).sfc", vec![], &snes_clean),
        f("gb/Test GB.gb", vec![], &gb),
        f("gb/CRC Only.gb", vec![], &crc_only),
        f("gb/Shared Hash.gb", vec![], &shared),
        f("gb/MD5 Only.gb", vec![], &md5_only),
        f("unknown.bin", vec![], &body("unknown", 1024)),
        f("archives/Test GBA.zip#Test GBA.gba", vec![], &gba),
        f("archives/GB copy.zip#Test GB.gb", vec![], &gb),
        f("archives/Mixed.7z#Test MD.md", vec![], &md),
        f(
            "archives/Mixed.7z#Test NES (in 7z).nes",
            nes_header(false),
            &nes7z,
        ),
    ];

    let g = |name, roms: Vec<(&'static str, &Vec<u8>, Hashes)>| DatGame {
        name,
        roms: roms.into_iter().map(|(n, b, h)| (n, b.clone(), h)).collect(),
    };
    use Hashes::*;
    let dats = vec![
        DatFile {
            file: "nes.dat",
            name: "Nintendo - Nintendo Entertainment System",
            header_hint: Some("No-Intro_NES.xml"),
            games: vec![
                g("Test NES (iNES)", vec![("Test NES (iNES).nes", &nes1, All)]),
                g("Test NES 2.0", vec![("Test NES 2.0.nes", &nes2, All)]),
                g("Test NES (in 7z)", vec![("Test NES (in 7z).nes", &nes7z, All)]),
                g(
                    "Missing NES Game",
                    vec![("Missing NES Game.nes", &body("missing", 1024), All)],
                ),
            ],
        },
        DatFile {
            file: "fds.dat",
            name: "Nintendo - Family Computer Disk System (FDS)",
            header_hint: Some("No-Intro_FDS.xml"),
            games: vec![g("Test FDS", vec![("Test FDS.fds", &fds, All)])],
        },
        DatFile {
            file: "lynx.dat",
            name: "Atari - Atari Lynx (LYX)",
            header_hint: Some("No-Intro_LNX.xml"),
            games: vec![
                g(
                    "Test Lynx (headered)",
                    vec![("Test Lynx (headered).lyx", &lynx, All)],
                ),
                g(
                    "Test Lynx (no header)",
                    vec![("Test Lynx (no header).lyx", &lynx_raw, All)],
                ),
            ],
        },
        DatFile {
            file: "a7800.dat",
            name: "Atari - Atari 7800 (BIN)",
            header_hint: Some("No-Intro_A7800.xml"),
            games: vec![g("Test 7800", vec![("Test 7800.a78", &a78, All)])],
        },
        DatFile {
            file: "snes.dat",
            name: "Nintendo - Super Nintendo Entertainment System",
            header_hint: None,
            games: vec![
                g(
                    "Test SNES (copier)",
                    vec![("Test SNES (copier).sfc", &snes_copier, All)],
                ),
                g(
                    "Test SNES (clean)",
                    vec![("Test SNES (clean).sfc", &snes_clean, All)],
                ),
            ],
        },
        DatFile {
            file: "gb.dat",
            name: "Nintendo - Game Boy",
            header_hint: None,
            games: vec![
                g("Test GB", vec![("Test GB.gb", &gb, All)]),
                g("CRC Only", vec![("CRC Only.gb", &crc_only, CrcOnly)]),
                g(
                    "Shared Hash (Rev A)",
                    vec![("Shared Hash (Rev A).gb", &shared, All)],
                ),
                g(
                    "Shared Hash (Rev B)",
                    vec![("Shared Hash (Rev B).gb", &shared, All)],
                ),
                g("MD5 Only", vec![("MD5 Only.gb", &md5_only, Md5Crc)]),
            ],
        },
        DatFile {
            file: "gba.dat",
            name: "Nintendo - Game Boy Advance",
            header_hint: None,
            games: vec![g("Test GBA", vec![("Test GBA.gba", &gba, All)])],
        },
        DatFile {
            file: "md.dat",
            name: "Sega - Mega Drive - Genesis",
            header_hint: None,
            games: vec![g("Test MD", vec![("Test MD.md", &md, All)])],
        },
    ];
    (fixtures, dats)
}

pub fn dat_xml(dat: &DatFile) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE datafile PUBLIC \"-//Logiqx//DTD ROM Management Datafile//EN\" \"http://www.logiqx.com/Dats/datafile.dtd\">\n<datafile>\n\t<header>\n",
    );
    s += &format!("\t\t<name>{}</name>\n", dat.name);
    s += &format!(
        "\t\t<description>{} (romforge test fixture)</description>\n",
        dat.name
    );
    s += "\t\t<version>1</version>\n\t\t<author>romforge fixtures</author>\n";
    if let Some(h) = dat.header_hint {
        s += &format!("\t\t<clrmamepro header=\"{h}\"/>\n");
    }
    s += "\t</header>\n";
    for game in &dat.games {
        s += &format!(
            "\t<game name=\"{0}\">\n\t\t<description>{0}</description>\n",
            game.name
        );
        for (name, body, which) in &game.roms {
            let h = hash_stream(&body[..], body.len() as u64).unwrap().raw;
            let hashes = match which {
                Hashes::All => format!("crc=\"{}\" md5=\"{}\" sha1=\"{}\"", h.crc32, h.md5, h.sha1),
                Hashes::Md5Crc => format!("crc=\"{}\" md5=\"{}\"", h.crc32, h.md5),
                Hashes::CrcOnly => format!("crc=\"{}\"", h.crc32),
            };
            s += &format!("\t\t<rom name=\"{name}\" size=\"{}\" {hashes}/>\n", body.len());
        }
        s += "\t</game>\n";
    }
    s + "</datafile>\n"
}
