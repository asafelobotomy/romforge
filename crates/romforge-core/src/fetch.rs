//! Downloading DATs.
//!
//! Redump serves each system's DAT as a public zip at
//! `http://redump.org/datfile/<code>/` (the codes are the `redump` column in
//! [`crate::systems`]). No-Intro's data is free to reuse, but DAT-o-MATIC
//! serves DATs only through its download form, so romforge does not fetch
//! them: it reads the public `profile.xml` version list to say which No-Intro
//! DATs are missing or out of date, and the user downloads those by hand.
//!
//! Requests to one host are at least [`POLITE_DELAY`] apart (DAT-o-MATIC's
//! robots.txt asks for 5 s). Downloads stay in memory and are never written
//! to disk.

use std::collections::HashMap;
use std::io::{BufRead, Cursor, Read};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use ureq::tls::{RootCerts, TlsConfig};

use crate::Error;
use crate::systems::System;

pub const USER_AGENT: &str = concat!("romforge/", env!("CARGO_PKG_VERSION"));
pub const REDUMP_BASE: &str = "http://redump.org";
pub const NOINTRO_PROFILE_URL: &str = "https://datomatic.no-intro.org/profile.xml";
pub const NOINTRO_DOWNLOAD_PAGE: &str = "https://datomatic.no-intro.org/index.php?page=download";
pub const POLITE_DELAY: Duration = Duration::from_secs(5);
/// No DAT zip is near this; it only stops a runaway response.
const MAX_BODY: u64 = 256 << 20;

/// The URL of a system's Redump DAT zip, if Redump has one.
pub fn redump_url(system: &System) -> Option<String> {
    system.redump.map(|code| format!("{REDUMP_BASE}/datfile/{code}/"))
}

/// GET `url` into memory, waiting first if the same host was asked less than
/// [`POLITE_DELAY`] ago.
pub fn get(url: &str) -> Result<Vec<u8>, Error> {
    wait_for_host(url);
    let agent = ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_global(Some(Duration::from_secs(120)))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Http(format!("{url}: {e}")))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()
        .map_err(|e| Error::Http(format!("{url}: {e}")))
}

fn wait_for_host(url: &str) {
    static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
    let host = url.split('/').nth(2).unwrap_or(url).to_owned();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let last = last.get_or_insert_with(HashMap::new);
    if let Some(t) = last.get(&host) {
        let since = t.elapsed();
        if since < POLITE_DELAY {
            std::thread::sleep(POLITE_DELAY - since);
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
    fn redump_urls_come_from_the_systems_table() {
        let psx = systems::by_key("PlayStation").unwrap();
        assert_eq!(redump_url(psx).as_deref(), Some("http://redump.org/datfile/psx/"));
        assert_eq!(redump_url(systems::by_key("SNES").unwrap()), None);
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
