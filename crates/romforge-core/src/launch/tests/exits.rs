//! Each emulator starts the game alone and quits with it, in a window or
//! fullscreen: the flags romforge passes, as read in each one's source.

use super::*;

fn args(name: &str, windowed: bool) -> Vec<String> {
    let e = known(name).unwrap();
    let rom = Path::new("/g/Game.iso");
    let v = Vars {
        rom,
        set: "Game",
        core: None,
        config: None,
        bios: None,
        system: "PlayStation",
    };
    let o = LaunchOptions {
        windowed,
        ..Default::default()
    };
    known_args(e, &v, &o)
}

#[test]
fn a_window_keeps_the_flags_that_quit_with_the_game() {
    for name in ["duckstation", "pcsx2"] {
        let w = args(name, true);
        assert!(
            w.contains(&"-batch".into()) && w.contains(&"-nofullscreen".into()),
            "{name}: {w:?}"
        );
        assert!(!w.contains(&"-fullscreen".into()), "{name}");
        let f = args(name, false);
        assert!(
            f.contains(&"-batch".into()) && f.contains(&"-nogui".into()),
            "{name}: {f:?}"
        );
    }
    let ppsspp = args("ppsspp", true);
    assert!(ppsspp.contains(&"--pause-menu-exit".into()) && ppsspp.contains(&"--windowed".into()));
    assert_eq!(args("mame", true)[..2], ["-skip_gameinfo", "-window"]);
    assert_eq!(args("mame", false)[..2], ["-skip_gameinfo", "-nowindow"]);
    assert!(args("dolphin", true).contains(&"Dolphin.Interface.ConfirmStop=False".into()));
    assert!(args("dolphin", true).contains(&"Dolphin.Display.Fullscreen=False".into()));
    assert_eq!(
        args("azahar", true).last().map(String::as_str),
        Some("/g/Game.iso"),
        "the game last"
    );
    assert!(args("azahar", true).contains(&"-w".into()));
}

#[test]
fn a_dos_program_closes_dosbox_and_a_folder_does_not() {
    let t = tempfile::tempdir().unwrap();
    let exe = t.path().join("GAME.EXE");
    std::fs::write(&exe, b"MZ").unwrap();
    assert_eq!(owned::saves::dos_exit("dosbox-x", &exe), Some("-exit"));
    assert_eq!(owned::saves::dos_exit("dosbox-staging", &exe), Some("--exit"));
    assert_eq!(
        owned::saves::dos_exit("dosbox-x", t.path()),
        None,
        "a folder: its prompt"
    );
    assert_eq!(owned::saves::dos_exit("mednafen", &exe), None);
}
