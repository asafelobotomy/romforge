//! Milestone 10: cleanup & rename, on copies in temp folders. Every new
//! file is read back before an original is touched; undo puts everything
//! back.
//!
//! Discs: CHDs, cue sheets and shared tracks.

use romforge_core::cleanup::Format;
use romforge_core::hash::hash_stream;
use romforge_core::settings::Settings;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::cleanup::*;

#[test]
fn cd_tracks_become_a_chd_with_a_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let library = dir.path().join("library");
    std::fs::create_dir_all(source.join("Game")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    // A MODE2 data track and an audio track with its 2-second pregap stored.
    let mut data = pattern(1, 6 * 2352);
    data[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    data[15] = 2;
    let audio = pattern(2, 155 * 2352);
    let cue = "FILE \"Game (USA) (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"Game (USA) (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n";
    std::fs::write(source.join("Game/Game (USA) (Track 1).bin"), &data).unwrap();
    std::fs::write(source.join("Game/Game (USA) (Track 2).bin"), &audio).unwrap();
    std::fs::write(source.join("Game/Game (USA).cue"), cue).unwrap();
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    let dat = dir.path().join("psx.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Game (USA)">{}{}{}</game></datafile>"#,
            rom("Game (USA) (Track 1).bin", &data),
            rom("Game (USA) (Track 2).bin", &audio),
            rom("Game (USA).cue", cue.as_bytes()),
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        staging_dir: Some(dir.path().join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let library = library.canonicalize().unwrap();

    let plan = lib.cleanup_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let item = &plan.items[0];
    assert_eq!(item.format, Format::ChdCd);
    assert_eq!(item.target, library.join("PlayStation/Game (USA).chd"));
    assert_eq!(item.trash.len(), 3, "both tracks and the cue: {item:?}");
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);

    let chd = library.join("PlayStation/Game (USA).chd");
    let members = romforge_core::disc::chd_members(&chd).unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].1.raw, h(&data));
    assert_eq!(members[1].1.raw, h(&audio));
    let side = library.join("PlayStation/Game (USA)");
    assert!(side.join("Game (USA).romforge.md").exists());
    let rows = row_status(&lib, "Game (USA)");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, RowStatus::Verified);

    lib.cleanup_undo(None).unwrap();
    assert!(!chd.exists() && !side.exists());
    assert_eq!(
        std::fs::read(source.join("Game/Game (USA) (Track 2).bin")).unwrap(),
        audio
    );
    assert_eq!(
        std::fs::read_to_string(source.join("Game/Game (USA).cue")).unwrap(),
        cue
    );
}

