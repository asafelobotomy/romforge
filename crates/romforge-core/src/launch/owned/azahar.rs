//! Milestone 24: Azahar and its discontinued predecessors Lime3DS and
//! Citra (Nintendo 3DS), from romforge's own config.

use std::path::Path;

use super::existing::UserDirs;
use super::theirs::{citra_storage, moved_in};
use super::{ConfigWrite, Format, Prepare};

/// One of Citra's line: its folder name (data and config alike), the
/// folders of its own and older lines it reads (never a newer one's
/// NAND), and its update-check key.
struct Kind {
    name: &'static str,
    folder: &'static str,
    reads: &'static [&'static str],
    update_check: &'static str,
}

/// Read from each one's `common_paths.h`, `file_util.cpp` and
/// `configuration/config.cpp`: Azahar 2126.2, Lime3DS 2119.1, Citra's last
/// build (`launch/table.rs`).
const KINDS: &[Kind] = &[
    Kind {
        name: "azahar",
        folder: "azahar-emu",
        reads: &["azahar-emu", "lime3ds-emu", "citra-emu"],
        update_check: "[Miscellaneous]check_for_update_on_start",
    },
    Kind {
        name: "lime3ds",
        folder: "lime3ds-emu",
        reads: &["lime3ds-emu", "citra-emu"],
        update_check: "[UI]Updater\\check_for_update_on_start",
    },
    Kind {
        name: "citra",
        folder: "citra-emu",
        reads: &["citra-emu"],
        update_check: "[UI]Updater\\check_for_update_on_start",
    },
];

/// `name`'s folder and the folders it reads, when it is one of Citra's line.
pub(crate) fn folders(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    KINDS.iter().find(|k| k.name == name).map(|k| (k.folder, k.reads))
}

