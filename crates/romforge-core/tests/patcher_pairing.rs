//! Milestone 19: patching a game into a new copy, the original untouched.
//! Always on copies in temp folders.
//!
//! Patches paired with their games, and the list.

use std::io::Write as _;

use romforge_core::home::Home;
use romforge_core::view::{Relation, RowStatus};
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::patcher::*;

/// Patches a scan finds join their game: a BPS by the ROM it was made
/// from, an IPS beside the ROM of the same name, a zipped patch by its
/// readme's title. A ROM no DAT knows that a paired BPS beside it makes
/// from the verified game is named a translation of it, with no click,
/// never verified; taken off by the owner, it isn't named again.
#[test]
fn scanned_patches_pair_with_their_game() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), incoming.join("Test GB.gb")).unwrap();
    let data = std::fs::read(incoming.join("Test GB.gb")).unwrap();
    let mut english = data.clone();
    english[0x134..0x138].copy_from_slice(b"ENGL");
    // A BPS with a name that says nothing of the game, and the ROM it makes.
    std::fs::write(
        incoming.join("mtl_v1.bps"),
        bps(data.len() as u64, crc32fast::hash(&data), &english),
    )
    .unwrap();
    std::fs::write(incoming.join("Some download.gb"), &english).unwrap();
    // An IPS of the ROM's own name; a zip with a readme naming the game.
    std::fs::write(incoming.join("Test GB.ips"), ips(4, b"HARD")).unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(incoming.join("xyz123.zip")).unwrap());
    z.start_file("patch.ips", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&ips(8, b"EASY")).unwrap();
    z.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"=====\n  Test GB  \n=====\nAn easy-mode hack.\n")
        .unwrap();
    z.finish().unwrap();
    // Not for this game.
    std::fs::write(incoming.join("Other Game.ips"), ips(0, b"X")).unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let report = lib.scan(&incoming, Scope::All).unwrap();
    assert!(
        report.skipped_kinds.iter().any(|k| k.ext == "ips"),
        "patches aren't hashed as ROMs"
    );
    let base = row_id(&lib, "Test GB");
    let d = lib.game_detail(&base).unwrap();
    let by: Vec<(String, String)> = d
        .patches
        .iter()
        .map(|p| (p.name.clone(), p.paired_by.clone()))
        .collect();
    assert!(by.contains(&("mtl_v1.bps".into(), "source".into())), "{by:?}");
    assert!(by.contains(&("Test GB.ips".into(), "beside".into())), "{by:?}");
    assert!(by.contains(&("patch.ips".into(), "title".into())), "{by:?}");
    assert_eq!(d.patches.len(), 3, "{by:?}");
    let offered = lib.patches_beside(&base).unwrap();
    let other = offered
        .iter()
        .find(|p| p.name == "Other Game.ips")
        .expect("in the folder");
    assert_eq!(other.paired_by, "folder");
    let easy = d.patches.iter().find(|p| p.name == "patch.ips").unwrap();
    assert!(easy.path.ends_with("xyz123.zip"), "the zip is what's applied");
    assert_eq!(easy.relation, Relation::Hack);
    // Planned without saying: the guess from its names.
    let plan = lib.patch_plan(&base, &easy.path, None, None).unwrap();
    assert_eq!(plan.relation, Relation::Hack);

    // The download is the BPS applied to the verified game: named so.
    let rows = lib.library_view().unwrap().rows;
    let named = rows
        .iter()
        .find(|r| r.title == "Test GB [Hack: mtl v1]")
        .unwrap_or_else(|| panic!("{rows:#?}"));
    assert_eq!(named.status, RowStatus::Derivative, "never verified");
    let nd = lib.game_detail(&named.id).unwrap();
    assert_eq!(nd.proof.as_deref(), Some("mtl_v1.bps"));
    assert_eq!(nd.derived.as_ref().map(|x| x.base.as_str()), Some("Test GB"));

    // Taken off: it stays unknown, rescans too.
    lib.forget_identity(&named.id).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    assert!(
        !rows.iter().any(|r| r.title == "Test GB [Hack: mtl v1]"),
        "{rows:#?}"
    );

    // A patch that goes away is forgotten.
    std::fs::remove_file(incoming.join("Test GB.ips")).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let d = lib.game_detail(&row_id(&lib, "Test GB")).unwrap();
    assert_eq!(d.patches.len(), 2);
}

