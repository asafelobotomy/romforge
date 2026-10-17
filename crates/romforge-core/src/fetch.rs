//! Downloading DATs.
//!
//! Where each system's DATs come from is in [`crate::systems`]:
//! - `nointro`: No-Intro's data may be reused freely, but DAT-o-MATIC hands
//!   DATs out only through its download form, so romforge takes them from
//!   libretro-database's public mirror on GitHub instead. For DATs downloaded
//!   from DAT-o-MATIC by hand, the public `profile.xml` says which are out of
//!   date.
//! - `redump`: Redump serves DATs at `http://redump.org/datfile/<code>/`
//!   (game DATs as zips, BIOS DATs as plain clrmamepro text).
//! - `tosec`: TOSEC publishes one zip of all its DATs per release (about
//!   100 MB). The latest release is found on tosecdev.org's download page;
//!   the pack is fetched at most once per batch and not at all when its
//!   DATs for a system are already imported.
//! - `hacks`, `homebrew`: libretro-database's own small DATs, fetched only
//!   when asked for ([`sources`] with `extras`).
//!
//! Requests to one host are spaced out ([`delay_for`]; DAT-o-MATIC's
//! robots.txt asks for 5 s). Downloads stay in memory and are never written
//! to disk.

use std::collections::HashMap;
use std::io::{BufRead, Cursor, Read};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use ureq::tls::{RootCerts, TlsConfig};

use crate::Error;
use crate::systems::System;

pub const USER_AGENT: &str = concat!("romforge/", env!("CARGO_PKG_VERSION"));
pub const REDUMP_BASE: &str = "http://redump.org";
pub const LIBRETRO_BASE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat";
pub const LIBRETRO_NOINTRO_BASE: &str =
    "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro";
pub const TOSEC_BASE: &str = "https://www.tosecdev.org";
pub const NOINTRO_PROFILE_URL: &str = "https://datomatic.no-intro.org/profile.xml";
pub const NOINTRO_DOWNLOAD_PAGE: &str = "https://datomatic.no-intro.org/index.php?page=download";
/// No DAT pack is near this; it only stops a runaway response.
const MAX_BODY: u64 = 512 << 20;

/// Minimum time between two requests to `host`.
pub fn delay_for(host: &str) -> Duration {
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    match name {
        "datomatic.no-intro.org" => Duration::from_secs(5),
        "127.0.0.1" | "localhost" | "[::1]" => Duration::ZERO,
        _ => Duration::from_secs(1),
    }
}

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
}

impl Source {
    /// The DAT's URL; `None` for TOSEC, whose pack URL changes per release
    /// ([`tosec_pack_url`]).
    pub fn url(&self) -> Option<String> {
        let libretro = |dir: &str, name: &str| format!("{LIBRETRO_BASE}/{dir}/{}.dat", percent_encode(name));
        match *self {
            Source::NoIntro(name) => Some(libretro("no-intro", name)),
            Source::Redump(code) => Some(format!("{REDUMP_BASE}/datfile/{code}/")),
            Source::Tosec(_) => None,
            Source::Hacks(name) => Some(libretro("hacks", name)),
            Source::Homebrew(name) => Some(libretro("homebrew", name)),
        }
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
        }
    }
}

/// Every DAT `dat download` fetches for `system`: No-Intro, Redump games
/// and BIOS, TOSEC, and with `extras` the hacks and homebrew DATs.
pub fn sources(system: &System, extras: bool) -> Vec<Source> {
    let mut out: Vec<Source> = (system.nointro.iter().copied().map(Source::NoIntro))
        .chain(system.redump.iter().copied().map(Source::Redump))
        .chain(system.tosec.iter().copied().map(Source::Tosec))
        .collect();
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
    if source_path.starts_with(TOSEC_BASE) {
        "tosec"
    } else if source_path.starts_with("http://") || source_path.starts_with("https://") {
        source_path
    } else {
        "file"
    }
}

