//! Refresh brings the lists games are identified by up to date first:
//! ScummVM's and MAME's lists when the installed program changed, with
//! stand-in programs that log how they were called. Online lists are off
//! (age 0) so nothing here goes on the network.

use std::path::Path;

use romforge_core::home::Home;
use romforge_core::launch::Env;
use romforge_core::scan::ScanOptions;
use romforge_core::settings::Settings;
use romforge_core::view::RowStatus;
use romforge_core::{Library, ListOptions, Scope};

fn md5_hex(data: &[u8]) -> String {
    romforge_core::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .md5
}

/// A program in `bin` that logs each call to `log` and answers by `body`.
fn program(bin: &Path, name: &str, log: &Path, body: &str) {
    std::fs::create_dir_all(bin).unwrap();
    let p = bin.join(name);
    std::fs::write(
        &p,
        format!("#!/bin/sh\necho \"$1\" >> '{}'\n{body}\n", log.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn calls(log: &Path, arg: &str) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|l| *l == arg)
        .count()
}

fn options(bin: &Path) -> ListOptions {
    ListOptions {
        max_age_days: 0,
        env: Some(Env {
            path: vec![bin.to_owned()],
            core_dirs: vec![],
            flatpak_dirs: vec![],
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Beneath a Steel Sky in `dir`, and ScummVM's list for it.
fn sky(dir: &Path) -> String {
    std::fs::create_dir_all(dir).unwrap();
    let dnr: Vec<u8> = (0..80u8).collect();
    let dsk: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("SKY.DNR"), &dnr).unwrap();
    std::fs::write(dir.join("SKY.DSK"), &dsk).unwrap();
    format!(
        "scummvm (\n\tversion \"2.9.0\"\n)\n\ngame (\n\tname \"sky\"\n\ttitle \"Beneath a Steel Sky\"\n\textra \"CD\"\n\tlanguage \"en\"\n\tplatform \"pc\"\n\tsourcefile \"sky\"\n\tengine \"sky\"\n\trom ( name \"sky.dnr\" size 80 md5-5000 {} )\n\trom ( name \"sky.dsk\" size -1 md5-5000 t:{} )\n)\n",
        md5_hex(&dnr),
        md5_hex(&dsk[dsk.len() - 5000..])
    )
}

fn source(lib: &mut Library, dir: &Path) {
    lib.save_settings(&Settings {
        source_dir: Some(dir.to_owned()),
        ..Settings::default()
    })
    .unwrap();
}

/// A ScummVM game is named in the first Refresh, its files not hashed;
/// ScummVM's list is read again only when ScummVM changed.
#[test]
fn refresh_reads_scummvm_s_list_when_scummvm_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let roms = tmp.path().join("roms");
    let list = tmp.path().join("list.dat");
    std::fs::write(&list, sky(&roms.join("Steel Sky"))).unwrap();
    let (bin, log, version) = (
        tmp.path().join("bin"),
        tmp.path().join("calls"),
        tmp.path().join("v"),
    );
    // Its first run says so before its version.
    std::fs::write(
        &version,
        "Default configuration file missing, creating a new one\nScummVM 2.9.0 (Dec 2024)",
    )
    .unwrap();
    program(
        &bin,
        "scummvm",
        &log,
        &format!(
            "case \"$1\" in\n--version) cat '{}' ;;\n--dump-all-detection-entries) cat '{}' ;;\nesac",
            version.display(),
            list.display()
        ),
    );
    let mut lib = Library::open(home.db()).unwrap();
    source(&mut lib, &roms);
    let opts = options(&bin);
    let refresh = |lib: &mut Library| {
        lib.refresh_with_lists(false, &ScanOptions::default(), &opts, |_| {})
            .unwrap()
    };

    let (lists, results) = refresh(&mut lib);
    assert!(
        lists
            .updated
            .iter()
            .any(|u| u.starts_with("ScummVM's list (ScummVM 2.9.0")),
        "{lists:?}"
    );
    let scanned = results.iter().find_map(|r| r.as_ref().ok()).unwrap();
    assert_eq!((scanned.in_games, scanned.hashed), (2, 0), "{scanned:?}");
    let rows = lib.library_view().unwrap().rows;
    assert!(
        rows.iter()
            .any(|r| r.status == RowStatus::Detected && r.title.starts_with("Beneath a Steel Sky")),
        "{rows:?}"
    );
    assert_eq!(calls(&log, "--dump-all-detection-entries"), 1);

    // A later run without the first-run line: still the same ScummVM.
    std::fs::write(&version, "ScummVM 2.9.0 (Dec 2024)").unwrap();
    let (lists, _) = refresh(&mut lib);
    assert!(lists.updated.is_empty(), "the same ScummVM: {lists:?}");
    assert_eq!(calls(&log, "--dump-all-detection-entries"), 1);

    std::fs::write(&version, "ScummVM 2026.3.0 (Oct 2026)").unwrap();
    let (lists, _) = refresh(&mut lib);
    assert!(lists.updated.iter().any(|u| u.contains("2026.3.0")), "{lists:?}");
    assert_eq!(calls(&log, "--dump-all-detection-entries"), 2);
}

/// MAME's list: not for a library without Arcade; once a folder is
/// scanned for Arcade, read, and again only when MAME's version changed
/// (a wrapper's "first time" line ignored).
#[test]
fn refresh_reads_mame_s_list_when_mame_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    let arcade = tmp.path().join("arcade");
    std::fs::create_dir_all(&arcade).unwrap();
    let (bin, log, version) = (
        tmp.path().join("bin"),
        tmp.path().join("calls"),
        tmp.path().join("v"),
    );
    std::fs::write(&version, "Running MAME for the first time...\n0.289 (mame0289)").unwrap();
    program(
        &bin,
        "mame",
        &log,
        &format!(
            "case \"$1\" in\n-version) cat '{}' ;;\n-listxml) echo '<mame build=\"x\"><machine name=\"pacman\"><description>Pac-Man</description><rom name=\"pm.1\" size=\"4\" crc=\"01020304\" sha1=\"{}\"/></machine></mame>' ;;\nesac",
            version.display(),
            "a".repeat(40)
        ),
    );
    let opts = options(&bin);
    let lists = |lib: &mut Library| lib.update_lists(&opts, |_| {}).unwrap();

    lists(&mut lib);
    assert_eq!(calls(&log, "-listxml"), 0, "no Arcade here");

    let id = romforge_core::systems::by_key("Arcade").unwrap().id;
    lib.scan(&arcade, Scope::System(id)).unwrap();
    let r = lists(&mut lib);
    assert!(
        r.updated.iter().any(|u| u.starts_with("MAME's list (0.289")),
        "{r:?}"
    );
    assert_eq!(calls(&log, "-listxml"), 1);
    lists(&mut lib);
    assert_eq!(calls(&log, "-listxml"), 1, "the same MAME");

    std::fs::write(&version, "0.290 (mame0290)").unwrap();
    lists(&mut lib);
    assert_eq!(calls(&log, "-listxml"), 2, "a newer MAME");
}
