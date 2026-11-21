//! What the app has and the CLI got to match: favourites, a game's emulator,
//! themes, the scan's dry run, and the settings of Display, Playing and
//! Fullscreen.

use std::path::Path;
use std::process::{Command, Output};

fn run_raw(db: &Path, args: &[&str]) -> Output {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .current_dir(&fixtures)
        .env("ROMFORGE_DB", db)
        .env("RUST_BACKTRACE", "0")
        .args(args)
        .output()
        .unwrap()
}

fn run(db: &Path, args: &[&str]) -> String {
    let out = run_raw(db, args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn fails(db: &Path, args: &[&str]) -> String {
    let out = run_raw(db, args);
    assert!(!out.status.success(), "{args:?} should fail");
    String::from_utf8(out.stderr).unwrap()
}

fn library(db: &Path) {
    run(
        db,
        &["dat", "import", "dats/gb.dat", "dats/nes.dat", "dats/snes.dat"],
    );
    run(db, &["scan", ".", "-q"]);
}

#[test]
fn scan_dry_run_keeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    run(&db, &["dat", "import", "dats/nes.dat"]);
    let seen = run(&db, &["scan", ".", "--dry-run"]);
    assert!(
        seen.contains("verified") && seen.contains("Test NES (iNES)"),
        "{seen}"
    );
    assert!(seen.contains("nothing was kept"), "{seen}");
    // Nothing was kept: the library has no file or game after it.
    let games = run(&db, &["games"]);
    assert!(games.starts_with("0 rows"), "{games}");
    // A scope that isn't a platform is refused, as for a real scan.
    assert!(fails(&db, &["scan", ".", "--dry-run", "--system", "nope"]).contains("unknown system"));
}

#[test]
fn scan_without_dats_says_how_to_get_them() {
    // The app offers to download DATs before a scan when none is imported;
    // off a terminal the CLI says how instead of asking, and scans anyway.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    let out = run_raw(&db, &["scan", ".", "-q"]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("no DATs are imported") && err.contains("dat download --all"),
        "{err}"
    );
    // With one imported, nothing is said.
    run(&db, &["dat", "import", "dats/nes.dat"]);
    let out = run_raw(&db, &["scan", ".", "-q"]);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("no DATs"));
}

#[test]
fn favourites_are_added_listed_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    library(&db);
    assert!(run(&db, &["favourite", "list"]).contains("no favourites"));
    run(&db, &["favourite", "add", "Test GB"]);
    run(&db, &["favourite", "add", "Test SNES (clean)"]);
    let list = run(&db, &["favourite", "list"]);
    assert!(
        list.contains("Test GB") && list.contains("Test SNES (clean)"),
        "{list}"
    );
    run(&db, &["favourite", "remove", "Test GB"]);
    let list = run(&db, &["favourite", "list"]);
    assert!(
        !list.contains("Test GB") && list.contains("Test SNES (clean)"),
        "{list}"
    );
    assert!(fails(&db, &["favourite", "add", "No Such Game"]).contains("no identified game"));
}

#[test]
fn a_games_emulator_only_takes_an_installed_one() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    library(&db);
    let shown = run(&db, &["emulator", "pick", "Test SNES (clean)"]);
    assert!(shown.contains("romforge's choice"), "{shown}");
    let err = fails(
        &db,
        &["emulator", "pick", "Test SNES (clean)", "no-such-emulator"],
    );
    assert!(err.contains("isn't an installed emulator"), "{err}");
    assert!(
        run(&db, &["emulator", "pick", "Test SNES (clean)", "--clear"])
            .contains("chooses the emulator again")
    );
}

#[test]
fn a_theme_is_picked_and_shown() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    assert!(run(&db, &["themes", "list"]).contains("<- in use"));
    let shown = run(&db, &["themes", "use", "famicom", "--text-size", "115"]);
    assert!(shown.contains("famicom") && shown.contains("115%"), "{shown}");
    assert!(run(&db, &["themes", "show"]).contains("famicom"));
    assert!(fails(&db, &["themes", "use", "no-such-theme"]).contains("no usable theme"));
    assert!(fails(&db, &["themes", "use", "famicom", "--text-size", "111"]).contains("text size"));
}

#[test]
fn display_playing_and_fullscreen_settings() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    let shown = run(
        &db,
        &[
            "settings",
            "--while-playing",
            "stay",
            "--language",
            "fr",
            "--video-snaps",
            "true",
            "--attract-minutes",
            "5",
            "--music-volume",
            "70",
            "--shelf-sort",
            "played",
            "--fs-details",
            "logo,genre",
        ],
    );
    for line in [
        "while playing   stay",
        "language        fr",
        "video snaps     yes",
        "attract mode    5 min",
        "fs details      logo, genre",
    ] {
        assert!(shown.contains(line), "{line:?} not in\n{shown}");
    }
    assert!(shown.contains("played order"), "{shown}");
    // They are kept.
    assert!(run(&db, &["settings"]).contains("while playing   stay"));
    // The settings' own checks and the lists of choices apply.
    assert!(fails(&db, &["settings", "--while-playing", "hide"]).contains("tray, minimise or stay"));
    assert!(fails(&db, &["settings", "--attract-minutes", "7"]).contains("one of"));
    assert!(fails(&db, &["settings", "--shelf-sort", "random"]).contains("title, released, newest, played"));
    assert!(fails(&db, &["settings", "--fs-details", "nonsense"]).contains("no detail"));
}
