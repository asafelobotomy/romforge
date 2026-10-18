//! Milestone 14: games that are folders (DOS, ScummVM). Always on copies in
//! temp folders.

use std::path::{Path, PathBuf};

use romforge_core::cleanup::Format;
use romforge_core::home::Home;
use romforge_core::launch::Env;
use romforge_core::scan::ScanOptions;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

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

fn zip_names(path: &Path) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut v: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    v.sort();
    v
}

/// A stand-in program: an executable script in `dir`.
fn fake_program(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

fn crc32(data: &[u8]) -> String {
    romforge_core::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .crc32
}

/// A DOS game folder: libretro's DOS.dat knows only its program's size and
/// CRC32, so it is a suggestion; once you confirm it, the whole folder is
/// the game. Import packs every file of it into one zip; undo restores it.
#[test]
fn a_dos_game_is_its_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    let game = incoming.join("KEEN");
    std::fs::create_dir_all(game.join("DOCS")).unwrap();
    let exe: Vec<u8> = (0..3000u32).map(|i| (i * 7) as u8).collect();
    std::fs::write(game.join("KEEN1.EXE"), &exe).unwrap();
    std::fs::write(game.join("EGAGRAPH.CK1"), vec![9u8; 1000]).unwrap();
    std::fs::write(game.join("DOCS/README.TXT"), b"read me").unwrap();
    std::fs::write(game.join("START.BAT"), b"KEEN1\r\n").unwrap();
    let before = files_under(&incoming);
    let dat = tmp.path().join("DOS.dat");
    std::fs::write(
        &dat,
        format!(
            "clrmamepro (\n\tname \"DOS\"\n)\n\ngame (\n\tname \"Commander Keen 1 (1990)\"\n\trom ( name \"KEEN1.EXE\" size {} crc {} )\n)\n",
            exe.len(),
            crc32(&exe)
        ),
    )
    .unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, Some("DOS")).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let keen = rows
        .iter()
        .find(|r| r.title == "Commander Keen 1 (1990)")
        .expect("named by its CRC");
    assert_eq!(keen.status, RowStatus::Candidate, "CRC32 only: never a match");
    assert_eq!(keen.folder, None);
    let queue = lib.search_queue(Some(std::slice::from_ref(&keen.id))).unwrap();
    let s = queue[0].suggestion.as_ref().expect("a suggestion");
    assert_eq!(
        (s.system.as_str(), s.title.as_str(), s.band),
        (
            "DOS",
            "Commander Keen 1 (1990)",
            romforge_core::suggest::Band::LikelyCrc
        )
    );

    lib.identify_as(&keen.id.clone(), "DOS", "Commander Keen 1 (1990)")
        .unwrap();
    let rows = lib.library_view().unwrap().rows;
    let keen = rows
        .iter()
        .find(|r| r.title == "Commander Keen 1 (1990)")
        .unwrap();
    assert_eq!(keen.status, RowStatus::ByYou);
    assert_eq!(keen.folder.as_deref(), Some(game.to_str().unwrap()));
    assert_eq!(keen.files, 2, "START.BAT joins it: {rows:?}");
    assert!(rows.iter().all(|r| r.title != "START.BAT"), "{rows:?}");

    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let it = &plan.items[0];
    let target = home.games().join("DOS/Commander Keen 1 (1990).zip");
    assert_eq!((it.format, &it.target), (Format::Folder, &target));
    assert_eq!(it.parts.len(), 4, "every file, hashed or not");
    assert_eq!(it.trash, std::slice::from_ref(&game));
    assert_eq!(files_under(&incoming), before, "planning moves nothing");

    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!((r.done, r.trashed), (1, 1));
    let names = zip_names(&target);
    for n in ["DOCS/README.TXT", "EGAGRAPH.CK1", "KEEN1.EXE", "START.BAT"] {
        assert!(names.contains(&n.to_owned()), "{names:?}");
    }
    assert!(names.iter().any(|n| n.starts_with("romforge/")), "{names:?}");
    assert!(!game.exists(), "the folder went to the trash whole");

    // Found again in Games/ by refresh: the zip is the game now.
    lib.refresh(ScanOptions::default(), |_, _| {}).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let keen = rows
        .iter()
        .find(|r| r.title == "Commander Keen 1 (1990)")
        .unwrap();
    assert_eq!(keen.status, RowStatus::ByYou);
    assert_eq!(keen.folder.as_deref(), Some(target.to_str().unwrap()));

    // DOSBox Pure takes the zip as it is; standalone DOSBox can't.
    let bin = tmp.path().join("bin");
    fake_program(&bin, "dosbox-staging");
    let cores = tmp.path().join("cores");
    let env = Env {
        path: vec![bin.clone()],
        core_dirs: vec![cores.clone()],
        flatpak_dirs: vec![],
    };
    let err = lib.launch_row(&env, &keen.id, None).unwrap_err().to_string();
    assert!(err.contains("dosbox-staging can't open archives"), "{err}");
    fake_program(&bin, "retroarch");
    fake_program(
        &cores,
        &format!("dosbox_pure_libretro.{}", std::env::consts::DLL_EXTENSION),
    );
    let spec = lib.launch_row(&env, &keen.id, None).unwrap();
    assert_eq!(spec.emulator, "retroarch:dosbox_pure");
    assert_eq!(spec.args.last().map(String::as_str), target.to_str());

    lib.cleanup_undo(None).unwrap();
    assert_eq!(files_under(&incoming), before, "undo puts the folder back");
    assert!(!target.exists());
}

