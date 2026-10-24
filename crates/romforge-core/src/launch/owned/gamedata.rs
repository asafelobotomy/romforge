//! RPCS3, Cemu and xemu: romforge's own config like the others, but their
//! game data (installed games, firmware, the Xbox hard disk) stays where
//! it already is, and romforge's config points at it; nothing big is
//! copied. Their BIOS files come from `Bios/<emulator>/`. Read from their
//! source at RPCS3 0.0.43, Cemu 2.6 and xemu 0.8.137:
//!
//! - RPCS3: `XDG_CONFIG_HOME` → `rpcs3/`. `vfs.yml` maps each device to a
//!   folder (absolute paths are fine, keys left out keep their defaults);
//!   the user's own `dev_hdd0`, `dev_flash`… are found in their usual
//!   folder or their own `vfs.yml`. `GuiConfigs/CurrentSettings.ini`
//!   `[main_window] infoBoxEnabledWelcome=false` keeps the welcome dialog
//!   away. (Its Flatpak still asks about being an unofficial build: no
//!   setting turns that off.)
//! - Cemu: `XDG_CONFIG_HOME` / `XDG_DATA_HOME` → `Cemu/`. `settings.xml`
//!   must exist or "Getting started" opens; `<mlc_path>` is the user's
//!   existing MLC (installed titles, updates, DLC, saves): their own
//!   setting, else their `mlc01`. `keys.txt`, `otp.bin`, `seeprom.bin` are
//!   read from the data folder, so they are linked in.
//! - xemu: `-config_path` (table) to romforge's `xemu.toml`;
//!   `show_welcome = false` or it doesn't start the game. `hdd_path` and
//!   `eeprom_path` are the user's own (from their `xemu.toml`), else the
//!   files in `Bios/xemu/`; `eeprom_path` is always set, or xemu would make
//!   one in the user's own xemu folder. `bootrom_path` / `flashrom_path`
//!   from `Bios/xemu/`.

use std::path::{Path, PathBuf};

use super::existing::UserDirs;
use super::formats::toml_string;
use super::theirs::{RPCS3_DEVICES, cemu_mlc, moved_in, rpcs3_devices, xemu_file};
use super::{ConfigWrite, Format, Prepare};

