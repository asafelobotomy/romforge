//! RetroBIOS's emulator profiles (`api/v1/emulators.json`, MIT): for each
//! emulator or core, the files it loads. Read leniently: a hash may be one
//! value, a list, or a comma-separated list, and most fields are optional.

use serde_json::Value;

use crate::Error;

/// One emulator or core as RetroBIOS profiles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub display_name: String,
    /// libretro core names it covers.
    pub cores: Vec<String>,
    /// RetroBIOS system ids.
    pub systems: Vec<String>,
    pub files: Vec<FileReq>,
}

/// A file an emulator loads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReq {
    pub name: String,
    /// Where it goes, relative to the emulator's BIOS folder (else `name`).
    pub path: Option<String>,
    pub required: bool,
    /// The emulator runs without it (high-level emulation).
    pub hle: bool,
    pub size: Option<u64>,
    /// Accepted hashes, lower case.
    pub sha1: Vec<String>,
    pub md5: Vec<String>,
    /// The RetroBIOS system it is for, when the profile covers several.
    pub system: Option<String>,
    /// An archive of several files (MAME-style BIOS zips).
    pub archive: bool,
    /// The regions it is loaded for (a regional BIOS is needed only for
    /// games of its regions); empty: all.
    pub regions: Vec<String>,
    /// Files of one group are alternatives: one of them is enough.
    pub variant_group: Option<String>,
}

impl FileReq {
    /// Where it goes under the emulator's BIOS folder.
    pub fn place(&self) -> &str {
        self.path
            .as_deref()
            .filter(|p| !p.is_empty())
            .unwrap_or(&self.name)
    }
}

/// The profiles in `bytes`, aliases resolved to what they point to
/// (`generated_at` too).
pub fn parse(bytes: &[u8]) -> Result<(Vec<Profile>, String), Error> {
    let bad = || Error::Launch("RetroBIOS's emulator list has an unexpected shape".into());
    let doc: Value = serde_json::from_slice(bytes).map_err(|_| bad())?;
    let items = doc.get("items").and_then(Value::as_array).ok_or_else(bad)?;
    let generated = doc
        .get("generated_at")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut out = Vec::new();
    let mut aliases = Vec::new();
    for item in items {
        let (Some(id), Some(p)) = (item.get("id").and_then(Value::as_str), item.get("profile")) else {
            continue;
        };
        if let Some(of) = p.get("alias_of").and_then(Value::as_str) {
            aliases.push((id.to_owned(), of.to_owned(), strings(p.get("cores"))));
            continue;
        }
        out.push(Profile {
            id: id.to_owned(),
            display_name: p
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned(),
            cores: strings(p.get("cores")),
            systems: strings(p.get("systems")),
            files: p
                .get("files")
                .and_then(Value::as_array)
                .map(|fs| fs.iter().filter_map(file).collect())
                .unwrap_or_default(),
        });
    }
    // An alias's cores are its target's.
    for (id, of, cores) in aliases {
        if let Some(t) = out.iter_mut().find(|p| p.id == of) {
            t.cores.push(id);
            t.cores.extend(cores);
        }
    }
    Ok((out, generated))
}

fn file(v: &Value) -> Option<FileReq> {
    let name = v.get("name").and_then(Value::as_str)?.to_owned();
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    Some(FileReq {
        name,
        path: v.get("path").and_then(Value::as_str).map(str::to_owned),
        required: flag("required"),
        hle: flag("hle_fallback"),
        size: v.get("size").and_then(Value::as_u64),
        sha1: hashes(v.get("sha1"), 40),
        md5: hashes(v.get("md5"), 32),
        system: v.get("system").and_then(Value::as_str).map(str::to_owned),
        archive: v
            .get("contents")
            .and_then(Value::as_array)
            .is_some_and(|c| !c.is_empty()),
        regions: strings(v.get("region")),
        variant_group: v.get("variant_group").and_then(Value::as_str).map(str::to_owned),
    })
}

/// Strings in a value that is a string or a list of them.
fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

/// Hex hashes of `len` digits in a value: one, a list, or comma-separated.
fn hashes(v: Option<&Value>, len: usize) -> Vec<String> {
    strings(v)
        .iter()
        .flat_map(|s| s.split(','))
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| h.len() == len && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .collect()
}
