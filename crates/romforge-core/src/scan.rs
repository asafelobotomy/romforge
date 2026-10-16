//! Walks a folder and hashes every file and every zip/7z member as a stream.
//! Nothing is extracted to disk and no external tool is run.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::hash::{Hashes, hash_stream};

/// Extensions that are never ROMs: DATs, notes, our own database.
const IGNORED_EXTENSIONS: &[&str] = &[
    "dat", "xml", "txt", "nfo", "md", "db", "db-wal", "db-shm", "sqlite",
];

#[derive(Debug)]
pub struct Entry {
    /// The file on disk (the archive, for members).
    pub path: PathBuf,
    /// Path inside the archive, or empty for plain files.
    pub member: String,
    pub hashes: Result<Hashes, String>,
}

/// Call `each` for every hashed file or member under `root`, in path order.
pub fn walk(root: &Path, skip: &[PathBuf], mut each: impl FnMut(Entry)) -> Result<(), Error> {
    let walker = walkdir::WalkDir::new(root).follow_links(true).sort_by_file_name();
    for item in walker {
        let item = match item {
            Ok(item) => item,
            Err(e) => {
                each(Entry {
                    path: e.path().map(Path::to_path_buf).unwrap_or_default(),
                    member: String::new(),
                    hashes: Err(e.to_string()),
                });
                continue;
            }
        };
        if !item.file_type().is_file() || skip.iter().any(|s| s == item.path()) {
            continue;
        }
        let path = item.path().to_path_buf();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if IGNORED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let result = match ext.as_str() {
            "zip" => walk_zip(&path, &mut each),
            "7z" => walk_7z(&path, &mut each),
            _ => {
                let hashes = hash_file(&path).map_err(|e| e.to_string());
                each(Entry {
                    path: path.clone(),
                    member: String::new(),
                    hashes,
                });
                Ok(())
            }
        };
        if let Err(e) = result {
            each(Entry {
                path,
                member: String::new(),
                hashes: Err(e.to_string()),
            });
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> std::io::Result<Hashes> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    hash_stream(BufReader::new(file), size)
}

fn walk_zip(path: &Path, each: &mut impl FnMut(Entry)) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    for i in 0..archive.len() {
        let mut member = archive.by_index(i)?;
        if member.is_dir() {
            continue;
        }
        let name = member.name()?.into_owned();
        let size = member.size();
        let hashes = hash_stream(&mut member, size).map_err(|e| e.to_string());
        each(Entry {
            path: path.to_path_buf(),
            member: name,
            hashes,
        });
    }
    Ok(())
}

fn walk_7z(path: &Path, each: &mut impl FnMut(Entry)) -> Result<(), Error> {
    let mut archive = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
    archive.for_each_entries(|entry, reader| {
        if !entry.is_directory() {
            let hashes = hash_stream(reader, entry.size()).map_err(|e| e.to_string());
            each(Entry {
                path: path.to_path_buf(),
                member: entry.name().to_owned(),
                hashes,
            });
        }
        Ok(true)
    })?;
    Ok(())
}