pub(super) fn gamedata(name: &str, dir: &Path, bios: &Path, user: Option<&UserDirs>) -> Option<Prepare> {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut p = Prepare {
        dirs: vec![dir.to_path_buf(), bios.to_path_buf()],
        ..Prepare::default()
    };
    match name {
        "rpcs3" => {
            p.env = vec![("XDG_CONFIG_HOME".into(), s(dir))];
            // Your devices where they are, or RomForge's once moved in.
            let devices: Vec<(&str, PathBuf)> = if moved_in(dir) {
                RPCS3_DEVICES
                    .iter()
                    .map(|d| (*d, dir.join("rpcs3").join(d)))
                    .filter(|(_, p)| p.exists())
                    .collect()
            } else {
                user.map(rpcs3_devices).unwrap_or_default()
            };
            let keys = devices
                .into_iter()
                .map(|(dev, path)| (format!("/{dev}/"), toml_string(&format!("{}/", s(&path)))))
                .collect();
            p.writes.push(ConfigWrite {
                path: dir.join("rpcs3/vfs.yml"),
                format: Format::Yaml,
                keys,
                remove: Vec::new(),
            });
            p.writes.push(ConfigWrite {
                path: dir.join("rpcs3/GuiConfigs/CurrentSettings.ini"),
                format: Format::Ini,
                keys: vec![("[main_window]infoBoxEnabledWelcome".into(), "false".into())],
                remove: Vec::new(),
            });
        }
        "cemu" => {
            let data = dir.join("data");
            p.env = vec![
                ("XDG_CONFIG_HOME".into(), s(dir)),
                ("XDG_DATA_HOME".into(), s(&data)),
            ];
            let mlc = if moved_in(dir) {
                Some(data.join("Cemu/mlc01"))
            } else {
                user.and_then(cemu_mlc)
            }
            .map(|m| s(&m));
            p.writes.push(ConfigWrite {
                path: dir.join("Cemu/settings.xml"),
                format: Format::Xml,
                keys: mlc.map(|m| ("mlc_path".to_owned(), m)).into_iter().collect(),
                remove: Vec::new(),
            });
            p.links.push((bios.to_path_buf(), data.join("Cemu")));
            p.dirs.push(data);
        }
        "xemu" => {
            // Yours where your xemu.toml says, or RomForge's once moved in.
            let their = |key: &str| {
                user.filter(|_| !moved_in(dir))
                    .and_then(|u| xemu_file(u, key))
                    .map(|p| s(&p))
            };
            let ours = |file: &str| Some(bios.join(file)).filter(|f| f.is_file()).map(|f| s(&f));
            let mut keys = vec![("[general]show_welcome".to_owned(), "false".to_owned())];
            let files = [
                ("bootrom_path", ours("mcpx_1.0.bin")),
                ("flashrom_path", ours("Complex_4627.bin")),
                ("hdd_path", their("hdd_path").or_else(|| ours("xbox_hdd.qcow2"))),
                (
                    "eeprom_path",
                    their("eeprom_path")
                        .or_else(|| ours("eeprom.bin"))
                        .or_else(|| Some(s(&dir.join("eeprom.bin")))),
                ),
            ];
            for (key, value) in files {
                if let Some(v) = value {
                    keys.push((format!("[sys.files]{key}"), toml_string(&v)));
                }
            }
            p.writes.push(ConfigWrite {
                path: dir.join("xemu.toml"),
                format: Format::Toml,
                keys,
                remove: Vec::new(),
            });
        }
        _ => return None,
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::super::existing::Xdg;
    use super::super::prepare;
    use super::*;

    fn user(t: &Path) -> UserDirs {
        UserDirs {
            home: t.join("u"),
            xdg: Xdg {
                config: t.join("u/.config"),
                data: t.join("u/.local/share"),
            },
        }
    }

    #[test]
    fn game_data_stays_where_it_is() {
        let t = tempfile::tempdir().unwrap();
        let u = user(t.path());
        let (dir, bios) = (t.path().join("cfg"), t.path().join("Bios/x"));
        let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap();

        // RPCS3: the user's devices, one moved to another drive in their
        // own vfs.yml.
        let theirs = u.xdg.config.join("rpcs3");
        std::fs::create_dir_all(theirs.join("dev_flash")).unwrap();
        let hdd = t.path().join("big/hdd0/");
        std::fs::create_dir_all(&hdd).unwrap();
        std::fs::write(
            theirs.join("vfs.yml"),
            format!("$(EmulatorDir): \"\"\n/dev_hdd0/: {}\n", hdd.display()),
        )
        .unwrap();
        prepare(&gamedata("rpcs3", &dir, &bios, Some(&u)).unwrap()).unwrap();
        let vfs = read("rpcs3/vfs.yml");
        assert!(
            vfs.contains(&format!("/dev_hdd0/: \"{}\"", hdd.display())),
            "{vfs}"
        );
        assert!(
            vfs.contains(&format!("/dev_flash/: \"{}/dev_flash/\"", theirs.display())),
            "{vfs}"
        );
        assert!(
            !vfs.contains("dev_hdd1"),
            "no folder there: its default (romforge's) stays"
        );
        assert!(read("rpcs3/GuiConfigs/CurrentSettings.ini").contains("infoBoxEnabledWelcome = false"));

        // Cemu: the user's MLC by their own setting; keys linked in.
        let mlc = t.path().join("wiiu/mlc");
        std::fs::create_dir_all(&mlc).unwrap();
        std::fs::create_dir_all(u.xdg.config.join("Cemu")).unwrap();
        std::fs::write(
            u.xdg.config.join("Cemu/settings.xml"),
            format!("<content><mlc_path>{}</mlc_path></content>", mlc.display()),
        )
        .unwrap();
        std::fs::create_dir_all(&bios).unwrap();
        std::fs::write(bios.join("keys.txt"), "k").unwrap();
        let cemu = gamedata("cemu", &dir, &bios, Some(&u)).unwrap();
        prepare(&cemu).unwrap();
        assert!(read("Cemu/settings.xml").contains(&format!("<mlc_path>{}</mlc_path>", mlc.display())));
        assert_eq!(read("data/Cemu/keys.txt"), "k");
        // No Cemu here before: the file still exists, so no first-run dialog.
        let fresh = t.path().join("fresh");
        prepare(&gamedata("cemu", &fresh, &bios, None).unwrap()).unwrap();
        assert!(
            std::fs::read_to_string(fresh.join("Cemu/settings.xml"))
                .unwrap()
                .contains("<content>")
        );

        // xemu: the user's hard disk; boot ROM from Bios/; EEPROM always set.
        let disk = t.path().join("xbox.qcow2");
        std::fs::write(&disk, b"hdd").unwrap();
        let toml_dir = u.xdg.data.join("xemu/xemu");
        std::fs::create_dir_all(&toml_dir).unwrap();
        std::fs::write(
            toml_dir.join("xemu.toml"),
            format!("[sys.files]\nhdd_path = '{}'\n", disk.display()),
        )
        .unwrap();
        std::fs::write(bios.join("mcpx_1.0.bin"), [0u8; 512]).unwrap();
        prepare(&gamedata("xemu", &dir, &bios, Some(&u)).unwrap()).unwrap();
        let toml = read("xemu.toml");
        assert!(toml.contains("[general]\nshow_welcome = false"), "{toml}");
        assert!(
            toml.contains(&format!("hdd_path = \"{}\"", disk.display())),
            "{toml}"
        );
        assert!(toml.contains(&format!(
            "bootrom_path = \"{}\"",
            bios.join("mcpx_1.0.bin").display()
        )));
        assert!(toml.contains(&format!("eeprom_path = \"{}\"", dir.join("eeprom.bin").display())));
        assert!(!toml.contains("flashrom_path"), "none in Bios/ yet");
    }
}
