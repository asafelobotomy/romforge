use std::io::Read;
use std::path::Path;

use crate::Error;
use crate::cleanup::{self};
use crate::patch::{self, Kind};

/// A patch read from disk: the patch itself, or the one patch in a zip
/// (with the zip's readme, kept with the result).
#[derive(Debug, Clone)]
pub struct PatchFile {
    pub name: String,
    pub kind: Kind,
    pub data: Vec<u8>,
    pub sha1: String,
    pub readme: Option<(String, Vec<u8>)>,
}

fn is_readme(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".txt", ".nfo", ".diz", ".md"].iter().any(|e| n.ends_with(e))
}

/// Read a patch file, or a zip holding exactly one.
pub fn read_patch(path: &Path) -> Result<PatchFile, Error> {
    let bad = |m: String| Error::Dat(format!("patch: {m}"));
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (name, data, readme) = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let mut z = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(path)?))?;
        let names: Vec<String> = (0..z.len())
            .filter_map(|i| {
                z.by_index_raw(i)
                    .ok()
                    .and_then(|m| m.name().ok().map(|n| n.into_owned()))
            })
            .filter(|n| !n.ends_with('/'))
            .collect();
        let patches: Vec<&String> = names.iter().filter(|n| patch::is_patch_name(n)).collect();
        let one = match patches.as_slice() {
            [one] => (*one).clone(),
            [] => return Err(bad(format!("{file_name} holds no patch romforge reads"))),
            many => {
                return Err(bad(format!(
                    "{file_name} holds {} patches ({}); unzip the one you want and pick it",
                    many.len(),
                    many.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )));
            }
        };
        let mut data = Vec::new();
        z.by_name(&one)?.take(512 << 20).read_to_end(&mut data)?;
        let readme = names
            .iter()
            .filter(|n| is_readme(n))
            .min_by_key(|n| !n.to_ascii_lowercase().contains("readme"))
            .and_then(|n| {
                let mut d = Vec::new();
                z.by_name(n).ok()?.take(1 << 20).read_to_end(&mut d).ok()?;
                Some((n.rsplit('/').next().unwrap_or(n).to_owned(), d))
            });
        (one.rsplit('/').next().unwrap_or(&one).to_owned(), data, readme)
    } else {
        (file_name, std::fs::read(path)?, None)
    };
    let kind = patch::info(&data)
        .ok_or_else(|| {
            bad(format!(
                "{name} isn't a patch romforge reads (IPS, BPS, UPS, PPF, xdelta)"
            ))
        })?
        .kind;
    let sha1 = cleanup::sha1_of(&data[..])?;
    Ok(PatchFile {
        name,
        kind,
        data,
        sha1,
        readme,
    })
}
