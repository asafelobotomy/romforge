use super::*;

fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn retroarch_keys_are_set_and_the_rest_kept() {
    let text =
        "# mine\nvideo_driver = \"vulkan\"\nsystem_directory = \"/old\"\nsystem_directory = \"/dup\"\n";
    let keys = kv(&[
        ("system_directory", "/rf/Bios/RetroArch"),
        ("config_save_on_exit", "false"),
    ]);
    assert_eq!(
        merge(Format::RetroArch, text, &keys, &[]).unwrap(),
        "# mine\nvideo_driver = \"vulkan\"\nsystem_directory = \"/rf/Bios/RetroArch\"\nconfig_save_on_exit = \"false\"\n"
    );
    assert!(merge(Format::RetroArch, "", &kv(&[("x", "a\"b")]), &[]).is_err());
    // A setting no longer set goes, so RetroArch's default applies.
    let gone = merge(Format::RetroArch, text, &[], &["video_driver".to_owned()]).unwrap();
    assert!(!gone.contains("video_driver"));
}

#[test]
fn ini_keys_go_in_their_sections() {
    let text = "[Main]\nSettingsVersion = 3\n\n[BIOS]\nSearchDirectory = bios\nPathNTSCU = x.bin\n";
    let keys = kv(&[
        ("[BIOS]SearchDirectory", "/rf/Bios/duckstation"),
        ("[Main]SetupWizardIncomplete", "false"),
        ("[UI]SettingsVersion", "1"),
    ]);
    let out = merge(Format::Ini, text, &keys, &["[BIOS]PathNTSCU".to_owned()]).unwrap();
    assert_eq!(
        out,
        "[Main]\nSettingsVersion = 3\nSetupWizardIncomplete = false\n\n[BIOS]\nSearchDirectory = /rf/Bios/duckstation\n\n[UI]\nSettingsVersion = 1\n"
    );
    let fresh = merge(
        Format::Ini,
        "",
        &kv(&[("[ports.qt]gba.bios", "/b/gba_bios.bin")]),
        &[],
    )
    .unwrap();
    assert_eq!(fresh, "[ports.qt]\ngba.bios = /b/gba_bios.bin\n");
}