/// A program loose in a scanned folder isn't a folder game: that folder
/// holds other things too.
#[test]
fn a_loose_program_is_not_a_folder_game() {
    let tmp = tempfile::tempdir().unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let exe = vec![5u8; 2048];
    std::fs::write(incoming.join("GAME.EXE"), &exe).unwrap();
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let row = lib.library_view().unwrap().rows[0].id.clone();
    lib.identify_as(&row, "DOS", "Some Game (1991)").unwrap();
    let rows = lib.library_view().unwrap().rows;
    assert_eq!(rows[0].status, RowStatus::ByYou);
    assert_eq!(rows[0].folder, None);
}

fn md5_hex(data: &[u8]) -> String {
    romforge_core::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .md5
}

/// A ScummVM game: named by ScummVM's own detection list (sizes and MD5s
/// of the first or last bytes), imported as a folder with its `.scummvm`
/// file, started with --auto-detect.
#[test]
fn a_scummvm_game_is_named_by_scummvm_s_list() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    let game = incoming.join("Steel Sky");
    std::fs::create_dir_all(&game).unwrap();
    let dnr: Vec<u8> = (0..80u8).collect();
    let dsk: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(game.join("SKY.DNR"), &dnr).unwrap();
    std::fs::write(game.join("SKY.DSK"), &dsk).unwrap();
    std::fs::write(game.join("readme.txt"), b"hi").unwrap();
    // Another folder with the right names but other data.
    let other = incoming.join("Not Sky");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("sky.dnr"), vec![1u8; 80]).unwrap();
    std::fs::write(other.join("sky.dsk"), vec![1u8; 9000]).unwrap();
    let before = files_under(&incoming);
    let list = format!(
        "scummvm (\n\tauthor \"scummvm\"\n\tversion \"2.9.0\"\n)\n\ngame (\n\tname \"sky\"\n\ttitle \"Beneath a Steel Sky\"\n\textra \"CD\"\n\tlanguage \"en\"\n\tplatform \"pc\"\n\tsourcefile \"sky\"\n\tengine \"sky\"\n\trom ( name \"sky.dnr\" size 80 md5-5000 {} )\n\trom ( name \"sky.dsk\" size -1 md5-5000 t:{} )\n)\n",
        md5_hex(&dnr),
        md5_hex(&dsk[dsk.len() - 5000..])
    );

    let mut lib = Library::open(home.db()).unwrap();
    let r = lib.scummvm_import(list.as_bytes()).unwrap();
    assert_eq!((r.entries, r.games), (1, 0), "nothing scanned yet");
    assert!(home.databases().join("scummvm-detection.dat").is_file());
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let sky: Vec<_> = rows.iter().filter(|r| r.status == RowStatus::Detected).collect();
    assert_eq!(sky.len(), 1, "{rows:?}");
    let sky = sky[0];
    assert_eq!(sky.title, "Beneath a Steel Sky (CD) (pc) (en)");
    assert_eq!(sky.system.as_deref(), Some("ScummVM"));
    assert_eq!(sky.folder.as_deref(), Some(game.to_str().unwrap()));
    assert_eq!(sky.files, 3);
    assert!(sky.playable);

    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let target = home.games().join("ScummVM/Beneath a Steel Sky (CD) (pc) (en)");
    assert_eq!(
        (plan.items[0].format, &plan.items[0].target),
        (Format::Dir, &target)
    );
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    let copied = files_under(&target);
    for n in ["SKY.DNR", "SKY.DSK", "readme.txt", "sky.scummvm"] {
        assert!(copied.contains(&n.to_owned()), "{copied:?}");
    }
    assert_eq!(
        std::fs::read_to_string(target.join("sky.scummvm")).unwrap(),
        "sky"
    );
    assert!(!game.exists() && other.exists(), "only the game moved");

    lib.refresh(ScanOptions::default(), |_, _| {}).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let sky = rows.iter().find(|r| r.status == RowStatus::Detected).unwrap();
    assert_eq!(sky.folder.as_deref(), Some(target.to_str().unwrap()));

    let bin = tmp.path().join("bin");
    fake_program(&bin, "scummvm");
    let env = Env {
        path: vec![bin],
        core_dirs: vec![],
        flatpak_dirs: vec![],
    };
    let spec = lib.launch_row(&env, &sky.id, None).unwrap();
    assert_eq!(spec.emulator, "scummvm");
    assert_eq!(
        spec.args,
        [format!("--path={}", target.display()), "--auto-detect".to_owned()]
    );

    lib.cleanup_undo(None).unwrap();
    assert_eq!(files_under(&incoming), before);
    assert!(!target.exists());
}
