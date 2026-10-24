use super::*;

/// One downloadable DAT (or, for TOSEC, group of DATs) for a system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum Source {
    /// A file in libretro-database's No-Intro mirror, without `.dat`.
    NoIntro(&'static str),
    /// A Redump DAT code, e.g. `psx` or `psx-bios`.
    Redump(&'static str),
    /// A TOSEC system name; every `<name> - ...` DAT in the pack.
    Tosec(&'static str),
    /// A file in libretro-database's `hacks` folder.
    Hacks(&'static str),
    /// A file in libretro-database's `homebrew` folder.
    Homebrew(&'static str),
    /// One of libretro-database's own DATs (`dat/<name>.dat`), e.g. DOS.
    Libretro(&'static str),
    /// A DAT at a fixed URL (the arcade DATs).
    Url(&'static str),
    /// A file in libretro-database's `serial` folder: product codes, not a
    /// DAT ([`crate::Library::download_dat`] stores them in `serials`).
    Serials(&'static str),
    /// One of libretro-database's own DATs of names and product codes only
    /// (`dat/<name>.dat`, `System::serial_dats`): stored in `serials` too.
    SerialDat(&'static str),
    /// Retool's clone list for a DAT name and its group ("No-Intro" or
    /// "Redump"); stored in `clone_groups`.
    CloneList(&'static str, &'static str),
}

pub const CLONELISTS_BASE: &str =
    "https://raw.githubusercontent.com/unexpectedpanda/retool-clonelists-metadata/main/clonelists";

impl Source {
    /// The DAT's URL; `None` for TOSEC, whose pack URL changes per release
    /// ([`tosec_pack_url`]).
    pub fn url(&self) -> Option<String> {
        let libretro = |dir: &str, name: &str| format!("{LIBRETRO_BASE}/{dir}/{}.dat", percent_encode(name));
        match *self {
            Source::NoIntro(name) => Some(libretro("no-intro", name)),
            Source::Redump(code) => Some(format!("{REDUMP_BASE}/datfile/{code}/serial,version")),
            Source::Tosec(_) => None,
            Source::Hacks(name) => Some(libretro("hacks", name)),
            Source::Homebrew(name) => Some(libretro("homebrew", name)),
            Source::Libretro(name) => Some(format!("{LIBRETRO_DAT_BASE}/{}.dat", percent_encode(name))),
            Source::Url(url) => Some(url.to_owned()),
            Source::Serials(name) => Some(libretro("serial", name)),
            Source::SerialDat(name) => Some(format!("{LIBRETRO_DAT_BASE}/{}.dat", percent_encode(name))),
            Source::CloneList(name, group) => Some(format!(
                "{CLONELISTS_BASE}/{}",
                percent_encode(&format!("{name} ({group}).json"))
            )),
        }
    }

    /// A list stored beside the DATs (serials, clone groups), not a DAT.
    pub fn is_list(&self) -> bool {
        matches!(
            self,
            Source::Serials(_) | Source::SerialDat(_) | Source::CloneList(..)
        )
    }

    /// Whether `source_path` (as stored for an imported DAT) came from the
    /// No-Intro mirror.
    pub fn is_mirror_url(source_path: &str) -> bool {
        source_path.starts_with(LIBRETRO_NOINTRO_BASE)
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::NoIntro(n) => write!(f, "No-Intro (libretro) {n}"),
            Source::Redump(c) => write!(f, "Redump {c}"),
            Source::Tosec(n) => write!(f, "TOSEC {n}"),
            Source::Hacks(n) => write!(f, "hacks (libretro) {n}"),
            Source::Homebrew(n) => write!(f, "homebrew (libretro) {n}"),
            Source::Libretro(n) => write!(f, "libretro {n}"),
            Source::Url(u) => write!(
                f,
                "{}",
                u.rsplit('/')
                    .next()
                    .unwrap_or(u)
                    .replace("%20", " ")
                    .replace("%2C", ",")
            ),
            Source::Serials(n) => write!(f, "serials (libretro) {n}"),
            Source::SerialDat(n) => write!(f, "serials (libretro) {n} (GameTDB)"),
            Source::CloneList(n, g) => write!(f, "clone list (Retool) {n} ({g})"),
        }
    }
}

/// Every DAT `dat download` fetches for `system`: No-Intro, Redump games
/// and BIOS, TOSEC, libretro's own DATs, its serial list for No-Intro
/// systems, and with
/// `extras` the hacks and homebrew DATs.
pub fn sources(system: &System, extras: bool) -> Vec<Source> {
    let serials = (!system.nointro.is_empty())
        .then(|| crate::enrich::libretro_name(system))
        .flatten();
    // Serials right after the No-Intro DATs, whose games they are matched
    // to by CRC.
    let mut out: Vec<Source> = (system.nointro.iter().copied().map(Source::NoIntro))
        .chain(serials.map(Source::Serials))
        .chain(system.serial_dats.iter().copied().map(Source::SerialDat))
        .chain(system.redump.iter().copied().map(Source::Redump))
        .chain(system.tosec.iter().copied().map(Source::Tosec))
        .chain(system.dats.iter().copied().map(Source::Libretro))
        .chain(system.urls.iter().copied().map(Source::Url))
        .collect();
    // Clone lists for the system's No-Intro and Redump DATs, by DAT name
    // (PSP has both; a list that doesn't exist is skipped).
    for (has, g) in [
        (!system.nointro.is_empty(), "No-Intro"),
        (!system.redump.is_empty(), "Redump"),
    ] {
        if has {
            out.extend(system.dat_names.iter().map(|n| Source::CloneList(n, g)));
        }
    }
    if extras {
        out.extend(system.hacks.iter().copied().map(Source::Hacks));
        out.extend(system.homebrew.iter().copied().map(Source::Homebrew));
    }
    out
}

/// Which earlier import a downloaded or imported DAT of the same name
/// replaces: one from the same URL, from any TOSEC release, or (for files)
/// any file.
pub(crate) fn source_family(source_path: &str) -> &str {
    let redump = format!("{REDUMP_BASE}/datfile/");
    if source_path.starts_with(TOSEC_BASE) {
        "tosec"
    } else if let Some(rest) = source_path.strip_prefix(redump.as_str()) {
        // With or without the fields asked for (`serial,version`).
        let code = rest.find('/').map_or(rest.len(), |i| i + 1);
        &source_path[..redump.len() + code]
    } else if source_path.starts_with("http://") || source_path.starts_with("https://") {
        source_path
    } else {
        "file"
    }
}

pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The URL of the newest TOSEC DAT pack, read from tosecdev.org's download
/// pages (each release is a `/downloads/category/<n>-<yyyy-mm-dd>` page with
/// a "TOSEC - DAT Pack - Complete" link). Looked up once per run.
pub fn tosec_pack_url() -> Result<String, Error> {
    static URL: Mutex<Option<String>> = Mutex::new(None);
    let mut cached = URL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(url) = &*cached {
        return Ok(url.clone());
    }
    let index = String::from_utf8_lossy(&get(&format!("{TOSEC_BASE}/downloads"))?).into_owned();
    let release = latest_tosec_release(&index)
        .ok_or_else(|| Error::Http("tosecdev.org: no release found on the downloads page".into()))?;
    let page = String::from_utf8_lossy(&get(&format!("{TOSEC_BASE}{release}"))?).into_owned();
    let url = tosec_pack_link(&page)
        .map(|href| format!("{TOSEC_BASE}{href}"))
        .ok_or_else(|| Error::Http(format!("tosecdev.org{release}: no DAT pack link")))?;
    *cached = Some(url.clone());
    Ok(url)
}

/// The `/downloads/category/<n>-<date>` link with the latest date.
pub(super) fn latest_tosec_release(html: &str) -> Option<String> {
    const PREFIX: &str = "href=\"/downloads/category/";
    html.match_indices(PREFIX)
        .filter_map(|(i, _)| {
            let rest = &html[i + PREFIX.len()..];
            let slug = &rest[..rest.find('"')?];
            let (_, date) = slug.split_once('-')?;
            let is_date = date.len() == 10
                && date.bytes().enumerate().all(|(i, b)| match i {
                    4 | 7 => b == b'-',
                    _ => b.is_ascii_digit(),
                });
            is_date.then(|| (date.to_owned(), format!("/downloads/category/{slug}")))
        })
        .max()
        .map(|(_, path)| path)
}

/// The "DAT Pack - Complete" download link on a release page.
pub(super) fn tosec_pack_link(html: &str) -> Option<String> {
    html.match_indices("href=\"")
        .filter_map(|(i, m)| {
            let rest = &html[i + m.len()..];
            Some(&rest[..rest.find('"')?])
        })
        .find(|href| href.contains("?download=") && href.contains("tosec-dat-pack-complete"))
        .map(|href| href.replace("&amp;", "&"))
}

static PACK: Mutex<Option<(String, Arc<Vec<u8>>)>> = Mutex::new(None);

/// The TOSEC pack at `url`, downloaded once and shared until
/// [`forget_tosec_pack`] (the end of a `Library::batch`).
pub fn tosec_pack(url: &str) -> Result<Arc<Vec<u8>>, Error> {
    let mut cached = PACK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((u, bytes)) = &*cached
        && u == url
    {
        return Ok(bytes.clone());
    }
    let bytes = Arc::new(get(url)?);
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(Error::Http(format!("{url}: not a zip")));
    }
    *cached = Some((url.to_owned(), bytes.clone()));
    Ok(bytes)
}

/// Drop the cached TOSEC pack (about 100 MB).
pub fn forget_tosec_pack() {
    *PACK.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The DATs in a TOSEC pack for system `name`: `TOSEC/<name> - *.dat`,
/// sorted. Only the main `TOSEC/` folder; `TOSEC-ISO`, `TOSEC-PIX` and the
/// cue sheets are left out.
pub fn tosec_dats(pack: &[u8], name: &str) -> Result<Vec<DatBytes>, Error> {
    let prefix = format!("TOSEC/{name} - ");
    let mut zip = zip::ZipArchive::new(Cursor::new(pack))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut member = zip.by_index(i)?;
        let member_name = member.name()?.into_owned();
        if member.is_file() && member_name.starts_with(&prefix) && member_name.ends_with(".dat") {
            let buf = read_member(&mut member)?;
            out.push(DatBytes {
                member: Some(member_name),
                bytes: buf,
            });
        }
    }
    out.sort_by(|a, b| a.member.cmp(&b.member));
    Ok(out)
}
