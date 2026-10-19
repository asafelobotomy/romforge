//! Milestone 10: cleanup & rename, on copies in temp folders. Every new
//! file is read back before an original is touched; undo puts everything
//! back.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use romforge_core::cleanup::Format;
use romforge_core::hash::hash_stream;
use romforge_core::layout::TRASH_DIR;
use romforge_core::settings::Settings;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

struct Setup {
    _dir: tempfile::TempDir,
    source: PathBuf,
    library: PathBuf,
    lib: Library,
}

/// A Source folder with copies of some fixtures, an empty Library, and the
/// DATs imported.
fn setup(keep_originals: bool, files: &[(&str, &str)], dats: &[&str]) -> Setup {
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

fn zip_names(path: &Path) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    names.sort();
    names
}

fn row_status(lib: &Library, title: &str) -> Vec<(RowStatus, String)> {
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

#[test]
fn cartridges_are_zipped_under_their_dat_names_and_undo_restores() {
    let mut s = setup(
        false,
        &[
            ("gb/Test GB.gb", "loose/tgb.gb"),
            ("archives/Test GBA.zip", "Test GBA.zip"),
            ("archives/Mixed.7z", "Mixed.7z"),
        ],
        &["gb.dat", "gba.dat", "md.dat", "nes.dat"],
    );
    let plan = s.lib.cleanup_plan(None).unwrap();
    let titles: Vec<&str> = plan.items.iter().map(|i| i.title.as_str()).collect();
    assert!(
        titles.contains(&"Test GB") && titles.contains(&"Test MD"),
        "{titles:?} {:?}",
        plan.skipped
    );
    assert!(plan.items.iter().all(|i| i.format == Format::Zip));
    // Mixed.7z holds notes.txt too, so it stays; Test GBA.zip has a .url.
    let md = plan.items.iter().find(|i| i.title == "Test MD").unwrap();
    assert!(
        md.trash.is_empty() && md.kept == [s.source.join("Mixed.7z")],
        "{md:?}"
    );
    let gb = plan.items.iter().find(|i| i.title == "Test GB").unwrap();
    assert_eq!(gb.target, s.library.join("Game Boy/Test GB.zip"));
    assert_eq!(gb.trash, [s.source.join("loose/tgb.gb")]);
    let original = std::fs::read(s.source.join("loose/tgb.gb")).unwrap();

    let report = s.lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.done, plan.items.len());
    assert_eq!(report.trashed, 1);

    // The new zip: the ROM under its DAT name and the details file.
    let zip = s.library.join("Game Boy/Test GB.zip");
    assert_eq!(zip_names(&zip), ["Test GB.gb", "romforge/Test GB.romforge.md"]);
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    let mut md_text = String::new();
    z.by_name("romforge/Test GB.romforge.md")
        .unwrap()
        .read_to_string(&mut md_text)
        .unwrap();
    assert!(
        md_text.starts_with("# Test GB\n") && md_text.contains("verified by hash"),
        "{md_text}"
    );
    let mut rom = Vec::new();
    z.by_name("Test GB.gb").unwrap().read_to_end(&mut rom).unwrap();
    assert_eq!(rom, original);
    // A 7z member came out whole too.
    let md_zip = s.library.join("Genesis/Test MD.zip");
    assert!(zip_names(&md_zip).contains(&"Test MD.md".to_owned()));

    // The original is in the trash, not gone; the library now shows the
    // new copy, verified, and nothing from the trash.
    assert!(!s.source.join("loose/tgb.gb").exists());
    let trashed = s
        .source
        .join("loose")
        .join(TRASH_DIR)
        .join(report.batch_id.to_string())
        .join("tgb.gb");
    assert_eq!(std::fs::read(&trashed).unwrap(), original);
    assert!(s.source.join("Mixed.7z").exists());
    let rows = row_status(&s.lib, "Test GB");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, RowStatus::Verified);
    assert!(rows[0].1.starts_with(&*zip.to_string_lossy()), "{rows:?}");
    // Nothing left to do for it.
    let again = s.lib.cleanup_plan(None).unwrap();
    assert!(again.items.iter().all(|i| i.title != "Test GB"), "{again:?}");
    assert_eq!(s.lib.cleanup_batches().unwrap().len(), 1);

    let undone = s.lib.cleanup_undo(None).unwrap();
    assert!(undone.failed.is_empty(), "{:?}", undone.failed);
    assert_eq!(undone.restored, 1);
    assert_eq!(std::fs::read(s.source.join("loose/tgb.gb")).unwrap(), original);
    assert!(!zip.exists() && !md_zip.exists());
    assert!(
        !s.source.join("loose").join(TRASH_DIR).exists(),
        "empty trash folders go"
    );
    let rows = row_status(&s.lib, "Test GB");
    assert!(rows[0].1.ends_with("loose/tgb.gb"), "{rows:?}");
    assert!(s.lib.cleanup_batches().unwrap()[0].undone_at.is_some());
}

