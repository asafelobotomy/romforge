//! Standalone emulators running from romforge's own data folder (`dir`,
//! `RomForge/Emulators/config/<emulator>/`), their BIOS files from
//! `RomForge/Bios/<emulator>/` (milestone 20, step 4). How each is pointed
//! at its folder, which keys it needs and where it looks for BIOS files
//! were read from its source at the release named:
//!
//! - DuckStation (rolling, 2026-10): `XDG_CONFIG_HOME` → `duckstation/`;
//!   `settings.ini` `[BIOS] SearchDirectory` (it scans the folder and
//!   tells BIOS files by hash); `[Main] SetupWizardIncomplete = false`, or
//!   its setup wizard opens.
//! - PCSX2 2.8.2: `-datapath` (table) → `PCSX2/inis/PCSX2.ini`,
//!   `[Folders] Bios`; `[UI] SettingsVersion = 1` or every setting is
//!   reset, and the setup wizard off.
//! - mGBA 0.10.5: `XDG_CONFIG_HOME` → `mgba/config.ini`, `[ports.qt]`
//!   `gba.bios` / `gb.bios` / `gbc.bios` / `sgb.bios` as absolute paths,
//!   set only for files there are.
//! - Mednafen 1.32.1: `MEDNAFEN_HOME`; `mednafen.cfg`
//!   `filesys.path_firmware` (its per-system defaults are the usual names).
//! - Flycast 2.7: `XDG_CONFIG_HOME` and `XDG_DATA_HOME`; BIOS folders in
//!   `FLYCAST_BIOS_PATH` (colon-separated).
//! - Dolphin 2609a: `-u` (table). No key for the GameCube IPL or DSP ROMs
//!   (optional: it skips the IPL by default), so files are linked into the
//!   user folder where it reads them (`GC/<region>/IPL.bin`).
//! - PPSSPP 1.20.4: `XDG_CONFIG_HOME`; no BIOS.
//! - melonDS 1.1: `XDG_CONFIG_HOME` → `melonDS/melonDS.toml`
//!   (`main.cpp`, `Config.cpp`): `[DS] BIOS9Path`, `BIOS7Path`,
//!   `FirmwarePath` and `[DSi]`'s four, set only for files there are;
//!   `[Emu] ExternalBIOSEnable` only with all three DS files, else it uses
//!   its own free BIOS (a missing file with it on stops the game).
//!
//! Not here: RPCS3, Cemu and xemu keep firmware, installed games or a hard
//! disk in their data folder, so a folder of romforge's would hide them.

use std::path::{Path, PathBuf};

use super::formats::toml_string;
use super::{ConfigWrite, Format, Prepare};

