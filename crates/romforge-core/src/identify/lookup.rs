use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Via {
    Sha1,
    Md5,
    Crc32,
    /// An arcade DAT's ROM that has only a CRC32 (FBNeo's), its size and
    /// CRC32 fitting the file, whose SHA1 fits a ROM of the same size and
    /// CRC32 in another DAT (MAME's): size + SHA1 after all (milestone 17).
    Confirmed,
}

impl Via {
    pub(super) fn parse(s: &str) -> Option<Via> {
        match s {
            "sha1" => Some(Via::Sha1),
            "md5" => Some(Via::Md5),
            "crc32" => Some(Via::Crc32),
            "confirmed" => Some(Via::Confirmed),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Via::Sha1 => "sha1",
            Via::Md5 => "md5",
            Via::Crc32 => "crc32",
            Via::Confirmed => "confirmed",
        }
    }

    pub(super) fn strong(self) -> bool {
        self != Via::Crc32
    }
}

pub(super) struct RomRow {
    pub(super) rom_id: i64,
    pub(super) game_id: i64,
    pub(super) dat_id: i64,
    pub(super) system_id: Option<i64>,
    pub(super) size: Option<u64>,
    pub(super) crc32: Option<String>,
    pub(super) md5: Option<String>,
    pub(super) sha1: Option<String>,
}

pub(super) fn compare(d: &Digest, rom: &RomRow) -> Option<Via> {
    if rom.size != Some(d.size) {
        return None;
    }
    if let Some(sha1) = &rom.sha1 {
        return (*sha1 == d.sha1).then_some(Via::Sha1);
    }
    if let Some(md5) = &rom.md5 {
        return (*md5 == d.md5).then_some(Via::Md5);
    }
    (rom.crc32.as_deref() == Some(d.crc32.as_str())).then_some(Via::Crc32)
}

pub(super) struct Hit {
    pub(super) rom_id: i64,
    pub(super) game_id: i64,
    pub(super) via: Via,
    pub(super) stripped: bool,
    pub(super) in_scope: bool,
}

pub(super) fn lookup(conn: &Connection, d: &Digest) -> Result<Vec<RomRow>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.rom_id, g.dat_id, d.system_id, r.size, r.crc32, r.md5, r.sha1, r.game_id
         FROM roms r JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
         WHERE r.sha1 = ?1 OR r.md5 = ?2 OR r.crc32 = ?3",
    )?;
    let rows = stmt.query_map(params![d.sha1, d.md5, d.crc32], |r| {
        Ok(RomRow {
            rom_id: r.get(0)?,
            dat_id: r.get(1)?,
            system_id: r.get(2)?,
            size: r.get(3)?,
            crc32: r.get(4)?,
            md5: r.get(5)?,
            sha1: r.get(6)?,
            game_id: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The disks (MAME's `<disk>`) with this CHD SHA1.
pub(super) fn disks(conn: &Connection, sha1: &str) -> Result<Vec<RomRow>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.rom_id, g.dat_id, d.system_id, r.game_id
         FROM roms r JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
         WHERE r.disk = 1 AND r.sha1 = ?1",
    )?;
    let rows = stmt.query_map([sha1], |r| {
        Ok(RomRow {
            rom_id: r.get(0)?,
            dat_id: r.get(1)?,
            system_id: r.get(2)?,
            game_id: r.get(3)?,
            size: None,
            crc32: None,
            md5: None,
            sha1: Some(sha1.to_owned()),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