#[test]
fn keep_originals_leaves_source_alone_and_nothing_is_overwritten() {
    let mut s = setup(
        true,
        &[("gb/Test GB.gb", "Test GB.gb"), ("gb/MD5 Only.gb", "MD5 Only.gb")],
        &["gb.dat"],
    );
    // Something already has MD5 Only's name in the Library.
    std::fs::create_dir_all(s.library.join("Game Boy")).unwrap();
    std::fs::write(s.library.join("Game Boy/MD5 Only.zip"), b"mine").unwrap();
    let plan = s.lib.cleanup_plan(None).unwrap();
    assert!(plan.keep_originals);
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert!(
        plan.skipped
            .iter()
            .any(|(t, why)| t == "MD5 Only" && why.contains("already exists"))
    );
    let report = s.lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!((report.done, report.trashed), (1, 0));
    assert!(s.source.join("Test GB.gb").exists());
    assert!(!s.source.join(TRASH_DIR).exists());
    assert_eq!(
        std::fs::read(s.library.join("Game Boy/MD5 Only.zip")).unwrap(),
        b"mine"
    );
    assert!(s.library.join("Game Boy/Test GB.zip").exists());
    // Staging is cleaned up after the batch.
    let staging = s.source.parent().unwrap().join("staging");
    assert!(!staging.exists() || std::fs::read_dir(&staging).unwrap().next().is_none());
}

fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

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

#[test]
fn cleanup_needs_a_source_folder() {
    let lib = Library::open_in_memory().unwrap();
    assert!(lib.cleanup_plan(None).is_err());
}

/// Pregaps stored in a CHD, by track.
fn chd_pregaps(path: &Path) -> Vec<u64> {
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

/// A game for [`disc_dat`]: its name and its ROMs as (name, bytes).
type DatGame<'a> = (&'a str, Vec<(String, Vec<u8>)>);