/// What `name` needs to start from `dir` with its BIOS files in `bios`;
/// `None` for an emulator romforge doesn't set up.
pub(super) fn standalone(name: &str, dir: &Path, bios: &Path) -> Option<Prepare> {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let env = |pairs: &[(&str, String)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
    let ini = |path: PathBuf, keys: &[(&str, String)], remove: &[String]| ConfigWrite {
        path,
        format: Format::Ini,
        keys: keys.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        remove: remove.to_vec(),
    };
    let mut p = Prepare {
        dirs: vec![dir.to_path_buf(), bios.to_path_buf()],
        ..Prepare::default()
    };
    match name {
        "duckstation" => {
            p.env = env(&[("XDG_CONFIG_HOME", s(dir))]);
            p.writes.push(ini(
                dir.join("duckstation/settings.ini"),
                &[
                    ("[Main]SetupWizardIncomplete", "false".into()),
                    ("[BIOS]SearchDirectory", s(bios)),
                ],
                &[],
            ));
        }
        "pcsx2" => p.writes.push(ini(
            dir.join("PCSX2/inis/PCSX2.ini"),
            &[
                ("[UI]SettingsVersion", "1".into()),
                ("[UI]SetupWizardIncomplete", "false".into()),
                ("[Folders]Bios", s(bios)),
            ],
            &[],
        )),
        "mgba" => {
            p.env = env(&[("XDG_CONFIG_HOME", s(dir))]);
            let (mut keys, mut remove) = (Vec::new(), Vec::new());
            for (key, file) in [
                ("gba.bios", "gba_bios.bin"),
                ("gb.bios", "gb_bios.bin"),
                ("gbc.bios", "gbc_bios.bin"),
                ("sgb.bios", "sgb_bios.bin"),
            ] {
                let path = bios.join(file);
                if path.is_file() {
                    keys.push((format!("[ports.qt]{key}"), s(&path)));
                } else {
                    remove.push(format!("[ports.qt]{key}"));
                }
            }
            p.writes.push(ConfigWrite {
                path: dir.join("mgba/config.ini"),
                format: Format::Ini,
                keys,
                remove,
            });
        }
        "mednafen" => {
            p.env = env(&[("MEDNAFEN_HOME", s(dir))]);
            p.writes.push(ConfigWrite {
                path: dir.join("mednafen.cfg"),
                format: Format::Spaced,
                keys: vec![("filesys.path_firmware".into(), s(bios))],
                remove: Vec::new(),
            });
        }
        "flycast" => {
            let data = dir.join("data");
            p.env = env(&[
                ("XDG_CONFIG_HOME", s(dir)),
                ("XDG_DATA_HOME", s(&data)),
                (
                    "FLYCAST_BIOS_PATH",
                    format!("{}:{}", s(bios), s(&bios.join("dc"))),
                ),
            ]);
            p.dirs.push(data);
        }
        "dolphin" => p.links.push((bios.to_path_buf(), dir.to_path_buf())),
        "ppsspp" => p.env = env(&[("XDG_CONFIG_HOME", s(dir))]),
        "melonds" => {
            p.env = env(&[("XDG_CONFIG_HOME", s(dir))]);
            let (mut keys, mut remove) = (Vec::new(), Vec::new());
            let mut ds = 0;
            for (key, file) in [
                ("[DS]BIOS9Path", "bios9.bin"),
                ("[DS]BIOS7Path", "bios7.bin"),
                ("[DS]FirmwarePath", "firmware.bin"),
                ("[DSi]BIOS9Path", "dsi_bios9.bin"),
                ("[DSi]BIOS7Path", "dsi_bios7.bin"),
                ("[DSi]FirmwarePath", "dsi_firmware.bin"),
                ("[DSi]NANDPath", "dsi_nand.bin"),
            ] {
                let path = bios.join(file);
                if path.is_file() {
                    ds += usize::from(key.starts_with("[DS]"));
                    keys.push((key.to_owned(), toml_string(&s(&path))));
                } else {
                    remove.push(key.to_owned());
                }
            }
            keys.push(("[Emu]ExternalBIOSEnable".into(), (ds == 3).to_string()));
            p.writes.push(ConfigWrite {
                path: dir.join("melonDS/melonDS.toml"),
                format: Format::Toml,
                keys,
                remove,
            });
        }
        _ => return None,
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::super::prepare;
    use super::*;

    #[test]
    fn each_emulator_gets_its_folder_and_its_bios() {
        let t = tempfile::tempdir().unwrap();
        let (dir, bios) = (t.path().join("cfg"), t.path().join("Bios/x"));
        let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap();

        let duck = standalone("duckstation", &dir, &bios).unwrap();
        assert_eq!(
            duck.env,
            [("XDG_CONFIG_HOME".to_owned(), dir.display().to_string())]
        );
        prepare(&duck).unwrap();
        let ini = read("duckstation/settings.ini");
        assert!(ini.contains("[Main]\nSetupWizardIncomplete = false\n"), "{ini}");
        assert!(
            ini.contains(&format!("[BIOS]\nSearchDirectory = {}\n", bios.display())),
            "{ini}"
        );

        prepare(&standalone("pcsx2", &dir, &bios).unwrap()).unwrap();
        let ini = read("PCSX2/inis/PCSX2.ini");
        assert!(
            ini.contains("[UI]\nSettingsVersion = 1\nSetupWizardIncomplete = false\n"),
            "{ini}"
        );

        // mGBA: only the BIOS files there are.
        std::fs::write(bios.join("gba_bios.bin"), b"bios").unwrap();
        prepare(&standalone("mgba", &dir, &bios).unwrap()).unwrap();
        let ini = read("mgba/config.ini");
        assert!(
            ini.contains(&format!("gba.bios = {}", bios.join("gba_bios.bin").display())),
            "{ini}"
        );
        assert!(!ini.contains("gb.bios"));

        prepare(&standalone("mednafen", &dir, &bios).unwrap()).unwrap();
        assert_eq!(
            read("mednafen.cfg"),
            format!("filesys.path_firmware {}\n", bios.display())
        );

        let fly = standalone("flycast", &dir, &bios).unwrap();
        let path = fly.env.iter().find(|(k, _)| k == "FLYCAST_BIOS_PATH").unwrap();
        assert_eq!(path.1, format!("{}:{}/dc", bios.display(), bios.display()));

        // Dolphin: files linked in where it reads them; one there stays.
        std::fs::create_dir_all(bios.join("GC/USA")).unwrap();
        std::fs::write(bios.join("GC/USA/IPL.bin"), b"ipl").unwrap();
        std::fs::create_dir_all(dir.join("GC/USA")).unwrap();
        std::fs::write(dir.join("gba_bios.bin"), b"mine").unwrap();
        prepare(&standalone("dolphin", &dir, &bios).unwrap()).unwrap();
        assert_eq!(read("GC/USA/IPL.bin"), "ipl");
        assert_eq!(read("gba_bios.bin"), "mine", "never replaced");

        // melonDS: the files there are, and its own BIOS until all three
        // DS files are.
        let ds = t.path().join("Bios/ds");
        std::fs::create_dir_all(&ds).unwrap();
        std::fs::write(ds.join("bios9.bin"), b"9").unwrap();
        prepare(&standalone("melonds", &dir, &ds).unwrap()).unwrap();
        let toml = read("melonDS/melonDS.toml");
        assert!(
            toml.contains(&format!("BIOS9Path = \"{}\"", ds.join("bios9.bin").display())),
            "{toml}"
        );
        assert!(toml.contains("ExternalBIOSEnable = false"), "{toml}");
        assert!(!toml.contains("BIOS7Path"), "{toml}");
        for f in ["bios7.bin", "firmware.bin"] {
            std::fs::write(ds.join(f), b"x").unwrap();
        }
        prepare(&standalone("melonds", &dir, &ds).unwrap()).unwrap();
        assert!(read("melonDS/melonDS.toml").contains("ExternalBIOSEnable = true"));

        for none in ["rpcs3", "cemu", "xemu", "scummvm"] {
            assert!(standalone(none, &dir, &bios).is_none(), "{none}");
        }
    }

    /// melonDS's DSi files are set like the DS ones, and a key for a file
    /// no longer there is taken out.
    #[test]
    fn melonds_dsi_keys_and_files_gone() {
        let t = tempfile::tempdir().unwrap();
        let (dir, bios) = (t.path().join("cfg"), t.path().join("Bios/melonDS"));
        std::fs::create_dir_all(&bios).unwrap();
        for f in [
            "bios9.bin",
            "bios7.bin",
            "firmware.bin",
            "dsi_bios9.bin",
            "dsi_nand.bin",
        ] {
            std::fs::write(bios.join(f), b"x").unwrap();
        }
        let read = || std::fs::read_to_string(dir.join("melonDS/melonDS.toml")).unwrap();
        prepare(&standalone("melonds", &dir, &bios).unwrap()).unwrap();
        let toml = read();
        let dsi = toml.split("[DSi]").nth(1).unwrap_or_default().to_owned();
        let has = |text: &str, key: &str, file: &str| {
            text.contains(&format!("{key} = \"{}\"", bios.join(file).display()))
        };
        assert!(has(&dsi, "BIOS9Path", "dsi_bios9.bin"), "{toml}");
        assert!(has(&dsi, "NANDPath", "dsi_nand.bin"), "{toml}");
        assert!(!dsi.contains("FirmwarePath"), "{toml}");
        assert!(toml.contains("ExternalBIOSEnable = true"), "{toml}");

        std::fs::remove_file(bios.join("bios9.bin")).unwrap();
        std::fs::remove_file(bios.join("dsi_nand.bin")).unwrap();
        prepare(&standalone("melonds", &dir, &bios).unwrap()).unwrap();
        let toml = read();
        assert!(
            !toml.contains(&bios.join("bios9.bin").display().to_string()),
            "{toml}"
        );
        assert!(!toml.contains("NANDPath"), "{toml}");
        assert!(has(&toml, "BIOS7Path", "bios7.bin"), "{toml}");
        assert!(toml.contains("ExternalBIOSEnable = false"), "{toml}");
    }
}
