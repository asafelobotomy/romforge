use super::*;

/// Recompute matches and statuses. Files in `relook` are looked up in the
/// DATs again; every file's status (and whether each match is in its
/// folder's scope) is worked out again from its matches, since which copy
/// is the duplicate depends on all of them: the most complete one wins.
pub fn identify(tx: &Transaction, relook: Relook) -> Result<(), Error> {
    let files: Vec<FileRow> = {
        let mut stmt = tx.prepare(
            "SELECT f.file_id, s.system_id, s.dat_id, f.size, f.crc32, f.md5, f.sha1,
                    f.stripped_size, f.stripped_crc32, f.stripped_md5, f.stripped_sha1, f.status,
                    f.path, f.member, f.chd_sha1
             FROM files f JOIN scan_roots s ON s.root_id = f.root_id
             ORDER BY f.path, f.member",
        )?;
        stmt.query_map([], |r| {
            let stripped = match r.get::<_, Option<u64>>(7)? {
                Some(size) => Some(Digest {
                    size,
                    crc32: r.get(8)?,
                    md5: r.get(9)?,
                    sha1: r.get(10)?,
                }),
                None => None,
            };
            let (path, member): (String, String) = (r.get(12)?, r.get(13)?);
            let place = if member.is_empty() {
                path.rsplit_once('/')
                    .map_or(String::new(), |(dir, _)| dir.to_owned())
            } else {
                path
            };
            Ok(FileRow {
                file_id: r.get(0)?,
                chd_sha1: r.get(14)?,
                place,
                scope: Scope::from_columns(r.get(1)?, r.get(2)?),
                raw: Digest {
                    size: r.get(3)?,
                    crc32: r.get(4)?,
                    md5: r.get(5)?,
                    sha1: r.get(6)?,
                },
                stripped,
                status: r.get(11)?,
            })
        })?
        .collect::<Result<_, _>>()?
    };

    // Files the owner named in Search, by content.
    let manual: std::collections::HashSet<(String, u64)> = tx
        .prepare("SELECT sha1, size FROM manual_ids")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    // Sheet matches are worked out again below.
    tx.execute("DELETE FROM matches WHERE via = 'sheet'", [])?;
    let relook_all = matches!(relook, Relook::All);
    if relook_all {
        tx.execute("DELETE FROM matches", [])?;
    }
    let mut insert = tx.prepare(
        "INSERT INTO matches (file_id, rom_id, via, stripped, out_of_scope) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut drop_matches = tx.prepare("DELETE FROM matches WHERE file_id = ?1")?;
    let mut set_scope =
        tx.prepare("UPDATE matches SET out_of_scope = ?3 WHERE file_id = ?1 AND rom_id = ?2")?;
    let mut set_status = tx.prepare("UPDATE files SET status = ?2 WHERE file_id = ?1")?;
    // The stored matches of the files not looked up again, with what decides
    // their scope (their ROM's DAT and system).
    let mut stored: HashMap<i64, Vec<(RomRow, Via, bool, bool)>> = HashMap::new();
    if !relook_all {
        let mut stmt = tx.prepare(
            "SELECT m.file_id, m.rom_id, m.via, m.stripped, m.out_of_scope, g.dat_id, d.system_id,
                    r.game_id
             FROM matches m JOIN roms r ON r.rom_id = m.rom_id
             JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })? {
            let (file, rom_id, via, stripped, out, dat_id, system_id, game_id) = row?;
            let Some(via) = Via::parse(&via) else { continue };
            let rom = RomRow {
                rom_id,
                game_id,
                dat_id,
                system_id,
                size: None,
                crc32: None,
                md5: None,
                sha1: None,
            };
            stored.entry(file).or_default().push((rom, via, stripped, out));
        }
    }
    // First every file's hits, then who owns which ROM.
    let mut all_hits: Vec<Vec<Hit>> = Vec::with_capacity(files.len());
    for file in &files {
        let again = match relook {
            Relook::All => true,
            Relook::Files(ids) => ids.contains(&file.file_id),
            Relook::Nothing => false,
        };
        let mut hits: Vec<Hit> = Vec::new();
        if again {
            if !relook_all {
                drop_matches.execute([file.file_id])?;
            }
            let mut seen = HashSet::new();
            let digests = std::iter::once((&file.raw, false)).chain(file.stripped.iter().map(|d| (d, true)));
            // An empty file says nothing: TOSEC lists many empty ROMs.
            for (digest, stripped) in digests.filter(|(d, _)| d.size > 0) {
                let roms = lookup(tx, digest)?;
                let vias: Vec<Option<Via>> = roms.iter().map(|r| compare(digest, r)).collect();
                // Its SHA1 fits a ROM of some DAT (size and CRC32 with it).
                let sha1_fits = vias.contains(&Some(Via::Sha1));
                for (rom, via) in roms.iter().zip(vias) {
                    let via = match via {
                        Some(Via::Crc32)
                            if sha1_fits
                                && rom.sha1.is_none()
                                && rom.md5.is_none()
                                && rom
                                    .system_id
                                    .and_then(crate::systems::by_id)
                                    .is_some_and(|s| s.arcade) =>
                        {
                            Some(Via::Confirmed)
                        }
                        v => v,
                    };
                    if let Some(via) = via
                        && seen.insert(rom.rom_id)
                    {
                        hits.push(Hit {
                            rom_id: rom.rom_id,
                            game_id: rom.game_id,
                            via,
                            stripped,
                            in_scope: file.scope.contains(rom),
                        });
                    }
                }
            }
            // A CHD: an arcade DAT's disk, by the SHA1 its header keeps.
            if let Some(sha1) = &file.chd_sha1 {
                for rom in disks(tx, sha1)? {
                    if seen.insert(rom.rom_id) {
                        hits.push(Hit {
                            rom_id: rom.rom_id,
                            game_id: rom.game_id,
                            via: Via::Sha1,
                            stripped: false,
                            in_scope: file.scope.contains(&rom),
                        });
                    }
                }
            }
            for h in &hits {
                insert.execute(params![
                    file.file_id,
                    h.rom_id,
                    h.via.as_str(),
                    h.stripped,
                    !h.in_scope
                ])?;
            }
        } else {
            for (rom, via, stripped, was_out) in stored.remove(&file.file_id).unwrap_or_default() {
                let in_scope = file.scope.contains(&rom);
                // The folder's scope may have changed since.
                if in_scope == was_out {
                    set_scope.execute(params![file.file_id, rom.rom_id, !in_scope])?;
                }
                hits.push(Hit {
                    rom_id: rom.rom_id,
                    game_id: rom.game_id,
                    via,
                    stripped,
                    in_scope,
                });
            }
        }
        all_hits.push(hits);
    }

    // Each ROM is owned by the copy whose place (an archive or a folder)
    // holds the most of that ROM's game; the other copies are duplicates.
    // Ties go by path. Per ROM, not per file: a track two discs share
    // belongs, on each disc, to the copy beside that disc's other tracks.
    let mut held: HashMap<(&str, i64), HashSet<i64>> = HashMap::new();
    for (file, hits) in files.iter().zip(&all_hits) {
        for h in hits.iter().filter(|h| h.in_scope && h.via.strong()) {
            held.entry((file.place.as_str(), h.game_id))
                .or_default()
                .insert(h.rom_id);
        }
    }
    // ROM id -> (how much of its game the owner's place holds, owner index).
    let mut owners: HashMap<i64, (usize, usize)> = HashMap::new();
    for (i, (file, hits)) in files.iter().zip(&all_hits).enumerate() {
        for h in hits.iter().filter(|h| h.in_scope && h.via.strong()) {
            let score = held
                .get(&(file.place.as_str(), h.game_id))
                .map_or(0, HashSet::len);
            let best = owners.entry(h.rom_id).or_insert((score, i));
            if score > best.0 {
                *best = (score, i);
            }
        }
    }
    for (i, (file, hits)) in files.iter().zip(&all_hits).enumerate() {
        let strong_in_scope: Vec<i64> = hits
            .iter()
            .filter(|h| h.in_scope && h.via.strong())
            .map(|h| h.rom_id)
            .collect();
        let status = if !strong_in_scope.is_empty() {
            if strong_in_scope
                .iter()
                .any(|r| owners.get(r).is_some_and(|o| o.1 == i))
            {
                Status::Matched
            } else {
                Status::Duplicate
            }
        } else if manual.contains(&(file.raw.sha1.clone(), file.raw.size)) {
            // The owner's pick outranks weak evidence (a CRC32-only hit, a
            // hash hit outside the folder's scope); only a match in scope
            // comes first.
            Status::Manual
        } else if hits.iter().any(|h| h.in_scope) {
            Status::Candidate
        } else if hits.iter().any(|h| h.via.strong()) {
            Status::WrongSystem
        } else {
            Status::Unknown
        };
        if status.as_str() != file.status {
            set_status.execute(params![file.file_id, status.as_str()])?;
        }
    }
    // Record who owns what, changing only what changed: the library lists
    // a game with the copies that own its ROMs (and their duplicates).
    let had: HashSet<(i64, i64)> = tx
        .prepare("SELECT file_id, rom_id FROM matches WHERE owns = 1")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let want: HashSet<(i64, i64)> = owners
        .iter()
        .map(|(rom, (_, i))| (files[*i].file_id, *rom))
        .collect();
    let mut set_owns = tx.prepare("UPDATE matches SET owns = ?3 WHERE file_id = ?1 AND rom_id = ?2")?;
    for (f, r) in had.difference(&want) {
        set_owns.execute(params![f, r, false])?;
    }
    for (f, r) in want.difference(&had) {
        set_owns.execute(params![f, r, true])?;
    }
    fit_sheets(tx)?;
    Ok(())
}