#[test]
fn toml_tables_and_strings() {
    let keys = kv(&[
        ("[general]show_welcome", "false"),
        (
            "[sys.files]bootrom_path",
            &toml_string("/rf/Bios/xemu/mcpx_1.0.bin"),
        ),
    ]);
    let text = "[general]\nshow_welcome = true\n\n[sys.files]\nhdd_path = \"/home/u/xbox.qcow2\"\n";
    assert_eq!(
        merge(Format::Toml, text, &keys, &[]).unwrap(),
        "[general]\nshow_welcome = false\n\n[sys.files]\nhdd_path = \"/home/u/xbox.qcow2\"\nbootrom_path = \"/rf/Bios/xemu/mcpx_1.0.bin\"\n"
    );
    assert_eq!(toml_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
}

#[test]
fn yaml_and_xml() {
    let text = "$(EmulatorDir): \"\"\n/dev_hdd0/: $(EmulatorDir)dev_hdd0/\n/dev_usb***/:\n  /dev_usb000:\n    Path: x\n";
    let out = merge(
        Format::Yaml,
        text,
        &kv(&[("/dev_hdd0/", &toml_string("/home/u/hdd0/"))]),
        &[],
    )
    .unwrap();
    assert!(
        out.contains("/dev_hdd0/: \"/home/u/hdd0/\"\n/dev_usb***/:\n  /dev_usb000:"),
        "{out}"
    );
    let fresh = merge(Format::Xml, "", &kv(&[("mlc_path", "/home/u/mlc01")]), &[]).unwrap();
    assert_eq!(
        fresh,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n    <mlc_path>/home/u/mlc01</mlc_path>\n</content>\n"
    );
    let theirs = "<?xml version=\"1.0\"?>\n<content>\n  <mlc_path></mlc_path>\n  <GamePaths/>\n</content>\n";
    let out = merge(Format::Xml, theirs, &kv(&[("mlc_path", "/a&b")]), &[]).unwrap();
    assert!(
        out.contains("<mlc_path>/a&amp;b</mlc_path>\n  <GamePaths/>"),
        "{out}"
    );
}

#[test]
fn mednafen_lines_are_key_space_value() {
    let text = ";VERSION 1.32.1\npsx.bios_na scph5501.bin\nfilesys.path_firmware firmware\n";
    let out = merge(
        Format::Spaced,
        text,
        &kv(&[("filesys.path_firmware", "/rf/Bios/mednafen")]),
        &[],
    )
    .unwrap();
    assert_eq!(
        out,
        ";VERSION 1.32.1\npsx.bios_na scph5501.bin\nfilesys.path_firmware /rf/Bios/mednafen\n"
    );
}

#[test]
fn nested_yaml_paths_are_set_and_added_where_they_belong() {
    let text = "Core:\n  PPU Decoder: Recompiler (LLVM)\nVideo:\n  Renderer: OpenGL\n  Resolution Scale: 100\n  Performance Overlay:\n    Enabled: false\n    Font size: 10\nAudio:\n  Volume: 100\n";
    let keys = kv(&[
        ("Video/Renderer", "Vulkan"),
        ("Video/Performance Overlay/Enabled", "true"),
        ("Video/VSync Mode", "Full"),
        ("Miscellaneous/Show trophy popups", "false"),
    ]);
    let out = merge(Format::NestedYaml, text, &keys, &[]).unwrap();
    assert_eq!(
        out,
        "Core:\n  PPU Decoder: Recompiler (LLVM)\nVideo:\n  Renderer: Vulkan\n  Resolution Scale: 100\n  Performance Overlay:\n    Enabled: true\n    Font size: 10\n  VSync Mode: Full\nAudio:\n  Volume: 100\nMiscellaneous:\n  Show trophy popups: false\n"
    );
    let back = read(Format::NestedYaml, &out);
    assert!(back.contains(&("Video/Performance Overlay/Enabled".into(), "true".into())));
    assert!(
        !back.iter().any(|(k, _)| k == "Video"),
        "a map has no value of its own"
    );
}

#[test]
fn nested_xml_paths_are_set_and_added() {
    let text = "<?xml version=\"1.0\"?>\n<content>\n    <mlc_path>/m</mlc_path>\n    <Graphic>\n        <api>0</api>\n        <Overlay>\n            <Position>0</Position>\n        </Overlay>\n    </Graphic>\n    <GamePaths>\n        <Entry>/a</Entry>\n        <Entry>/b</Entry>\n    </GamePaths>\n</content>\n";
    let keys = kv(&[
        ("Graphic/api", "1"),
        ("Graphic/Overlay/FPS", "true"),
        ("Graphic/VSync", "1"),
    ]);
    let out = merge(Format::Xml, text, &keys, &[]).unwrap();
    let back = read(Format::Xml, &out);
    for (k, v) in [
        ("Graphic/api", "1"),
        ("Graphic/Overlay/FPS", "true"),
        ("Graphic/VSync", "1"),
        ("mlc_path", "/m"),
    ] {
        assert!(back.contains(&(k.to_owned(), v.to_owned())), "{k} in {out}");
    }
    assert!(
        !back.iter().any(|(k, _)| k == "GamePaths/Entry"),
        "a list's entries name no one element"
    );
    assert!(out.contains("<Position>0</Position>"));
}

#[test]
fn qt_ini_keys_get_their_default_flag_and_reads_skip_it() {
    let out = merge(
        Format::QtIni,
        "[Renderer]\nuse_vsync\\default=true\nuse_vsync=true\n",
        &kv(&[("[Renderer]use_vsync", "false")]),
        &[],
    )
    .unwrap();
    assert!(
        out.contains("use_vsync = false") && out.contains("use_vsync\\default = false"),
        "{out}"
    );
    assert_eq!(
        read(Format::QtIni, &out),
        [("[Renderer]use_vsync".to_owned(), "false".to_owned())]
    );
    assert_eq!(
        read(Format::RetroArch, "video_driver = \"vulkan\"\n# c\n"),
        [("video_driver".to_owned(), "vulkan".to_owned())]
    );
    assert_eq!(
        read(
            Format::Spaced,
            ";VERSION 1\nvideo.driver opengl\nwaitvsync                 1\n"
        ),
        [
            ("video.driver".to_owned(), "opengl".to_owned()),
            ("waitvsync".to_owned(), "1".to_owned())
        ]
    );
}

#[test]
fn nested_values_and_qt_keys_can_be_taken_out() {
    let yaml =
        "Video:\n  Renderer: Vulkan\n  Performance Overlay:\n    Enabled: true\n  Aspect ratio: 16:9\n";
    let out = merge(
        Format::NestedYaml,
        yaml,
        &[],
        &["Video/Aspect ratio".into(), "Video".into()],
    )
    .unwrap();
    assert_eq!(
        out, "Video:\n  Renderer: Vulkan\n  Performance Overlay:\n    Enabled: true\n",
        "a map stays"
    );
    let xml = "<content>\n    <Graphic>\n        <api>1</api>\n        <VSync>1</VSync>\n    </Graphic>\n</content>\n";
    let out = merge(Format::Xml, xml, &[], &["Graphic/VSync".into()]).unwrap();
    assert_eq!(
        out,
        "<content>\n    <Graphic>\n        <api>1</api>\n    </Graphic>\n</content>\n"
    );
    let qt = "[Renderer]\nuse_vsync\\default=false\nuse_vsync=false\nresolution_factor=2\n";
    let out = merge(Format::QtIni, qt, &[], &["[Renderer]use_vsync".into()]).unwrap();
    assert_eq!(out, "[Renderer]\nresolution_factor=2\n");
}