/// Azahar, Lime3DS or Citra (`name`): its data (`sysdata/`, `nand/`,
/// `sdmc/`) in `$XDG_DATA_HOME/<folder>`, its config in
/// `$XDG_CONFIG_HOME/<folder>/qt-config.ini`. Both are romforge's
/// (`Bios/<name>/sysdata/` linked in); the console's NAND and SD card are
/// the user's own (their data of that emulator or an older one, XDG or
/// Flatpak) through `[Data Storage] use_custom_storage`, else `saves` (the
/// system's folder in `Saves/`). The first-start prompt and update check
/// are off.
pub(super) fn citra_line(
    name: &str,
    dir: &Path,
    bios: &Path,
    saves: &Path,
    user: Option<&UserDirs>,
) -> Option<Prepare> {
    let kind = KINDS.iter().find(|k| k.name == name)?;
    let mut p = Prepare {
        dirs: vec![dir.to_path_buf(), bios.to_path_buf()],
        ..Prepare::default()
    };
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let data = dir.join("data");
    p.env = vec![
        ("XDG_CONFIG_HOME".into(), s(dir)),
        ("XDG_DATA_HOME".into(), s(&data)),
    ];
    // Yours where it is, or RomForge's once moved in.
    let theirs = user
        .filter(|_| !moved_in(dir))
        .and_then(|u| citra_storage(u, kind.reads));
    // RetroBIOS's standalone places: `sysdata/…` in the data folder, the
    // console's own files in `nand/…`, linked only into a NAND that is
    // romforge's, never into the user's.
    if theirs.is_none() {
        p.links.push((bios.join("nand"), saves.join("nand")));
    }
    let storage = theirs.unwrap_or_else(|| saves.to_path_buf());
    let off = |k: &str| {
        [
            (k.to_owned(), "false".to_owned()),
            (format!("{k}\\default"), "false".to_owned()),
        ]
    };
    let folder = |d: &str| format!("{}/", s(&storage.join(d)));
    let mut keys: Vec<(String, String)> = [off("[UI]firstStart"), off(kind.update_check)].concat();
    keys.extend([
        ("[Data%20Storage]use_custom_storage".to_owned(), "true".to_owned()),
        (
            "[Data%20Storage]use_custom_storage\\default".to_owned(),
            "false".to_owned(),
        ),
        ("[Data%20Storage]nand_directory".to_owned(), folder("nand")),
        ("[Data%20Storage]sdmc_directory".to_owned(), folder("sdmc")),
    ]);
    p.dirs.extend([storage.join("nand"), storage.join("sdmc")]);
    p.writes.push(ConfigWrite {
        path: dir.join(kind.folder).join("qt-config.ini"),
        format: Format::Ini,
        keys,
        remove: Vec::new(),
    });
    p.links
        .push((bios.join("sysdata"), data.join(kind.folder).join("sysdata")));
    p.dirs.push(data);
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::super::existing::Xdg;
    use super::super::prepare;
    use super::*;

    #[test]
    fn azahar_keeps_the_user_s_nand_and_sd_card() {
        let t = tempfile::tempdir().unwrap();
        let (dir, bios, saves) = (
            t.path().join("cfg"),
            t.path().join("Bios/azahar"),
            t.path().join("Saves/Nintendo 3DS"),
        );
        for f in ["sysdata/keys.txt", "nand/private/movable.sed"] {
            std::fs::create_dir_all(bios.join(f).parent().unwrap()).unwrap();
            std::fs::write(bios.join(f), f).unwrap();
        }
        let ini = |d: &Path| {
            std::fs::read_to_string(d.join("azahar-emu/qt-config.ini"))
                .unwrap()
                .replace(" = ", "=")
        };

        // Nothing of theirs: the NAND and SD card in Saves/, BIOS files in.
        let p = citra_line("azahar", &dir, &bios, &saves, None).unwrap();
        prepare(&p).unwrap();
        let text = ini(&dir);
        assert!(
            text.contains("[Data%20Storage]") && text.contains("use_custom_storage=true"),
            "{text}"
        );
        assert!(text.contains(&format!("nand_directory={}/", saves.join("nand").display())));
        assert!(text.contains("firstStart=false") && text.contains("check_for_update_on_start=false"));
        let linked = dir.join("data/azahar-emu/sysdata/keys.txt");
        assert!(linked.is_file());
        // A symbolic link to the one copy in Bios/.
        assert!(linked.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&linked).unwrap(),
            std::path::absolute(bios.join("sysdata/keys.txt")).unwrap()
        );
        assert!(saves.join("nand/private/movable.sed").is_file());
        assert!(saves.join("sdmc").is_dir());
        assert!(
            p.env
                .iter()
                .any(|(k, v)| k == "XDG_DATA_HOME" && v.ends_with("cfg/data"))
        );

        // Their Citra data (Flatpak): used as it is, nothing linked into it.
        let home = t.path().join("u");
        let theirs = home.join(".var/app/org.citra_emu.citra/data/citra-emu");
        std::fs::create_dir_all(theirs.join("nand")).unwrap();
        let user = UserDirs {
            home: home.clone(),
            xdg: Xdg {
                config: home.join(".config"),
                data: home.join(".local/share"),
            },
        };
        let dir2 = t.path().join("cfg2");
        prepare(&citra_line("azahar", &dir2, &bios, &t.path().join("unused"), Some(&user)).unwrap()).unwrap();
        let text = ini(&dir2);
        assert!(
            text.contains(&format!("sdmc_directory={}/", theirs.join("sdmc").display())),
            "{text}"
        );
        assert!(!theirs.join("nand/private/movable.sed").exists());
        assert!(!t.path().join("unused").exists());

        // Lime3DS: its own folder and update key; Citra's data read too,
        // but never a newer emulator's.
        let azahars = home.join(".local/share/azahar-emu");
        std::fs::create_dir_all(azahars.join("nand")).unwrap();
        let dir3 = t.path().join("cfg3");
        let lime = citra_line("lime3ds", &dir3, &bios, &saves, Some(&user)).unwrap();
        prepare(&lime).unwrap();
        let text = std::fs::read_to_string(dir3.join("lime3ds-emu/qt-config.ini"))
            .unwrap()
            .replace(" = ", "=");
        assert!(
            text.contains(r"Updater\check_for_update_on_start=false"),
            "{text}"
        );
        assert!(text.contains(&format!("nand_directory={}/", theirs.join("nand").display())));
        assert!(dir3.join("data/lime3ds-emu/sysdata/keys.txt").is_file());
        assert!(citra_line("dolphin", &dir3, &bios, &saves, None).is_none());
    }
}
