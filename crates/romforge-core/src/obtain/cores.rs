//! RetroArch cores from the libretro buildbot
//! (`buildbot.libretro.com/nightly/linux/<arch>/latest/<core>_libretro.so.zip`).
//! Its `.index-extended` lists each core's date and the CRC32 of the core
//! inside its zip (a download is checked against it, and a newer date is
//! an update); libretro's core
//! info (`assets/frontend/info.zip`) gives each core's name and licence.
//! Both are kept in the downloaded databases' folder and refreshed when
//! asked. Cores romforge downloaded are recorded in
//! `Emulators/cores/romforge-cores.json`, so only those can be updated or
//! removed from romforge.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::home::Home;
use crate::launch::Env;
use crate::{Error, systems};

const BUILDBOT: &str = "https://buildbot.libretro.com";
const INFO_FILE: &str = "libretro-core-info.zip";
const MANIFEST: &str = "romforge-cores.json";

/// The buildbot's folder for this computer.
pub fn arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("x86_64"),
        "aarch64" => Some("aarch64"),
        _ => None,
    }
}

fn index_file(arch: &str) -> String {
    format!("libretro-cores-{arch}.index")
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CoreRow {
    pub name: String,
    pub display_name: String,
    pub license: String,
    /// Its licence forbids commercial use.
    pub non_commercial: bool,
    pub experimental: bool,
    /// The platforms romforge uses it for (names).
    pub systems: Vec<&'static str>,
    /// Where RetroArch's copy is found, if installed.
    pub installed: Option<PathBuf>,
    /// The buildbot date of the copy romforge downloaded.
    pub ours: Option<String>,
    /// The buildbot's latest date for it.
    pub available: Option<String>,
    /// romforge's copy is older than the buildbot's.
    pub update: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CoreList {
    /// The buildbot's list has been fetched.
    pub checked: bool,
    pub arch: Option<&'static str>,
    pub cores: Vec<CoreRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Ours {
    date: String,
    crc: String,
}

/// Fetch the buildbot's list and the core info into `db_dir`.
pub fn refresh_cores(db_dir: &Path) -> Result<usize, Error> {
    let arch = arch().ok_or_else(|| Error::Launch("the buildbot has no cores for this processor".into()))?;
    let index = crate::fetch::get(&format!("{BUILDBOT}/nightly/linux/{arch}/latest/.index-extended"))?;
    let info = crate::fetch::get(&format!("{BUILDBOT}/assets/frontend/info.zip"))?;
    zip::ZipArchive::new(std::io::Cursor::new(&info))?;
    std::fs::create_dir_all(db_dir)?;
    crate::atomic::write(&db_dir.join(index_file(arch)), &index)?;
    crate::atomic::write(&db_dir.join(INFO_FILE), &info)?;
    Ok(parse_index(&String::from_utf8_lossy(&index)).len())
}

/// `core → (date, crc)` from `.index-extended` lines `date crc file`.
fn parse_index(text: &str) -> BTreeMap<String, (String, String)> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (date, crc, file) = (it.next()?, it.next()?, it.next()?);
            let core = file.strip_suffix("_libretro.so.zip")?;
            Some((core.to_owned(), (date.to_owned(), crc.to_ascii_lowercase())))
        })
        .collect()
}

/// `key = "value"` lines of a core's `.info`.
fn info_of(zip: Option<&mut zip::ZipArchive<std::fs::File>>, core: &str) -> BTreeMap<String, String> {
    let mut text = String::new();
    if let Some(z) = zip
        && let Ok(mut f) = z.by_name(&format!("{core}_libretro.info"))
    {
        let _ = f.read_to_string(&mut text);
    }
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_owned(), v.trim().trim_matches('"').to_owned()))
        })
        .collect()
}

fn manifest(home: &Home) -> BTreeMap<String, Ours> {
    std::fs::read(cores_dir(home).join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_manifest(home: &Home, m: &BTreeMap<String, Ours>) -> Result<(), Error> {
    let json = serde_json::to_vec_pretty(m).map_err(|e| Error::Launch(e.to_string()))?;
    crate::atomic::write(&cores_dir(home).join(MANIFEST), &json)?;
    Ok(())
}

pub fn cores_dir(home: &Home) -> PathBuf {
    home.emulators().join("cores")
}

/// The cores romforge's platforms use, a core name each.
fn used() -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for s in systems::SYSTEMS {
        for c in s.cores {
            out.entry(c).or_default().push(s.name);
        }
    }
    out
}

/// Every core romforge's platforms use: installed where, licence, and what
/// the buildbot has (from the list last fetched).
pub fn cores(db_dir: &Path, home: &Home, env: &Env) -> CoreList {
    let arch = arch();
    let index = arch
        .and_then(|a| std::fs::read_to_string(db_dir.join(index_file(a))).ok())
        .map(|t| parse_index(&t));
    let mut info = std::fs::File::open(db_dir.join(INFO_FILE))
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok());
    let ours = manifest(home);
    let mut rows = Vec::new();
    for (core, systems) in used() {
        let i = info_of(info.as_mut(), core);
        let license = i.get("license").cloned().unwrap_or_default();
        let available = index.as_ref().and_then(|x| x.get(core)).map(|(d, _)| d.clone());
        let mine = ours.get(core).map(|o| o.date.clone());
        rows.push(CoreRow {
            name: core.to_owned(),
            display_name: i.get("display_name").cloned().unwrap_or_else(|| core.to_owned()),
            non_commercial: license.to_ascii_lowercase().contains("non-commercial"),
            license,
            experimental: i.get("is_experimental").is_some_and(|v| v == "true"),
            systems,
            installed: env.core_path(core),
            update: matches!((&mine, &available), (Some(m), Some(a)) if a > m),
            ours: mine,
            available,
        });
    }
    CoreList {
        checked: index.is_some(),
        arch,
        cores: rows,
    }
}