/// The list puts each game's original on top and its versions under
/// headings: other regions, editions, translations, hacks, and patch files
/// apart. A game you have only a patch of is still listed, as missing,
/// with the patch under it.
#[test]
fn the_list_shows_originals_with_their_versions() {
    use romforge_core::view::Category;
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let games = [
        ("Quest (USA)", pattern(1, 2048)),
        ("Quest (Japan)", pattern(2, 2048)),
        ("Quest (USA) (Limited Edition)", pattern(3, 2048)),
        ("Other Story (USA)", pattern(4, 2048)),
    ];
    let mut dat =
        String::from("<?xml version=\"1.0\"?><datafile><header><name>Nintendo - Game Boy</name></header>");
    for (name, d) in &games {
        let h = romforge_core::hash::hash_stream(&d[..], d.len() as u64)
            .unwrap()
            .raw;
        dat += &format!(
            "<game name=\"{name}\"><rom name=\"{name}.gb\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\"/></game>",
            d.len(),
            h.crc32,
            h.md5,
            h.sha1
        );
    }
    dat += "</datafile>";
    std::fs::write(tmp.path().join("gb.dat"), dat).unwrap();
    for (name, d) in &games[..3] {
        std::fs::write(incoming.join(format!("{name}.gb")), d).unwrap();
    }
    // A translation of the Japanese release, and the copy it made.
    let japan = &games[1].1;
    let mut english = japan.clone();
    english[0..4].copy_from_slice(b"ENGL");
    std::fs::write(
        incoming.join("English.bps"),
        bps(japan.len() as u64, crc32fast::hash(japan), &english),
    )
    .unwrap();
    std::fs::write(incoming.join("Quest translated.gb"), &english).unwrap();
    // A hack patch of the US release, beside it.
    std::fs::write(incoming.join("Quest (USA).ips"), ips(4, b"HARD")).unwrap();
    // A translation of a game you don't have, named by its readme.
    let mut z = zip::ZipWriter::new(std::fs::File::create(incoming.join("os-en.zip")).unwrap());
    z.start_file("os.ips", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&ips(0, b"EN")).unwrap();
    z.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"v1.0\n\nAn English translation of Other Story for the Game Boy.\n")
        .unwrap();
    z.finish().unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(tmp.path().join("gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let find = |t: &str| {
        rows.iter()
            .find(|r| r.title == t)
            .unwrap_or_else(|| panic!("no {t}: {rows:#?}"))
    };
    let usa = find("Quest (USA)");
    assert!(usa.main && usa.status == RowStatus::Verified);
    assert_eq!(usa.versions, 6, "{rows:#?}");
    let of = |t: &str| {
        let r = find(t);
        assert_eq!(r.group, usa.group, "{t}");
        assert!(!r.main, "{t}");
        r.category
    };
    assert_eq!(of("Quest (Japan)"), Some(Category::Region));
    assert_eq!(of("Quest (USA) (Limited Edition)"), Some(Category::Edition));
    assert_eq!(
        of("Quest (Japan) [Translation: English]"),
        Some(Category::Translation)
    );
    assert_eq!(of("English.bps"), Some(Category::PatchTranslation));
    assert_eq!(of("Quest (USA).ips"), Some(Category::PatchHack));
    assert_eq!(find("Quest (USA).ips").base_id.as_ref(), Some(&usa.id));
    assert_eq!(
        find("English.bps").base_id.as_ref(),
        Some(&find("Quest (Japan)").id),
        "for the release it was made from"
    );

    let other = find("Other Story (USA)");
    assert_eq!(other.status, RowStatus::Missing);
    assert!(other.main);
    let patch = find("os.ips");
    assert_eq!(patch.status, RowStatus::Patch);
    assert_eq!(patch.label.as_deref(), Some("os.ips (os-en.zip)"));
    assert_eq!(patch.group, other.group);
    assert_eq!(patch.category, Some(Category::PatchTranslation));
    assert_eq!(patch.base_id, None, "nothing to apply it to");
    let d = lib.game_detail(&patch.id).unwrap();
    assert_eq!(d.patches.len(), 1);
    let d = lib.game_detail(&other.id).unwrap();
    assert_eq!(d.patches.len(), 1, "the missing game lists its patch");
}
