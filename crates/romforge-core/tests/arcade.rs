//! Milestone 17: arcade sets (MAME, FBNeo). Always on copies in temp
//! folders.

use std::io::Write;
use std::path::{Path, PathBuf};

use romforge_core::cleanup::Format;
use romforge_core::home::Home;
use romforge_core::launch::Env;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

fn h(d: &[u8]) -> romforge_core::hash::Digest {
    romforge_core::hash::hash_stream(d, d.len() as u64).unwrap().raw
}

fn zip(path: &Path, members: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in members {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// A Neo Geo BIOS, a game on it, a split clone and a device: MAME's
/// `-listxml` (SHA1 per ROM) and FBNeo's DAT (CRC32 only).
struct Sets {
    bios: Vec<u8>,
    p1: Vec<u8>,
    s1: Vec<u8>,
    clone_p1: Vec<u8>,
}

impl Sets {
    fn new() -> Self {
        Sets {
            bios: vec![1; 1024],
            p1: (0..4096u32).map(|i| (i % 253) as u8).collect(),
            s1: vec![7; 2048],
            clone_p1: (0..4096u32).map(|i| (i % 241) as u8).collect(),
        }
    }

    fn rom(name: &str, d: &[u8], sha1: bool, extra: &str) -> String {
        let x = h(d);
        if sha1 {
            format!(
                "<rom name=\"{name}\" size=\"{}\" crc=\"{}\" sha1=\"{}\"{extra}/>",
                d.len(),
                x.crc32,
                x.sha1
            )
        } else {
            format!(
                "<rom name=\"{name}\" size=\"{}\" crc=\"{}\"{extra}/>",
                d.len(),
                x.crc32
            )
        }
    }

    fn mame(&self) -> String {
        let r = |n: &str, d: &[u8], e: &str| Self::rom(n, d, true, e);
        format!(
            r#"<?xml version="1.0"?>
<mame build="0.289 (mame0289)">
<machine name="neogeo" isbios="yes"><description>Neo-Geo MV-6F</description>{}</machine>
<machine name="mslug" romof="neogeo"><description>Metal Slug - Super Vehicle-001</description>{}{}{}
  <rom name="bad.rom" size="1024" status="nodump"/>{}</machine>
<machine name="mslugj" cloneof="mslug" romof="mslug"><description>Metal Slug (Japan)</description>{}{}{}</machine>
<machine name="ym2610" isdevice="yes"><description>YM2610</description>{}</machine>
</mame>
"#,
            r("sp-s2.sp1", &self.bios, ""),
            r("sp-s2.sp1", &self.bios, " bios=\"euro\" merge=\"sp-s2.sp1\""),
            r("201-p1.p1", &self.p1, ""),
            r("201-s1.s1", &self.s1, ""),
            Self::rom("opt.rom", &[9; 64], true, " optional=\"yes\""),
            r("sp-s2.sp1", &self.bios, " bios=\"euro\" merge=\"sp-s2.sp1\""),
            r("201-p1j.p1", &self.clone_p1, ""),
            r("201-s1.s1", &self.s1, " merge=\"201-s1.s1\""),
            r("dev.bin", &[5; 64], ""),
        )
    }

    fn fbneo(&self) -> String {
        let r = |n: &str, d: &[u8]| Self::rom(n, d, false, "");
        format!(
            "<?xml version=\"1.0\"?>\n<datafile><header><name>FinalBurn Neo - Arcade Games</name></header>\n\
             <game name=\"mslug\"><description>Metal Slug - Super Vehicle-001</description>{}{}{}</game>\n</datafile>\n",
            r("sp-s2.sp1", &self.bios),
            r("201-p1.p1", &self.p1),
            r("201-s1.s1", &self.s1),
        )
    }

    /// Split sets: the BIOS, the parent, the clone with only its own ROM.
    fn write(&self, dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        zip(&dir.join("neogeo.zip"), &[("sp-s2.sp1", &self.bios)]);
        zip(
            &dir.join("mslug.zip"),
            &[("201-p1.p1", &self.p1), ("201-s1.s1", &self.s1)],
        );
        zip(&dir.join("mslugj.zip"), &[("201-p1j.p1", &self.clone_p1)]);
    }
}

#[test]
fn mame_sets_are_matched_imported_whole_and_started_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let sets = Sets::new();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("arcade");
    sets.write(&incoming);
    let before = files_under(&incoming);
    let mut lib = Library::open(home.db()).unwrap();
    // Scanned before the DAT: the ROMs have no extension romforge knows.
    lib.scan(&incoming, Scope::All).unwrap();
    let mame = lib
        .import_dat_bytes(sets.mame().as_bytes(), "mame -listxml (0.289)", Some("Arcade"))
        .unwrap();
    assert_eq!(
        (mame.name.as_str(), mame.games),
        ("MAME", 3),
        "the device isn't a game"
    );
    // Importing the DAT marked the set zips for a full read.
    lib.scan(&incoming, Scope::All).unwrap();

    let rows = lib.library_view().unwrap().rows;
    for (set, label) in [
        ("mslug", "Metal Slug - Super Vehicle-001"),
        ("mslugj", "Metal Slug (Japan)"),
        ("neogeo", "Neo-Geo MV-6F"),
    ] {
        let r = rows
            .iter()
            .find(|r| r.title == set)
            .unwrap_or_else(|| panic!("{set}: {rows:?}"));
        assert_eq!(
            r.status,
            RowStatus::Verified,
            "{set}: nodump and optional ROMs aside"
        );
        assert_eq!(r.label.as_deref(), Some(label));
        assert_eq!(r.system.as_deref(), Some("Arcade"));
    }

    let group = |set: &str| rows.iter().find(|r| r.title == set).unwrap().group.clone();
    assert!(
        group("mslug").is_some() && group("mslug") == group("mslugj"),
        "a clone with its parent"
    );

    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert!(plan.items.iter().all(|i| i.format == Format::Set), "{plan:?}");
    let moved: Vec<String> = plan
        .items
        .iter()
        .flat_map(|i| i.parts.iter().map(|p| p.name.clone()))
        .collect();
    let mut sorted = moved.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        ["mslug.zip", "mslugj.zip", "neogeo.zip"],
        "each zip once: {moved:?}"
    );
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    let games = home.games().join("Arcade");
    for z in ["mslug.zip", "mslugj.zip", "neogeo.zip"] {
        assert!(games.join(z).is_file(), "{z}");
    }
    assert!(files_under(&incoming).is_empty(), "originals to the trash");

    // Found again in Games/, and started there by set name.
    lib.refresh(&Default::default(), |_, _| {}).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let clone = rows.iter().find(|r| r.title == "mslugj").unwrap();
    assert_eq!(clone.status, RowStatus::Verified);
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("mame"), "#!/bin/sh\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(bin.join("mame"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = Env {
        path: vec![bin],
        core_dirs: vec![],
        flatpak_dirs: vec![],
        ..Default::default()
    };
    let spec = lib.launch_row(&env, &clone.id, Some("mame")).unwrap();
    assert_eq!(
        spec.args,
        [
            "-nowindow".to_owned(),
            "-rompath".to_owned(),
            games.display().to_string(),
            "mslugj".to_owned()
        ]
    );
    assert_eq!(spec.rom, games.join("mslugj.zip"), "the clone's own zip");

    lib.cleanup_undo(None).unwrap();
    assert_eq!(files_under(&incoming), before);
}