/// A zip of two tracks and a cue sheet that isn't Redump's (rewritten with
/// CRLF, as some packs do). The cue that names exactly the tracks gives its
/// pregaps to the CHD and the zip goes; one that doesn't keeps the zip.
#[test]
fn a_cue_that_isnt_the_dats_counts_when_it_names_the_tracks() {
    let mut data = pattern(1, 6 * 2352);
    data[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    data[15] = 2;
    let audio = pattern(2, 155 * 2352);
    let redump = "FILE \"Game (USA) (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"Game (USA) (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n";
    let fits = redump.replace('\n', "\r\n");
    let other = fits.replace("Game (USA) (Track 2)", "Something Else");
    // The right files, but three TRACKs in two FILEs: a layout a CHD built
    // track by track would lose.
    let extra_track = fits.replace(
        "    INDEX 01 00:02:00\r\n",
        "    INDEX 01 00:02:00\r\n  TRACK 03 AUDIO\r\n    INDEX 01 00:04:00\r\n",
    );
    assert_ne!(extra_track, fits);
    for (cue, taken) in [
        (fits.as_str(), true),
        (other.as_str(), false),
        (extra_track.as_str(), false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let library = dir.path().join("library");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        let zip_path = source.join("Game (USA).zip");
        {
            use std::io::Write as _;
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            for (name, d) in [
                ("Game (USA) (Track 1).bin", &data[..]),
                ("Game (USA) (Track 2).bin", &audio[..]),
                ("Game (USA).cue", cue.as_bytes()),
            ] {
                z.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(d).unwrap();
            }
            z.finish().unwrap();
        }
        let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
        let rom = |name: &str, d: &[u8]| {
            let x = h(d);
            format!(
                r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
                x.size, x.crc32, x.md5, x.sha1
            )
        };
        let dat = dir.path().join("psx.dat");
        std::fs::write(
            &dat,
            format!(
                r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Game (USA)">{}{}{}</game></datafile>"#,
                rom("Game (USA) (Track 1).bin", &data),
                rom("Game (USA) (Track 2).bin", &audio),
                rom("Game (USA).cue", redump.as_bytes()),
            ),
        )
        .unwrap();
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_dat(&dat, None).unwrap();
        lib.save_settings(&Settings {
            source_dir: Some(source.clone()),
            library_dir: Some(library.clone()),
            staging_dir: Some(dir.path().join("staging")),
            ..Settings::default()
        })
        .unwrap();
        lib.scan(&source, Scope::All).unwrap();
        let (source, library) = (source.canonicalize().unwrap(), library.canonicalize().unwrap());

        let plan = lib.cleanup_plan(None).unwrap();
        assert_eq!(plan.items.len(), 1, "{plan:?}");
        let item = &plan.items[0];
        assert_eq!(item.parts.len(), 2, "the cue isn't the DAT's");
        assert_eq!(
            item.sheet.as_ref().map(|s| s.member.as_str()),
            Some("Game (USA).cue")
        );
        assert_eq!(item.trash, [source.join("Game (USA).zip")], "{item:?}");
        let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
        let chd = library.join("PlayStation/Game (USA).chd");
        if taken {
            assert!(report.failed.is_empty(), "{:?}", report.failed);
            assert_eq!(chd_pregaps(&chd), [0, 150]);
            assert_eq!(report.trashed, 1);
            assert!(!source.join("Game (USA).zip").exists());
        } else {
            // No cue fits two tracks: their pregaps are unknown, so no CHD
            // is written and the original stays.
            assert_eq!(report.failed.len(), 1, "{:?}", report.failed);
            assert!(report.failed[0].1.contains("pregaps"), "{:?}", report.failed);
            assert!(!chd.exists());
            assert_eq!(report.trashed, 0);
            assert!(source.join("Game (USA).zip").exists());
        }
    }
}

/// Two regions' discs share an identical audio track. Cleaning one of them
/// must not trash a file the other still needs.
#[test]
fn files_another_game_needs_are_never_trashed() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(&library).unwrap();
    let shared_audio = pattern(9, 155 * 2352);
    let mut games = Vec::new();
    for (game, seed) in [("Game (USA)", 1u8), ("Game (Europe)", 2u8)] {
        let d = source.join(game);
        std::fs::create_dir_all(&d).unwrap();
        let data = cd_tracks(seed);
        std::fs::write(d.join(format!("{game} (Track 1).bin")), &data).unwrap();
        std::fs::write(d.join(format!("{game} (Track 2).bin")), &shared_audio).unwrap();
        std::fs::write(d.join(format!("{game}.cue")), cue_for(game)).unwrap();
        games.push((
            game,
            vec![
                (format!("{game}.cue"), cue_for(game).into_bytes()),
                (format!("{game} (Track 1).bin"), data),
                (format!("{game} (Track 2).bin"), shared_audio.clone()),
            ],
        ));
    }
    let dat = disc_dat(dir.path(), "Sony - PlayStation", &games);
    let mut lib = lib_with(dir.path(), &dat, &source, &library);
    let usa = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "Game (USA)" && r.status == RowStatus::Verified)
        .unwrap()
        .id;
    let plan = lib.cleanup_plan(Some(&[usa])).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    // Europe's files are all where they were.
    for f in [
        "Game (Europe) (Track 1).bin",
        "Game (Europe) (Track 2).bin",
        "Game (Europe).cue",
    ] {
        assert!(source.join("Game (Europe)").join(f).exists(), "{f} was taken");
    }
    lib.scan(&source, Scope::All).unwrap();
    assert!(
        lib.library_view()
            .unwrap()
            .rows
            .iter()
            .any(|r| r.title == "Game (Europe)" && r.status == RowStatus::Verified),
        "Europe still complete"
    );
}

/// A disc of several tracks with no cue sheet anywhere is skipped: its
/// pregaps are unknown.
#[test]
fn multi_track_discs_need_a_cue() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(source.join("g")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let (data, audio) = (cd_tracks(3), pattern(4, 155 * 2352));
    std::fs::write(source.join("g/Game (USA) (Track 1).bin"), &data).unwrap();
    std::fs::write(source.join("g/Game (USA) (Track 2).bin"), &audio).unwrap();
    let dat = disc_dat(
        dir.path(),
        "Sony - PlayStation",
        &[(
            "Game (USA)",
            vec![
                ("Game (USA) (Track 1).bin".into(), data),
                ("Game (USA) (Track 2).bin".into(), audio),
            ],
        )],
    );
    let lib = lib_with(dir.path(), &dat, &source, &library);
    let plan = lib.cleanup_plan(None).unwrap();
    assert!(plan.items.is_empty(), "{plan:?}");
    assert!(
        plan.skipped
            .iter()
            .any(|(t, why)| t == "Game (USA)" && why.contains("cue sheet")),
        "{:?}",
        plan.skipped
    );
}