/// Download `core` from the buildbot into `Emulators/cores/` (replacing
/// only romforge's own older copy). Returns its buildbot date.
pub fn download_core(db_dir: &Path, home: &Home, core: &str) -> Result<String, Error> {
    if !used().contains_key(core) {
        return Err(Error::Launch(format!(
            "{core:?} isn't a core romforge's platforms use"
        )));
    }
    let arch = arch().ok_or_else(|| Error::Launch("the buildbot has no cores for this processor".into()))?;
    let index = std::fs::read_to_string(db_dir.join(index_file(arch)))
        .map_err(|_| Error::Launch("fetch the buildbot's list first (`emulator cores --refresh`)".into()))?;
    let (date, crc) = parse_index(&index)
        .remove(core)
        .ok_or_else(|| Error::Launch(format!("the buildbot has no {core} for {arch}")))?;
    let file = format!("{core}_libretro.so");
    let dest = cores_dir(home).join(&file);
    let mut ours = manifest(home);
    if dest.exists() && !ours.contains_key(core) {
        return Err(Error::Launch(format!(
            "{} is there and not romforge's: left as it is",
            dest.display()
        )));
    }
    let bytes = crate::fetch::get(&format!("{BUILDBOT}/nightly/linux/{arch}/latest/{file}.zip"))?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut so = Vec::new();
    zip.by_name(&file)?.read_to_end(&mut so)?;
    // The list's CRC32 is the core's own (inside the zip), as checked
    // against the buildbot 2026-10-19.
    let got = format!("{:08x}", crc32fast::hash(&so));
    if got != crc {
        return Err(Error::Launch(format!(
            "{file} didn't download whole (CRC32 {got}, the list says {crc})"
        )));
    }
    crate::atomic::write(&dest, &so)?;
    ours.insert(
        core.to_owned(),
        Ours {
            date: date.clone(),
            crc,
        },
    );
    save_manifest(home, &ours)?;
    Ok(date)
}

/// Remove a core romforge downloaded.
pub fn remove_core(home: &Home, core: &str) -> Result<(), Error> {
    let mut ours = manifest(home);
    if ours.remove(core).is_none() {
        return Err(Error::Launch(format!("{core} wasn't downloaded by romforge")));
    }
    match std::fs::remove_file(cores_dir(home).join(format!("{core}_libretro.so"))) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    save_manifest(home, &ours)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_comes_from_what_was_fetched() {
        let t = tempfile::tempdir().unwrap();
        let home = Home::create(t.path()).unwrap();
        let db = t.path().join("db");
        std::fs::create_dir_all(&db).unwrap();
        let Some(arch) = arch() else { return };
        std::fs::write(
            db.join(index_file(arch)),
            "2026-10-19 aaaaaaaa mgba_libretro.so.zip\n2026-10-19 bbbbbbbb genesis_plus_gx_libretro.so.zip\n",
        )
        .unwrap();
        let mut w = zip::ZipWriter::new(std::fs::File::create(db.join(INFO_FILE)).unwrap());
        for (core, info) in [
            (
                "mgba",
                "display_name = \"Nintendo - Game Boy Advance (mGBA)\"\nlicense = \"MPLv2.0\"\n",
            ),
            (
                "genesis_plus_gx",
                "display_name = \"GPGX\"\nlicense = \"Non-commercial\"\n",
            ),
        ] {
            w.start_file(
                format!("{core}_libretro.info"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            std::io::Write::write_all(&mut w, info.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        // romforge downloaded an older mGBA.
        let mut ours = BTreeMap::new();
        ours.insert(
            "mgba".to_owned(),
            Ours {
                date: "2026-09-01".into(),
                crc: "x".into(),
            },
        );
        std::fs::create_dir_all(cores_dir(&home)).unwrap();
        save_manifest(&home, &ours).unwrap();
        std::fs::write(cores_dir(&home).join("mgba_libretro.so"), b"so").unwrap();

        let env = Env::default().with_emulators_dir(&home.emulators());
        let list = cores(&db, &home, &env);
        assert!(list.checked);
        let row = |n: &str| list.cores.iter().find(|c| c.name == n).unwrap();
        let mgba = row("mgba");
        assert_eq!(mgba.display_name, "Nintendo - Game Boy Advance (mGBA)");
        assert!(
            mgba.update && mgba.installed.is_some() && mgba.systems.contains(&"Nintendo Game Boy Advance")
        );
        let gpgx = row("genesis_plus_gx");
        assert!(gpgx.non_commercial && gpgx.installed.is_none() && gpgx.available.is_some());
        assert!(row("snes9x").available.is_none(), "not in this list");

        // Only romforge's own copies are removed; a stranger is refused.
        assert!(remove_core(&home, "snes9x").is_err());
        assert!(download_core(&db, &home, "../evil").is_err());
        remove_core(&home, "mgba").unwrap();
        assert!(!cores_dir(&home).join("mgba_libretro.so").exists());
    }

    #[test]
    fn the_index_lists_cores_by_name() {
        let ix = parse_index("2026-10-19 521E6850 mgba_libretro.so.zip\n2026-10-18 3a224a58 notes.txt\n");
        assert_eq!(ix.len(), 1);
        assert_eq!(ix["mgba"], ("2026-10-19".to_owned(), "521e6850".to_owned()));
    }
}
