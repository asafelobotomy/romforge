//! Milestone 19: patching a game into a new copy, the original untouched.
//! Always on copies in temp folders.
//!
//! Discs.

use romforge_core::home::Home;
use romforge_core::view::{Relation, RowStatus};
use romforge_core::{Library, Scope};

mod common;
use common::patcher::*;

/// A disc in a CHD: an xdelta (or a PPF) patches its data track, the
/// audio track and its pregap come through, and the result is a new CHD.
#[test]
fn a_disc_patch_makes_a_new_chd() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let chd = incoming.join("Game.chd");
    let (data, audio) = write_cd(&chd);
    let rom = |name: &str, d: &[u8]| {
        let h = romforge_core::hash::hash_stream(d, d.len() as u64).unwrap().raw;
        format!(
            "<rom name=\"{name}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\"/>",
            d.len(),
            h.crc32,
            h.md5,
            h.sha1
        )
    };
    let dat = tmp.path().join("ps1.dat");
    std::fs::write(
        &dat,
        format!(
            "<?xml version=\"1.0\"?><datafile><header><name>Sony - PlayStation</name></header>\
             <game name=\"Game (Japan)\">{}{}</game></datafile>",
            rom("Game (Japan) (Track 1).bin", &data),
            rom("Game (Japan) (Track 2).bin", &audio)
        ),
    )
    .unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let base = row_id(&lib, "Game (Japan)");

    let mut english = data.clone();
    english[100..107].copy_from_slice(b"ENGLISH");
    let mut delta = Vec::new();
    oxidelta::engine::encode(&data, &english, &mut delta).unwrap();
    let patch = tmp.path().join("Game English.xdelta");
    std::fs::write(&patch, &delta).unwrap();
    let plan = lib
        .patch_plan(&base, &patch, Some(Relation::Translation), Some("English"))
        .unwrap();
    assert!(plan.disc);
    let done = lib.patch_apply(&plan).unwrap();
    let target = home
        .games()
        .join("PlayStation/Game (Japan) [Translation- English].chd");
    assert_eq!(done.target, target);
    let tracks = romforge_core::disc::chd_members(&target).unwrap();
    assert_eq!(tracks.len(), 2, "{tracks:?}");
    assert_eq!(tracks[0].1.raw.sha1, sha1(&english), "the data track patched");
    assert_eq!(
        tracks[1].1.raw.sha1,
        sha1(&audio),
        "the audio track and its pregap kept"
    );
    assert_eq!(done.sha1, sha1(&english));
    let before = romforge_core::disc::chd_members(&chd).unwrap();
    assert_eq!(before[0].1.raw.sha1, sha1(&data), "the original untouched");
    let rows = lib.library_view().unwrap().rows;
    let copy = rows.iter().find(|r| r.title == plan.title).expect("listed");
    assert_eq!(copy.status, RowStatus::Derivative);

    // A PPF on the same disc, as a hack.
    let mut ppf = b"PPF30".to_vec();
    ppf.push(2);
    ppf.extend_from_slice(b"Hard Mode");
    ppf.extend_from_slice(&[b' '; 41]);
    ppf.extend_from_slice(&[0, 0, 0, 0]);
    ppf.extend_from_slice(&200u64.to_le_bytes());
    ppf.push(4);
    ppf.extend_from_slice(b"HARD");
    let ppf_path = tmp.path().join("hard.ppf");
    std::fs::write(&ppf_path, &ppf).unwrap();
    let plan = lib
        .patch_plan(&base, &ppf_path, Some(Relation::Hack), None)
        .unwrap();
    assert_eq!(
        plan.title, "Game (Japan) [Hack: Hard Mode]",
        "the PPF's own description"
    );
    let done = lib.patch_apply(&plan).unwrap();
    let tracks = romforge_core::disc::chd_members(&done.target).unwrap();
    let mut hard = data.clone();
    hard[200..204].copy_from_slice(b"HARD");
    assert_eq!(tracks[0].1.raw.sha1, sha1(&hard));

    // An xdelta made from the second track: the first refuses it (its
    // checksums don't match), so it goes on the second.
    let mut remix = audio.clone();
    remix[5000..5005].copy_from_slice(b"REMIX");
    let mut delta = Vec::new();
    oxidelta::engine::encode(&audio, &remix, &mut delta).unwrap();
    let patch = tmp.path().join("remix.xdelta");
    std::fs::write(&patch, &delta).unwrap();
    let plan = lib.patch_plan(&base, &patch, Some(Relation::Hack), None).unwrap();
    let done = lib.patch_apply(&plan).unwrap();
    let tracks = romforge_core::disc::chd_members(&done.target).unwrap();
    assert_eq!(tracks[0].1.raw.sha1, sha1(&data), "the first track as it was");
    assert_eq!(tracks[1].1.raw.sha1, sha1(&remix), "the second track patched");
    assert_eq!(done.sha1, sha1(&remix));
}
