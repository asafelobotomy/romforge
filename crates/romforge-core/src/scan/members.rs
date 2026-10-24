//! Hashing one unit: a plain file, the members of a zip or 7z, or a CHD's
//! tracks.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use super::{Entry, ScanOptions, Stamp, Unit, Visit, wanted_member};
use crate::Error;
use crate::hash::{Hashes, hash_stream};

pub(super) fn hash_unit(unit: Unit, stamp: Option<Stamp>, opts: &ScanOptions, each: &mut dyn FnMut(Visit)) {
    // Every entry of the unit carries the file's stamp.
    let each = &mut |v: Visit| match v {
        Visit::Hashed(mut e) => {
            e.stamp = stamp;
            each(Visit::Hashed(e))
        }
        v => each(v),
    };
    let (path, result) = match unit {
        Unit::Zip(path, whole) => {
            let r = walk_zip(&path, &member_opts(opts, whole), each);
            (path, r)
        }
        Unit::SevenZ(path, whole) => {
            let r = walk_7z(&path, &member_opts(opts, whole), each);
            (path, r)
        }
        Unit::Chd(path) => {
            let r = crate::disc::chd_members(&path).map(|members| {
                for (member, hashes) in members {
                    each(Visit::Hashed(Entry {
                        stamp: None,
                        path: path.clone(),
                        member,
                        hashes: Ok(hashes),
                    }));
                }
            });
            (path, r)
        }
        Unit::File(path) => {
            let hashes = hash_file(&path).map_err(|e| e.to_string());
            each(Visit::Hashed(Entry {
                stamp: None,
                path,
                member: String::new(),
                hashes,
            }));
            return;
        }
    };
    if let Err(e) = result {
        each(Visit::Hashed(Entry {
            stamp: None,
            path,
            member: String::new(),
            hashes: Err(e.to_string()),
        }));
    }
}

/// The options for an archive's members: all of them for an arcade set.
fn member_opts(opts: &ScanOptions, whole: bool) -> ScanOptions {
    ScanOptions {
        all_files: opts.all_files || whole,
        ..opts.clone()
    }
}

fn hash_file(path: &Path) -> std::io::Result<Hashes> {
    // A Z3DS file (Azahar's compressed 3DS games) is the ROM it holds: its
    // hashes, size and header ID are that ROM's (milestone 24).
    if let Some((header, inner)) = crate::z3ds::open(BufReader::new(File::open(path)?))? {
        return hash_stream(inner, header.uncompressed_size);
    }
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hashes = hash_stream(BufReader::new(file), size)?;
    // A compressed disc image: its own header says what it holds.
    if hashes.id.is_none() {
        hashes.id = crate::ident::read_container(&mut BufReader::new(File::open(path)?));
    }
    // A Wii U title folder's game: its folder's `meta/meta.xml` says.
    if hashes.id.is_none() {
        hashes.id = crate::ident::wup::beside(path);
    }
    Ok(hashes)
}

fn walk_zip(path: &Path, opts: &ScanOptions, each: &mut dyn FnMut(Visit)) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    for i in 0..archive.len() {
        // One member that can't be opened (encrypted, an unsupported
        // method) is that member's error; the rest are still read.
        let name = match archive.name_for_index(i) {
            Some(Ok(n)) => n.into_owned(),
            _ => format!("#{i}"),
        };
        let mut member = match archive.by_index(i) {
            Ok(m) => m,
            Err(e) => {
                let path = path.to_path_buf();
                each(match wanted_member(&name, opts) {
                    true => Visit::Hashed(Entry {
                        stamp: None,
                        path,
                        member: name,
                        hashes: Err(e.to_string()),
                    }),
                    false => Visit::Skipped { path, member: name },
                });
                continue;
            }
        };
        if member.is_dir() {
            continue;
        }
        let name = member.name().map_or(name, |n| n.into_owned());
        if !wanted_member(&name, opts) {
            each(Visit::Skipped {
                path: path.to_path_buf(),
                member: name,
            });
            continue;
        }
        let size = member.size();
        let hashes = hash_stream(&mut member, size).map_err(|e| e.to_string());
        each(Visit::Hashed(Entry {
            stamp: None,
            path: path.to_path_buf(),
            member: name,
            hashes,
        }));
    }
    Ok(())
}

fn walk_7z(path: &Path, opts: &ScanOptions, each: &mut dyn FnMut(Visit)) -> Result<(), Error> {
    let mut archive = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
    archive.for_each_entries(|entry, reader| {
        if entry.is_directory() {
            // nothing to hash
        } else if !wanted_member(entry.name(), opts) {
            // A solid archive is one stream: a member not read to its end
            // would leave the next one starting at the wrong bytes.
            std::io::copy(reader, &mut std::io::sink())?;
            each(Visit::Skipped {
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
            });
        } else {
            let hashes = hash_stream(&mut *reader, entry.size()).map_err(|e| e.to_string());
            if hashes.is_err() {
                let _ = std::io::copy(reader, &mut std::io::sink());
            }
            each(Visit::Hashed(Entry {
                stamp: None,
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
                hashes,
            }));
        }
        Ok(true)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_z3ds_file_hashes_as_the_rom_it_holds() {
        let rom: Vec<u8> = (0..70_000u32).map(|i| (i * 7 % 256) as u8).collect();
        let t = tempfile::tempdir().unwrap();
        let (plain, packed) = (t.path().join("g.3ds"), t.path().join("g.zcci"));
        std::fs::write(&plain, &rom).unwrap();
        std::fs::write(&packed, crate::z3ds::tests::z3ds_of(&rom, *b"NCSD", 16)).unwrap();
        let (a, b) = (
            super::hash_file(&plain).unwrap(),
            super::hash_file(&packed).unwrap(),
        );
        assert_eq!(a.raw, b.raw, "same size and hashes");
        assert!(std::fs::metadata(&packed).unwrap().len() != rom.len() as u64);
        // Its header ID is the held file's: a CXI's product code.
        let mut cxi = vec![0u8; 0x400];
        cxi[0x100..0x104].copy_from_slice(b"NCCH");
        cxi[0x150..0x15A].copy_from_slice(b"CTR-P-AREE");
        std::fs::write(&packed, crate::z3ds::tests::z3ds_of(&cxi, *b"NCCH", 0)).unwrap();
        let id = super::hash_file(&packed).unwrap().id.unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("3ds", Some("CTR-P-AREE")));
    }
}