fn percent_encode(s: &str) -> String {
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
fn latest_tosec_release(html: &str) -> Option<String> {
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
fn tosec_pack_link(html: &str) -> Option<String> {
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
            let mut buf = Vec::new();
            member.read_to_end(&mut buf)?;
            out.push(DatBytes {
                member: Some(member_name),
                bytes: buf,
            });
        }
    }
    out.sort_by(|a, b| a.member.cmp(&b.member));
    Ok(out)
}

/// [`get`], but a 404 is `None` rather than an error.
pub fn get_optional(url: &str) -> Result<Option<Vec<u8>>, Error> {
    match fetch(url) {
        Ok(body) => Ok(Some(body)),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(Error::Http(format!("{url}: {e}"))),
    }
}

/// GET `url` into memory, waiting first if the same host was asked less than
/// [`delay_for`] ago.
pub fn get(url: &str) -> Result<Vec<u8>, Error> {
    fetch(url).map_err(|e| Error::Http(format!("{url}: {e}")))
}

fn fetch(url: &str) -> Result<Vec<u8>, ureq::Error> {
    wait_for_host(url);
    let agent = ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_global(Some(Duration::from_secs(900)))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();
    let mut response = agent.get(url).call()?;
    response.body_mut().with_config().limit(MAX_BODY).read_to_vec()
}

/// Percent-encode one URL path segment.
pub fn encode_segment(s: &str) -> String {
    percent_encode(s)
}

fn wait_for_host(url: &str) {
    static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
    let host = url.split('/').nth(2).unwrap_or(url).to_owned();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let last = last.get_or_insert_with(HashMap::new);
    if let Some(t) = last.get(&host) {
        let (since, delay) = (t.elapsed(), delay_for(&host));
        if since < delay {
            std::thread::sleep(delay - since);
        }
    }
    last.insert(host, Instant::now());
}

/// One DAT read from a file or zip: the zip member's name, if any, and its
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatBytes {
    pub member: Option<String>,
    pub bytes: Vec<u8>,
}

/// The DATs inside `bytes`: the bytes themselves for a plain DAT, or every
/// `.dat`/`.xml` member (sorted by name) for a zip. Read from memory, never
/// extracted.
pub fn dats_in(bytes: &[u8]) -> Result<Vec<DatBytes>, Error> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return Ok(vec![DatBytes {
            member: None,
            bytes: bytes.to_vec(),
        }]);
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut member = zip.by_index(i)?;
        let name = member.name()?.into_owned();
        let lower = name.to_ascii_lowercase();
        if member.is_file() && (lower.ends_with(".dat") || lower.ends_with(".xml")) {
            let mut buf = Vec::new();
            member.read_to_end(&mut buf)?;
            out.push(DatBytes {
                member: Some(name),
                bytes: buf,
            });
        }
    }
    if out.is_empty() {
        return Err(Error::Dat("zip has no .dat or .xml file".into()));
    }
    out.sort_by(|a, b| a.member.cmp(&b.member));
    Ok(out)
}

/// One entry of No-Intro's `profile.xml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ProfileEntry {
    pub name: String,
    pub version: String,
}

/// Fetch and parse No-Intro's `profile.xml`.
pub fn nointro_profile() -> Result<Vec<ProfileEntry>, Error> {
    parse_profile(&get(NOINTRO_PROFILE_URL)?[..])
}

