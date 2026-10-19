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
//!
//! Not here: RPCS3, Cemu and xemu keep firmware, installed games or a hard
//! disk in their data folder, so a folder of romforge's would hide them.

use std::path::{Path, PathBuf};

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

        for none in ["rpcs3", "cemu", "xemu", "scummvm"] {
            assert!(standalone(none, &dir, &bios).is_none(), "{none}");
        }
    }
}
