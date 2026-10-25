//! A game's own settings (`emusettings::per_game`) written where its
//! emulator reads them, at its start, read in each one's source:
//! - RetroArch 1.22.2: a game override, `<rgui_config_directory>/<core's
//!   library name>/<content's name>.cfg` (`configuration.c`,
//!   `config_load_override`), read after `--appendconfig`, so it wins.
//!   romforge's `retroarch.cfg` points `rgui_config_directory` at
//!   `config/` beside it. The library name is the core info's `corename`.
//! - DuckStation: `gamesettings/<serial>.ini` beside `settings.ini`, the
//!   serial from the disc's boot file as DuckStation writes it
//!   (`SLUS_007.07` → `SLUS-00707`), the same keys as `settings.ini`.
//!
//! A setting not set for the game is taken out of its file; a game with
//! nothing set gets no file (one there, of the user's own, keeps their
//! other keys).

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use super::{ConfigWrite, Format, Prepare};
use crate::Error;
use crate::systems::System;

/// Put `emulator`'s keys for `game` into `file` in `p`.
fn put(
    conn: &Connection,
    system: &System,
    game: &str,
    emulator: &str,
    file: &Path,
    format: Format,
    p: &mut Prepare,
) -> Result<(), Error> {
    let (mut keys, mut remove) = (Vec::new(), Vec::new());
    for (k, v) in crate::emusettings::game_keys(conn, system.key, game, emulator)? {
        match v {
            Some(v) => keys.push((k.key.to_owned(), v)),
            None => remove.push(k.key.to_owned()),
        }
    }
    if keys.is_empty() && !file.exists() {
        return Ok(());
    }
    p.writes.push(ConfigWrite {
        path: file.to_path_buf(),
        format,
        keys,
        remove,
    });
    Ok(())
}

/// RetroArch's override for `game` (`content`, run by `core`).
pub(in crate::launch) fn retroarch(
    conn: &Connection,
    dir: &Path,
    core: &str,
    content: &Path,
    system: &System,
    game: &str,
    p: &mut Prepare,
) -> Result<(), Error> {
    let db_dir = conn.path().map(Path::new).and_then(Path::parent);
    let (Some(library), Some(stem)) = (crate::obtain::library_name(db_dir, core), content.file_stem()) else {
        return Ok(());
    };
    let file = dir
        .join("config")
        .join(library)
        .join(format!("{}.cfg", stem.to_string_lossy()));
    put(conn, system, game, "retroarch", &file, Format::RetroArch, p)
}

/// `SLUS_007.07` as DuckStation names a game by it: `SLUS-00707`.
pub(crate) fn duckstation_serial(boot: &str) -> String {
    boot.chars()
        .filter(|&c| c != '.')
        .map(|c| if c == '_' { '-' } else { c })
        .collect()
}

/// DuckStation's game settings for `game`, when its disc's serial is known.
pub(in crate::launch) fn duckstation(
    conn: &Connection,
    dir: &Path,
    system: &System,
    game: &str,
    p: &mut Prepare,
) -> Result<(), Error> {
    let id: Option<i64> = conn
        .query_row(
            "SELECT system_id FROM systems WHERE key = ?1",
            [system.key],
            |r| r.get(0),
        )
        .optional()?;
    let Some(id) = id else { return Ok(()) };
    let serial = crate::artsources::serial_for(conn, id, game, &["ps1"])?.and_then(|g| g.serial);
    let Some(serial) = serial.filter(|s| !s.contains(['/', '\\'])) else {
        return Ok(());
    };
    let file = dir
        .join("duckstation/gamesettings")
        .join(format!("{}.ini", duckstation_serial(&serial)));
    put(conn, system, game, "duckstation", &file, Format::Ini, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duckstation_names_a_game_by_its_boot_file() {
        assert_eq!(duckstation_serial("SLUS_007.07"), "SLUS-00707");
        assert_eq!(duckstation_serial("SCES_021.05"), "SCES-02105");
    }

    /// A game's override goes in its core's folder under RetroArch's
    /// `rgui_config_directory`; a setting it no longer has comes out.
    #[test]
    fn a_retroarch_game_gets_its_own_override() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&t.path().join("romforge.db")).unwrap();
        let mut w =
            zip::ZipWriter::new(std::fs::File::create(t.path().join("libretro-core-info.zip")).unwrap());
        w.start_file("mgba_libretro.info", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, b"corename = \"mGBA\"\n").unwrap();
        w.finish().unwrap();
        let gba = crate::systems::by_key("Game Boy Advance").unwrap();
        let dir = t.path().join("cfg");
        let run = |p: &mut Prepare| {
            retroarch(
                &conn,
                &dir,
                "mgba",
                Path::new("/g/Test GBA.zip"),
                gba,
                "Test GBA",
                p,
            )
        };

        let mut p = Prepare::default();
        run(&mut p).unwrap();
        assert!(p.writes.is_empty(), "nothing set: no file");

        let values = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let err = crate::emusettings::set_game_settings(
            &conn,
            gba.key,
            "Test GBA",
            &values(&[("fullscreen", "true")]),
        );
        assert!(err.is_err(), "launch settings aren't per game");
        crate::emusettings::set_game_settings(
            &conn,
            gba.key,
            "Test GBA",
            &values(&[("smooth", "true"), ("vsync", "false"), ("rewind", "")]),
        )
        .unwrap();
        let mut p = Prepare::default();
        run(&mut p).unwrap();
        super::super::prepare(&p).unwrap();
        let file = dir.join("config/mGBA/Test GBA.cfg");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("video_smooth = \"true\"") && text.contains("video_vsync = \"false\""),
            "{text}"
        );
        assert!(!text.contains("rewind"), "{text}");

        crate::emusettings::set_game_settings(&conn, gba.key, "Test GBA", &values(&[("smooth", "true")]))
            .unwrap();
        let mut p = Prepare::default();
        run(&mut p).unwrap();
        super::super::prepare(&p).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("video_vsync"), "unset for the game: out");
    }
}
