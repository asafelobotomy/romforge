//! Milestone 18 with stand-in emulators: romforge's own config for an
//! emulator, and the one picked for a game. Nothing real is installed or run.
#![cfg(unix)]

mod common;
use common::launch::*;

/// With a RomForge folder, RetroArch runs from romforge's copy of its
/// config: written when the game starts, the user's own keys kept.
#[test]
fn retroarch_runs_from_romforge_s_own_config() {
    let lib = library();
    let mut f = fake();
    let out = f.root.join("args.txt");
    script(
        &f.root.join("bin/retroarch"),
        &format!("echo \"$@\" > '{}'", out.display()),
    );
    core(&f, "mgba");
    let home = romforge_core::home::Home::create(&f.root.join("parent")).unwrap();
    f.env.home = Some(home.clone());

    let spec = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    let config = home.emulator_config("retroarch");
    let (base, system) = (
        config.join("retroarch.cfg"),
        config.join("systems/Game Boy Advance.cfg"),
    );
    assert_eq!(
        spec.args[..5],
        [
            "-f".to_owned(),
            "--config".to_owned(),
            base.display().to_string(),
            "--appendconfig".to_owned(),
            system.display().to_string()
        ]
    );
    assert!(!base.exists(), "planning writes nothing");

    romforge_core::launch::spawn(&spec).unwrap();
    let text = std::fs::read_to_string(&base).unwrap();
    let bios = home.bios().join("RetroArch");
    assert!(
        text.contains(&format!("system_directory = \"{}\"", bios.display())),
        "{text}"
    );
    assert!(text.contains("config_save_on_exit = \"false\""));
    let saves = home.saves().join("Game Boy Advance");
    let per_system = std::fs::read_to_string(&system).unwrap();
    assert!(per_system.contains(&format!("savefile_directory = \"{}\"", saves.display())));
    assert!(saves.is_dir() && home.states().join("Game Boy Advance").is_dir() && bios.is_dir());

    // A key set by hand in romforge's copy stays; romforge's own come back.
    let edited = text.replace(
        "config_save_on_exit = \"false\"",
        "config_save_on_exit = \"true\"",
    );
    std::fs::write(&base, format!("video_driver = \"vulkan\"\n{edited}")).unwrap();
    romforge_core::launch::spawn(&spec).unwrap();
    let text = std::fs::read_to_string(&base).unwrap();
    assert!(text.starts_with("video_driver = \"vulkan\"\n"), "{text}");
    assert!(text.contains("config_save_on_exit = \"false\""));

    // Emulator Settings' keys go in too; ones not set come out.
    f.env.options.insert(
        "retroarch".into(),
        romforge_core::launch::LaunchOptions {
            config: vec![romforge_core::launch::ConfigWrite {
                path: "retroarch.cfg".into(),
                format: romforge_core::launch::Format::RetroArch,
                keys: vec![("video_vsync".into(), "false".into())],
                remove: vec!["video_driver".into()],
            }],
            ..Default::default()
        },
    );
    let spec = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    romforge_core::launch::spawn(&spec).unwrap();
    let text = std::fs::read_to_string(&base).unwrap();
    assert!(
        text.contains("video_vsync = \"false\"") && !text.contains("video_driver"),
        "{text}"
    );
}

/// A game's own emulator (stage 2, step 6) is used, and romforge's choice
/// comes back when that one is gone.
#[test]
fn a_game_keeps_the_emulator_picked_for_it() {
    let lib = library();
    let f = fake();
    script(&f.root.join("bin/retroarch"), "exit 0");
    core(&f, "mgba");
    let vbam = core(&f, "vbam");
    let rows = lib.library_view().unwrap().rows;
    let gba = rows.iter().find(|r| r.title.starts_with("Test GBA")).unwrap();
    let (system, title) = (gba.system.clone().unwrap(), gba.title.clone());
    assert_eq!(
        lib.launch_row(&f.env, &gba.id, None).unwrap().emulator,
        "retroarch:mgba"
    );

    let (picked, choices) = lib.game_emulator(&f.env, &system, &title).unwrap();
    assert_eq!(picked, None);
    assert!(choices.contains(&"retroarch:vbam".to_owned()), "{choices:?}");
    assert!(
        lib.set_game_emulator(&f.env, &system, &title, Some("mgba"))
            .is_err(),
        "not installed"
    );
    lib.set_game_emulator(&f.env, &system, &title, Some("retroarch:vbam"))
        .unwrap();
    assert_eq!(
        lib.launch_row(&f.env, &gba.id, None).unwrap().emulator,
        "retroarch:vbam"
    );
    // Asked for one: that one.
    assert_eq!(
        lib.launch_row(&f.env, &gba.id, Some("retroarch:mgba"))
            .unwrap()
            .emulator,
        "retroarch:mgba"
    );
    std::fs::remove_file(vbam).unwrap();
    assert_eq!(
        lib.launch_row(&f.env, &gba.id, None).unwrap().emulator,
        "retroarch:mgba",
        "gone: romforge's choice"
    );
}
