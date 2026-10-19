//! Milestone 13: one RomForge folder. Always on copies in temp folders.
//!
//! Multi-file sets and shared tracks.

use romforge_core::home::Home;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::home::*;

/// A set of a system whose core opens disks itself (PC-98's np2kai) is
/// kept unzipped in its own folder, with the `.cmd` np2kai reads.
#[test]
fn a_pc98_set_is_kept_loose_with_its_cmd() {
    let (_tmp, home, r) = import_set("PC-98", "d88");
    assert_eq!(r.playlists, 1);
    let set = home.games().join("PC-98/Ys (1987)(Nihon Falcom)");
    assert!(set.join("Ys (1987)(Nihon Falcom)(Disk 1 of 2).d88").is_file());
    assert!(set.join("Ys (1987)(Nihon Falcom)(Disk 2 of 2).d88").is_file());
    assert_eq!(
        std::fs::read_to_string(set.join("Ys (1987)(Nihon Falcom).cmd")).unwrap(),
        "np2kai \"Ys (1987)(Nihon Falcom)(Disk 1 of 2).d88\" \"Ys (1987)(Nihon Falcom)(Disk 2 of 2).d88\"\n"
    );
    assert!(set.join("romforge/Ys (1987)(Nihon Falcom)(Disk 1 of 2)").is_dir());

    // Play starts the set's .cmd, whichever disk's row it is.
    let mut lib = Library::open(home.db()).unwrap();
    lib.refresh(romforge_core::scan::ScanOptions::default(), |_, _| {})
        .unwrap();
    let rows = lib.library_view().unwrap().rows;
    let disk2 = rows
        .iter()
        .find(|r| r.title == "Ys (1987)(Nihon Falcom)(Disk 2 of 2)")
        .unwrap();
    assert_eq!(disk2.status, RowStatus::Verified);
    let bin = _tmp.path().join("bin");
    let cores = _tmp.path().join("cores");
    for (dir, name) in [
        (&bin, "retroarch".to_owned()),
        (
            &cores,
            format!("np2kai_libretro.{}", std::env::consts::DLL_EXTENSION),
        ),
    ] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(&name), "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let env = romforge_core::launch::Env {
        path: vec![bin],
        core_dirs: vec![cores],
        flatpak_dirs: vec![],
        ..Default::default()
    };
    let spec = lib.launch_row(&env, &disk2.id, None).unwrap();
    assert_eq!(spec.rom, set.join("Ys (1987)(Nihon Falcom).cmd"));
}

/// PC-88's quasi88 reads an `.m3u` of the loose images.
#[test]
fn a_pc88_set_is_kept_loose_with_its_m3u() {
    let (_tmp, home, r) = import_set("PC-88", "d88");
    assert_eq!(r.playlists, 1);
    let set = home.games().join("PC-88/Ys (1987)(Nihon Falcom)");
    assert_eq!(
        std::fs::read_to_string(set.join("Ys (1987)(Nihon Falcom).m3u")).unwrap(),
        "Ys (1987)(Nihon Falcom)(Disk 1 of 2).d88\nYs (1987)(Nihon Falcom)(Disk 2 of 2).d88\n"
    );
}

/// Elsewhere every disk is a zip, and the playlist lists the zips.
#[test]
fn an_amiga_set_is_zipped_with_an_m3u() {
    let (_tmp, home, r) = import_set("Amiga", "adf");
    assert_eq!(r.playlists, 1);
    assert_eq!(
        std::fs::read_to_string(home.games().join("Amiga/Ys (1987)(Nihon Falcom).m3u")).unwrap(),
        "Ys (1987)(Nihon Falcom)(Disk 1 of 2).zip\nYs (1987)(Nihon Falcom)(Disk 2 of 2).zip\n"
    );
}