/// A DAT for `games`.
fn disc_dat(dir: &Path, system: &str, games: &[DatGame]) -> PathBuf {
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

fn cd_tracks(seed: u8) -> Vec<u8> {
    let mut data = pattern(seed, 6 * 2352);
    data[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    data[15] = 2;
    data
}

fn cue_for(game: &str) -> String {
    format!(
        "FILE \"{game} (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"{game} (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n"
    )
}

fn lib_with(dir: &Path, dat: &Path, source: &Path, library: &Path) -> Library {
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

/// A DAT's ROM name is untrusted: one that would climb out of the archive
/// skips the game.
#[test]
fn unsafe_dat_names_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let data = pattern(5, 1024);
    std::fs::write(source.join("x.gb"), &data).unwrap();
    let dat = disc_dat(
        dir.path(),
        "Nintendo - Game Boy",
        &[("Evil (USA)", vec![("../../evil.gb".into(), data)])],
    );
    let lib = lib_with(dir.path(), &dat, &source, &library);
    let plan = lib.cleanup_plan(None).unwrap();
    assert!(plan.items.is_empty(), "{plan:?}");
    assert!(
        plan.skipped.iter().any(|(_, why)| why.contains("safely")),
        "{:?}",
        plan.skipped
    );
}

/// Undo keeps the new files while an original can't come back (its place
/// is taken), so the trash is never the only copy left.
#[test]
fn undo_keeps_new_files_until_every_original_is_back() {
    let s = setup(false, &[("gb/Test GB.gb", "Test GB.gb")], &["gb.dat"]);
    let mut lib = s.lib;
    let plan = lib.cleanup_plan(None).unwrap();
    let item = plan.items.iter().find(|i| i.title == "Test GB").unwrap().clone();
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(item.target.exists());
    // Something else now sits where the original was.
    std::fs::write(s.source.join("Test GB.gb"), b"someone else's file").unwrap();
    let undone = lib.cleanup_undo(None).unwrap();
    assert!(!undone.failed.is_empty());
    assert!(
        item.target.exists(),
        "the new zip stays while the original is in the trash"
    );
    // Once the place is free, undo finishes the job.
    std::fs::remove_file(s.source.join("Test GB.gb")).unwrap();
    let again = lib.cleanup_undo(None).unwrap();
    assert!(again.failed.is_empty(), "{:?}", again.failed);
    assert!(!item.target.exists());
    assert!(s.source.join("Test GB.gb").exists());
}

/// A game you named that sits in Source twice: one copy goes in the
/// archive, the other is a copy, not a second ROM.
#[test]
fn a_named_game_twice_is_one_part() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let library = dir.path().join("library");
    std::fs::create_dir_all(source.join("b")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let data = vec![3u8; 2048];
    std::fs::write(source.join("mine.gb"), &data).unwrap();
    std::fs::write(source.join("b/mine copy.gb"), &data).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        staging_dir: Some(dir.path().join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let row = lib.search_queue(None).unwrap()[0].row_id.clone();
    lib.identify_as(&row, "Game Boy", "My Game").unwrap();
    let plan = lib.cleanup_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(plan.items[0].parts.len(), 1, "{:?}", plan.items[0].parts);
    assert_eq!(plan.items[0].copies.len(), 1);
}

/// "Replace originals": the game is renamed and repacked in its own folder,
/// not moved into the Library; the original goes to the trash.
#[test]
fn replace_originals_works_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let library = dir.path().join("library");
    std::fs::create_dir_all(source.join("handheld")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    std::fs::copy(
        fixtures().join("roms/gb/Test GB.gb"),
        source.join("handheld/tgb.gb"),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        in_place: true,
        staging_dir: Some(dir.path().join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let source = source.canonicalize().unwrap();
    let plan = lib.cleanup_plan(None).unwrap();
    assert!(plan.in_place);
    let gb = plan.items.iter().find(|i| i.title == "Test GB").expect("planned");
    assert_eq!(
        gb.target,
        source.join("handheld/Test GB.zip"),
        "beside the original"
    );
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(source.join("handheld/Test GB.zip").exists());
    assert!(
        !source.join("handheld/tgb.gb").exists(),
        "the original went to the trash"
    );
    assert!(
        std::fs::read_dir(&library).unwrap().next().is_none(),
        "nothing in the Library"
    );
    lib.cleanup_undo(None).unwrap();
    assert!(source.join("handheld/tgb.gb").exists(), "undo puts it back");
    assert!(!source.join("handheld/Test GB.zip").exists());
    // It can't both replace and keep the originals.
    assert!(
        lib.save_settings(&Settings {
            source_dir: Some(source.clone()),
            library_dir: Some(library),
            in_place: true,
            keep_originals: true,
            ..Settings::default()
        })
        .is_err()
    );
}

/// Convert: a disc becomes a CHD beside itself under its own name (not
/// renamed or moved), the originals to the trash; a GameCube image, which
/// its emulator can't read as a CHD, is left alone and says why.
#[test]
fn convert_makes_chds_where_the_discs_are() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(&library).unwrap();
    let game = "Game (USA)";
    let d = source.join("my discs");
    std::fs::create_dir_all(&d).unwrap();
    let (t1, t2) = (cd_tracks(1), pattern(5, 155 * 2352));
    std::fs::write(d.join(format!("{game} (Track 1).bin")), &t1).unwrap();
    std::fs::write(d.join(format!("{game} (Track 2).bin")), &t2).unwrap();
    std::fs::write(d.join(format!("{game}.cue")), cue_for(game)).unwrap();
    let ps1 = disc_dat(
        dir.path(),
        "Sony - PlayStation",
        &[(
            game,
            vec![
                (format!("{game}.cue"), cue_for(game).into_bytes()),
                (format!("{game} (Track 1).bin"), t1),
                (format!("{game} (Track 2).bin"), t2),
            ],
        )],
    );
    let iso = pattern(7, 4096);
    std::fs::write(d.join("cube.iso"), &iso).unwrap();
    let gc = disc_dat(
        dir.path(),
        "Nintendo - GameCube",
        &[("Cube (USA)", vec![("Cube (USA).iso".into(), iso)])],
    );
    let mut lib = lib_with(dir.path(), &ps1, &source, &library);
    lib.import_dat(&gc, Some("GameCube")).unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let d = d.canonicalize().unwrap();

    let plan = lib.convert_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(plan.items[0].target, d.join(format!("{game}.chd")));
    assert_eq!(plan.items[0].format, Format::ChdCd);
    assert!(
        plan.items[0].sidecar.is_none(),
        "a CHD alone, no folder beside it"
    );
    assert!(
        plan.skipped
            .iter()
            .any(|(t, why)| t == "Cube (USA)" && why.contains("CHD")),
        "{:?}",
        plan.skipped
    );
    let report = lib.convert_apply(None, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(d.join(format!("{game}.chd")).exists());
    assert!(!d.join(format!("{game}.cue")).exists(), "originals to the trash");
    assert!(d.join("cube.iso").exists(), "GameCube left alone");
    let v = lib.library_view().unwrap();
    assert!(
        v.rows
            .iter()
            .any(|r| r.title == game && r.status == RowStatus::Verified),
        "the CHD verifies"
    );
    // Nothing more to convert.
    assert!(lib.convert_plan(None).unwrap().items.is_empty());
}