/// FBNeo's DAT has only CRC32: a set counts when MAME's DAT confirms the
/// same ROMs by SHA1; without MAME's, it stays a suggestion.
#[test]
fn fbneo_sets_are_verified_through_mame() {
    let tmp = tempfile::tempdir().unwrap();
    let sets = Sets::new();
    let dir: PathBuf = tmp.path().join("arcade");
    sets.write(&dir);
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    lib.import_dat_bytes(sets.fbneo().as_bytes(), "fbneo.dat", Some("FBNeo"))
        .unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let fb = |lib: &Library| {
        lib.library_view()
            .unwrap()
            .rows
            .into_iter()
            .find(|r| r.system.as_deref() == Some("FBNeo") && r.title == "mslug")
    };
    assert!(
        fb(&lib).is_none_or(|r| r.status != RowStatus::Verified),
        "CRC32 alone is never a match"
    );
    lib.import_dat_bytes(sets.mame().as_bytes(), "mame -listxml (0.289)", Some("Arcade"))
        .unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let r = fb(&lib).expect("FBNeo's mslug");
    assert_eq!(r.status, RowStatus::Verified, "confirmed by MAME's SHA1");
    assert_eq!(r.label.as_deref(), Some("Metal Slug - Super Vehicle-001"));
}

/// Only an arcade DAT's ROM confirms an FBNeo one: the same bytes listed
/// with a SHA1 by a DAT of another system don't make a CRC32-only ROM a
/// match (house rule 8).
#[test]
fn fbneo_is_not_confirmed_by_a_non_arcade_dat() {
    let tmp = tempfile::tempdir().unwrap();
    let sets = Sets::new();
    let dir: PathBuf = tmp.path().join("arcade");
    sets.write(&dir);
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    lib.import_dat_bytes(sets.fbneo().as_bytes(), "fbneo.dat", Some("FBNeo"))
        .unwrap();
    let r = |n: &str, d: &[u8]| Sets::rom(n, d, true, "");
    let other = format!(
        "<?xml version=\"1.0\"?>\n<datafile><header><name>Nintendo - Game Boy</name></header>\n\
         <game name=\"same bytes\"><description>same bytes</description>{}{}{}</game>\n</datafile>\n",
        r("a.gb", &sets.bios),
        r("b.gb", &sets.p1),
        r("c.gb", &sets.s1),
    );
    lib.import_dat_bytes(other.as_bytes(), "gb.dat", Some("Game Boy"))
        .unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let fb = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.system.as_deref() == Some("FBNeo") && r.title == "mslug");
    assert!(
        fb.is_none_or(|r| r.status != RowStatus::Verified),
        "a Game Boy DAT's SHA1 confirms no arcade ROM"
    );
}