/// Parse a clrmamepro `profile.xml` (`<clrmamepro><datfile><name>…`).
pub fn parse_profile(input: impl BufRead) -> Result<Vec<ProfileEntry>, Error> {
    let mut reader = quick_xml::Reader::from_reader(input);
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut buf = Vec::new();
    let err = |e: quick_xml::Error| Error::Dat(format!("profile.xml: {e}"));
    loop {
        match reader.read_event_into(&mut buf).map_err(err)? {
            Event::Start(e) => {
                let tag = e.local_name().as_ref().to_owned();
                if path.len() == 1 && tag == "datfile" {
                    out.push(ProfileEntry::default());
                }
                path.push(tag);
                text.clear();
            }
            Event::Text(t) => text.push_str(&t.xml_content(XmlVersion::Implicit1_0)),
            Event::GeneralRef(r) => {
                if let Some(s) = quick_xml::escape::resolve_predefined_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::End(_) => {
                let tag = path.pop().unwrap_or_default();
                if path.len() == 2
                    && path[1] == "datfile"
                    && let Some(entry) = out.last_mut()
                {
                    let value = text.trim().to_owned();
                    match tag.as_str() {
                        "name" => entry.name = value,
                        "version" => entry.version = value,
                        _ => {}
                    }
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if out.is_empty() {
        return Err(Error::Dat("profile.xml lists no DATs".into()));
    }
    out.retain(|e| !e.name.is_empty());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems;

    #[test]
    fn sources_come_from_the_systems_table() {
        let psx = systems::by_key("PlayStation").unwrap();
        let urls: Vec<_> = sources(psx, false).iter().map(|s| s.url().unwrap()).collect();
        assert_eq!(
            urls,
            [
                "http://redump.org/datfile/psx/",
                "http://redump.org/datfile/psx-bios/"
            ]
        );
        assert_eq!(sources(psx, true).len(), 3, "plus the PlayStation hacks DAT");
        let gba = sources(systems::by_key("Game Boy Advance").unwrap(), false);
        assert_eq!(
            gba[0].url().unwrap(),
            "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro/Nintendo%20-%20Game%20Boy%20Advance.dat"
        );
        let c64 = sources(systems::by_key("Commodore 64").unwrap(), false);
        assert_eq!(c64.last(), Some(&Source::Tosec("Commodore C64")));
        assert_eq!(c64.last().unwrap().url(), None);
        assert!(sources(systems::by_key("Arcade").unwrap(), true).is_empty());
    }

    #[test]
    fn finds_latest_tosec_release_and_pack_link() {
        let index = r#"<a href="/downloads/category/22-datfiles">x</a>
            <a href="/downloads/category/59-2025-03-13">x</a>
            <a href="/downloads/category/58-2024-03-31">x</a>
            <a href="/downloads/category/8-tosec-dat-explorer">x</a>"#;
        assert_eq!(
            latest_tosec_release(index).as_deref(),
            Some("/downloads/category/59-2025-03-13")
        );
        let page = r#"<a href="/downloads/category/22-datfiles">x</a>
            <a href="/downloads/category/59-2025-03-13?download=117:tosec-dat-pack-complete-4743-tosec-v2025-03-13&amp;x=1">DAT</a>"#;
        assert_eq!(
            tosec_pack_link(page).as_deref(),
            Some(
                "/downloads/category/59-2025-03-13?download=117:tosec-dat-pack-complete-4743-tosec-v2025-03-13&x=1"
            )
        );
        assert_eq!(latest_tosec_release("<html></html>"), None);
    }

    #[test]
    fn source_families() {
        assert_eq!(
            source_family("https://www.tosecdev.org/a?download=1#TOSEC/x.dat"),
            "tosec"
        );
        assert_eq!(
            source_family("http://redump.org/datfile/psx/"),
            "http://redump.org/datfile/psx/"
        );
        assert_eq!(source_family("/home/me/x.dat"), "file");
    }

    #[test]
    fn parses_profile() {
        let xml = r#"<clrmamepro>
  <datfile><name>Nintendo - Game Boy</name><description>x</description><version>20260101-000000</version><url>_</url></datfile>
  <datfile><name>Sega - Mega Drive - Genesis</name><version>20250505-121212</version></datfile>
</clrmamepro>"#;
        let p = parse_profile(xml.as_bytes()).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "Nintendo - Game Boy");
        assert_eq!(p[0].version, "20260101-000000");
        assert_eq!(p[1].name, "Sega - Mega Drive - Genesis");
    }

    #[test]
    fn plain_dat_is_passed_through() {
        let d = dats_in(b"<datafile/>").unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(
            (d[0].member.as_deref(), &d[0].bytes[..]),
            (None, &b"<datafile/>"[..])
        );
    }
}
