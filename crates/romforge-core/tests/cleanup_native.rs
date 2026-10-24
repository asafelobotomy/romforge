//! Milestone 23: Convert writes Z3DS for a decrypted 3DS game and a WUA for
//! a Wii U title folder, each read back before the original goes to the
//! trash; undo puts the originals back. An encrypted 3DS game stays and
//! says why.

use romforge_core::cleanup::Format;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::cleanup::*;

/// A card image: NCSD, its NCCH at 0x4000 with NoCrypto set when `plain`.
fn card(plain: bool, seed: u8) -> Vec<u8> {
    let mut img = pattern(seed, 0x60000);
    img[..0x200].fill(0);
    img[0x100..0x104].copy_from_slice(b"NCSD");
    img[0x120..0x124].copy_from_slice(&0x20u32.to_le_bytes());
    img[0x4000..0x4200].fill(0);
    img[0x4100..0x4104].copy_from_slice(b"NCCH");
    img[0x4150..0x415A].copy_from_slice(b"CTR-P-AREE");
    if plain {
        img[0x418F] = 0x04;
    }
    img
}

#[test]
fn convert_makes_z3ds_of_decrypted_3ds_games() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(source.join("3ds")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let (dec, enc) = (card(true, 3), card(false, 4));
    std::fs::write(source.join("3ds/dec.3ds"), &dec).unwrap();
    std::fs::write(source.join("3ds/enc.3ds"), &enc).unwrap();
    let dat = disc_dat(
        dir.path(),
        "Nintendo - Nintendo 3DS",
        &[
            (
                "Decrypted (USA)",
                vec![("Decrypted (USA).3ds".into(), dec.clone())],
            ),
            ("Encrypted (USA)", vec![("Encrypted (USA).3ds".into(), enc)]),
        ],
    );
    let mut lib = lib_with(dir.path(), &dat, &source, &library);
    let d = source.join("3ds").canonicalize().unwrap();
    let plan = lib.convert_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(
        (plan.items[0].format, plan.items[0].target.clone()),
        (Format::Z3ds, d.join("dec.zcci"))
    );
    assert!(
        plan.skipped
            .iter()
            .any(|(t, why)| t == "Encrypted (USA)" && why.contains("encrypted")),
        "{:?}",
        plan.skipped
    );
    let report = lib.convert_apply(None, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(d.join("dec.zcci").exists() && !d.join("dec.3ds").exists());
    assert!(
        row_status(&lib, "Decrypted (USA)")
            .iter()
            .any(|(s, _)| *s == RowStatus::Verified),
        "the Z3DS verifies"
    );
    assert!(lib.convert_plan(None).unwrap().items.is_empty(), "nothing more");
    lib.cleanup_undo(None).unwrap();
    assert_eq!(
        std::fs::read(d.join("dec.3ds")).unwrap(),
        dec,
        "undo puts it back"
    );
    assert!(!d.join("dec.zcci").exists());
}

#[test]
fn convert_makes_a_wua_of_a_wii_u_title_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    let title = source.join("wiiu/Splatoon");
    for d in ["code", "content/Common", "meta"] {
        std::fs::create_dir_all(title.join(d)).unwrap();
    }
    std::fs::create_dir_all(&library).unwrap();
    std::fs::write(title.join("code/Gambit.rpx"), pattern(9, 70_000)).unwrap();
    std::fs::write(title.join("content/Common/Pack.bin"), pattern(1, 200_000)).unwrap();
    std::fs::write(
        title.join("meta/meta.xml"),
        "<menu><product_code>WUP-P-AGMP</product_code><company_code>0001</company_code>\
         <title_id>0005000010176A00</title_id><title_version>288</title_version></menu>",
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.save_settings(&romforge_core::settings::Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        staging_dir: Some(dir.path().join("staging")),
        ..Default::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    // No DAT of Wii U games: the user names it.
    let row = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title.contains("Gambit"))
        .unwrap();
    lib.identify_as(&row.id, "Wii U", "Splatoon (Europe)").unwrap();
    let wiiu = source.join("wiiu").canonicalize().unwrap();
    let plan = lib.convert_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(
        (plan.items[0].format, plan.items[0].target.clone()),
        (Format::Wua, wiiu.join("Splatoon.wua"))
    );
    let report = lib.convert_apply(None, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(wiiu.join("Splatoon.wua").exists());
    assert!(
        !wiiu.join("Splatoon").exists(),
        "the folder went to the trash whole"
    );
    // Your name for it moved to the WUA.
    lib.scan(&source, Scope::All).unwrap();
    let rows = row_status(&lib, "Splatoon (Europe)");
    assert!(
        rows.iter()
            .any(|(s, f)| *s == RowStatus::ByYou && f.ends_with("Splatoon.wua")),
        "{rows:?}"
    );
    lib.cleanup_undo(None).unwrap();
    assert!(
        wiiu.join("Splatoon/content/Common/Pack.bin").exists(),
        "undo puts it back"
    );
    assert!(!wiiu.join("Splatoon.wua").exists());

    // A name longer than a WUA can hold: the folder stays, and says why.
    let long = "x".repeat(130);
    std::fs::write(wiiu.join("Splatoon/content").join(&long), b"x").unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let plan = lib.convert_plan(None).unwrap();
    assert!(plan.items.is_empty(), "{plan:?}");
    assert!(
        plan.skipped
            .iter()
            .any(|(_, why)| why.contains("longer than a WUA can hold")),
        "{:?}",
        plan.skipped
    );
}
