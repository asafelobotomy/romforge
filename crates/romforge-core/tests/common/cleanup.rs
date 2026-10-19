//! Helpers from the `cleanup` tests.

use std::path::{Path, PathBuf};

use romforge_core::hash::hash_stream;
use romforge_core::settings::Settings;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

use super::fixtures;

pub struct Setup {
    pub _dir: tempfile::TempDir,
    pub source: PathBuf,
    pub library: PathBuf,
    pub lib: Library,
}

/// A Source folder with copies of some fixtures, an empty Library, and the
/// DATs imported.
pub fn setup(keep_originals: bool, files: &[(&str, &str)], dats: &[&str]) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let library = dir.path().join("library");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    for (from, to) in files {
        let to = source.join(to);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(fixtures().join("roms").join(from), to).unwrap();
    }
    let mut lib = Library::open_in_memory().unwrap();
    for d in dats {
        lib.import_dat(fixtures().join("dats").join(d), None).unwrap();
    }
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        keep_originals,
        staging_dir: Some(dir.path().join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let (source, library) = (source.canonicalize().unwrap(), library.canonicalize().unwrap());
    Setup {
        _dir: dir,
        source,
        library,
        lib,
    }
}

pub fn zip_names(path: &Path) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    names.sort();
    names
}

pub fn row_status(lib: &Library, title: &str) -> Vec<(RowStatus, String)> {
    let v = lib.library_view().unwrap();
    v.rows
        .iter()
        .filter(|r| r.title == title)
        .map(|r| {
            let d = lib.game_detail(&r.id).unwrap();
            (
                r.status,
                d.files.first().map(|f| f.path.clone()).unwrap_or_default(),
            )
        })
        .collect()
}

pub fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// Pregaps stored in a CHD, by track.
pub fn chd_pregaps(path: &Path) -> Vec<u64> {
    let mut chd = chd::Chd::open(std::io::BufReader::new(std::fs::File::open(path).unwrap()), None).unwrap();
    let refs: Vec<_> = chd.metadata_refs().collect::<Result<_, _>>().unwrap();
    let mut out = Vec::new();
    for r in refs {
        let m = r.read(chd.inner()).unwrap();
        if let Some(t) = romforge_core::disc::parse_track(&String::from_utf8_lossy(&m.value)) {
            out.push(t.pregap);
        }
    }
    out
}

/// A game for [`disc_dat`]: its name and its ROMs as (name, bytes).
pub type DatGame<'a> = (&'a str, Vec<(String, Vec<u8>)>);

/// A DAT for `games`.
pub fn disc_dat(dir: &Path, system: &str, games: &[DatGame]) -> PathBuf {
    let mut xml = format!("<datafile><header><name>{system}</name></header>");
    for (name, roms) in games {
        xml += &format!(r#"<game name="{name}">"#);
        for (rom, data) in roms {
            let x = hash_stream(&data[..], data.len() as u64).unwrap().raw;
            xml += &format!(
                r#"<rom name="{rom}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
                x.size, x.crc32, x.md5, x.sha1
            );
        }
        xml += "</game>";
    }
    xml += "</datafile>";
    let p = dir.join(format!("{system}.dat"));
    std::fs::write(&p, xml).unwrap();
    p
}

pub fn cd_tracks(seed: u8) -> Vec<u8> {
    let mut data = pattern(seed, 6 * 2352);
    data[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    data[15] = 2;
    data
}

pub fn cue_for(game: &str) -> String {
    format!(
        "FILE \"{game} (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"{game} (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n"
    )
}

pub fn lib_with(dir: &Path, dat: &Path, source: &Path, library: &Path) -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dat, None).unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.to_path_buf()),
        library_dir: Some(library.to_path_buf()),
        staging_dir: Some(dir.join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(source, Scope::All).unwrap();
    lib
}