/// Two discs share an identical audio track, each folder has its own copy:
/// each copy belongs to the disc beside it, neither is a duplicate.
#[test]
fn a_shared_track_belongs_to_each_disc() {
    let tmp = tempfile::tempdir().unwrap();
    let (a1, b1, shared) = (vec![1u8; 2352 * 3], vec![2u8; 2352 * 3], vec![9u8; 2352 * 2]);
    let dat = tmp.path().join("discs.dat");
    write_multi_dat(
        &dat,
        &[
            (
                "Disc A (USA)",
                vec![
                    ("Disc A (USA) (Track 1).bin", &a1[..]),
                    ("Disc A (USA) (Track 2).bin", &shared[..]),
                ],
            ),
            (
                "Disc B (USA)",
                vec![
                    ("Disc B (USA) (Track 1).bin", &b1[..]),
                    ("Disc B (USA) (Track 2).bin", &shared[..]),
                ],
            ),
        ],
    );
    let root = tmp.path().join("discs");
    for (dir, t1) in [("A", &a1), ("B", &b1)] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        std::fs::write(root.join(dir).join("track1.bin"), t1).unwrap();
        std::fs::write(root.join(dir).join("track2.bin"), &shared).unwrap();
    }
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    lib.import_dat(&dat, Some("PlayStation")).unwrap();
    lib.scan(&root, Scope::All).unwrap();
    let files = lib.files(&romforge_core::LibraryFilter::default()).unwrap();
    for f in &files {
        assert_eq!(f.status.as_str(), "matched", "{}", f.display_path());
    }
    let rows = lib.library_view().unwrap().rows;
    for (title, dir) in [("Disc A (USA)", "A"), ("Disc B (USA)", "B")] {
        let row = rows.iter().find(|r| r.title == title).unwrap();
        assert_eq!(row.status, RowStatus::Verified);
        let d = lib.game_detail(&row.id).unwrap();
        let mine: Vec<&str> = d
            .files
            .iter()
            .filter(|f| f.status == "matched")
            .map(|f| f.path.as_str())
            .collect();
        assert!(
            mine.iter().all(|p| p.contains(&format!("/{dir}/"))),
            "{title}'s own copies are its: {mine:?}"
        );
    }
}

/// The same game always zips to the same bytes, wherever it came from: no
/// clock in the zip, members in one order (milestone 16).
#[test]
fn the_same_game_zips_to_the_same_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let (one, two) = (vec![3u8; 3000], vec![4u8; 5000]);
    let dat = tmp.path().join("set.dat");
    write_multi_dat(
        &dat,
        &[(
            "Two Parts (USA)",
            vec![("b-part.bin", &two[..]), ("A-part.bin", &one[..])],
        )],
    );
    let mut made = Vec::new();
    for (i, zipped) in [true, false].into_iter().enumerate() {
        let home = Home::create(&tmp.path().join(format!("h{i}"))).unwrap();
        let src = tmp.path().join(format!("src{i}"));
        std::fs::create_dir_all(&src).unwrap();
        if zipped {
            // Members in the other order, with a time of their own.
            let mut z = zip::ZipWriter::new(std::fs::File::create(src.join("x.zip")).unwrap());
            let o = zip::write::SimpleFileOptions::default()
                .last_modified_time(zip::DateTime::from_date_and_time(2020, 5, 6, 7, 8, 10).unwrap());
            z.start_file("b-part.bin", o).unwrap();
            std::io::Write::write_all(&mut z, &two).unwrap();
            z.start_file("A-part.bin", o).unwrap();
            std::io::Write::write_all(&mut z, &one).unwrap();
            z.finish().unwrap();
        } else {
            std::fs::write(src.join("A-part.bin"), &one).unwrap();
            std::fs::write(src.join("b-part.bin"), &two).unwrap();
        }
        let mut lib = Library::open(home.db()).unwrap();
        lib.import_dat(&dat, Some("Game Boy")).unwrap();
        lib.scan(&src, Scope::All).unwrap();
        let plan = lib.import_plan(Some(&src)).unwrap();
        let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        made.push(std::fs::read(home.games().join("Game Boy/Two Parts (USA).zip")).unwrap());
    }
    assert_eq!(made[0], made[1], "same game, same bytes");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&made[0])).unwrap();
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    assert_eq!(names[..2], ["A-part.bin", "b-part.bin"], "by lower-case name");
    assert_eq!(
        z.by_index(0).unwrap().last_modified(),
        Some(zip::DateTime::default())
    );
}