/// A game with a disk: its CHD (in a folder named after the set) is
/// matched by the SHA1 its header keeps, and imported with the set.
#[test]
fn a_disk_is_matched_by_the_chd_s_own_sha1() {
    use chd::write::{ChdWriteOptions, ChdWriter};
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let dir = tmp.path().join("arcade");
    std::fs::create_dir_all(dir.join("kinst")).unwrap();
    let rom = vec![3u8; 2048];
    zip(&dir.join("kinst.zip"), &[("u98-l15d", &rom)]);
    let data: Vec<u8> = (0..2048u32 * 4).map(|i| (i % 251) as u8).collect();
    let chd = dir.join("kinst").join("kinst.chd");
    let mut w = ChdWriter::new(
        std::fs::File::create(&chd).unwrap(),
        ChdWriteOptions::dvd().logical_bytes(data.len() as u64),
    )
    .unwrap();
    w.write_all(&data).unwrap();
    w.finish().unwrap();
    let disk_sha1 = romforge_core::disc::chd_sha1(&chd).expect("a v5 CHD keeps its SHA1");
    let r = h(&rom);
    let xml = format!(
        "<?xml version=\"1.0\"?>\n<mame build=\"0.289\">\n<machine name=\"kinst\"><description>Killer Instinct (v1.5d)</description>\
         <rom name=\"u98-l15d\" size=\"{}\" crc=\"{}\" sha1=\"{}\"/><disk name=\"kinst\" sha1=\"{disk_sha1}\"/></machine>\n</mame>\n",
        rom.len(),
        r.crc32,
        r.sha1
    );
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat_bytes(xml.as_bytes(), "mame -listxml (0.289)", Some("Arcade"))
        .unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let row = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "kinst")
        .unwrap();
    assert_eq!(row.status, RowStatus::Verified, "the ROM and the disk");

    let plan = lib.import_plan(Some(&dir)).unwrap();
    let names: Vec<&str> = plan.items[0].parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["kinst.zip", "kinst/kinst.chd"]);
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert!(home.games().join("Arcade/kinst/kinst.chd").is_file());
    lib.cleanup_undo(None).unwrap();
    assert!(
        chd.is_file() && !home.games().join("Arcade/kinst").exists(),
        "undo removes the set folder"
    );
}
