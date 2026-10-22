//! Arcade sets go to the trash only when no other game needs them.
//! Always on copies in temp folders.

use std::io::Write;
use std::path::Path;

use romforge_core::home::Home;
use romforge_core::{Library, Scope};

fn zip(path: &Path, members: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in members {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

fn rom(name: &str, d: &[u8]) -> String {
    let x = romforge_core::hash::hash_stream(d, d.len() as u64).unwrap().raw;
    format!(
        "<rom name=\"{name}\" size=\"{}\" crc=\"{}\" sha1=\"{}\"/>",
        d.len(),
        x.crc32,
        x.sha1
    )
}

/// Importing one game of a BIOS takes the BIOS zip along, but its
/// original stays while another game (not imported) still needs it.
#[test]
fn a_bios_another_game_needs_is_not_trashed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("arcade");
    std::fs::create_dir_all(&incoming).unwrap();
    let (bios, a, b) = (vec![1u8; 1024], vec![2u8; 2048], vec![3u8; 2048]);
    zip(&incoming.join("neogeo.zip"), &[("sp-s2.sp1", &bios)]);
    zip(&incoming.join("gamea.zip"), &[("a.p1", &a)]);
    zip(&incoming.join("gameb.zip"), &[("b.p1", &b)]);
    let xml = format!(
        "<?xml version=\"1.0\"?>\n<mame build=\"0.289\">\n\
         <machine name=\"neogeo\" isbios=\"yes\"><description>Neo-Geo</description>{}</machine>\n\
         <machine name=\"gamea\" romof=\"neogeo\"><description>Game A</description>{}{}</machine>\n\
         <machine name=\"gameb\" romof=\"neogeo\"><description>Game B</description>{}{}</machine>\n</mame>\n",
        rom("sp-s2.sp1", &bios),
        rom("sp-s2.sp1", &bios),
        rom("a.p1", &a),
        rom("sp-s2.sp1", &bios),
        rom("b.p1", &b),
    );
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat_bytes(xml.as_bytes(), "mame -listxml (0.289)", Some("Arcade"))
        .unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let id = rows.iter().find(|r| r.title == "gamea").unwrap().id.clone();
    let plan = lib.cleanup_plan(Some(&[id])).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let it = &plan.items[0];
    assert!(it.parts.iter().any(|p| p.name == "neogeo.zip"), "{it:?}");
    assert_eq!(it.trash, [incoming.join("gamea.zip")], "{it:?}");
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert!(incoming.join("neogeo.zip").is_file(), "Game B still needs it");
    assert!(incoming.join("gameb.zip").is_file());
    assert!(!incoming.join("gamea.zip").exists());
    assert!(home.games().join("Arcade/neogeo.zip").is_file());
}
