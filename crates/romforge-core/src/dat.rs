//! Logiqx XML DAT parsing (No-Intro, Redump, TOSEC and MAME-style `<machine>`).

use std::io::BufRead;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::Error;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dat {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub url: Option<String>,
    /// `<clrmamepro header="No-Intro_NES.xml"/>`: the DAT's hashes are of
    /// header-stripped data.
    pub header_hint: Option<String>,
    pub games: Vec<Game>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    pub description: Option<String>,
    pub roms: Vec<Rom>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Rom {
    pub name: String,
    pub size: Option<u64>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    /// e.g. `baddump`, `nodump`, `verified`.
    pub status: Option<String>,
}

pub fn parse(input: impl BufRead) -> Result<Dat, Error> {
    let mut reader = quick_xml::Reader::from_reader(input);
    // Text is trimmed per element at `End`; trimming fragments would eat the
    // spaces around `&amp;`.

    let mut dat = Dat::default();
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut saw_root = false;
    let mut buf = Vec::new();

    loop {
        let event = reader.read_event_into(&mut buf).map_err(xml_err)?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let tag = e.local_name().as_ref().to_owned();
                text.clear();
                match (stack.last().map(String::as_str), tag.as_str()) {
                    (None, "datafile") => saw_root = true,
                    (Some("header"), "clrmamepro") => dat.header_hint = attr(e, "header")?,
                    (Some("datafile"), "game" | "machine") => dat.games.push(Game {
                        name: attr(e, "name")?.unwrap_or_default(),
                        ..Game::default()
                    }),
                    (Some("game" | "machine"), "rom") => {
                        let rom = Rom {
                            name: attr(e, "name")?.unwrap_or_default(),
                            size: attr(e, "size")?.and_then(|s| parse_size(&s)),
                            crc32: hash_attr(e, "crc")?,
                            md5: hash_attr(e, "md5")?,
                            sha1: hash_attr(e, "sha1")?,
                            status: attr(e, "status")?,
                        };
                        if let Some(g) = dat.games.last_mut() {
                            g.roms.push(rom);
                        }
                    }
                    _ => {}
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(tag);
                }
            }
            Event::Text(t) => text.push_str(&t.xml_content(XmlVersion::Implicit1_0)),
            Event::CData(t) => text.push_str(&t.into_inner()),
            Event::GeneralRef(r) => {
                if let Some(c) = r.resolve_char_ref().map_err(xml_err)? {
                    text.push(c);
                } else if let Some(s) = quick_xml::escape::resolve_predefined_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::End(_) => {
                let tag = stack.pop().unwrap_or_default();
                let value = std::mem::take(&mut text).trim().to_owned();
                let value = (!value.is_empty()).then_some(value);
                match (stack.last().map(String::as_str), tag.as_str()) {
                    (Some("header"), "name") => dat.name = value.unwrap_or_default(),
                    (Some("header"), "description") => dat.description = value,
                    (Some("header"), "version") => dat.version = value,
                    (Some("header"), "author") => dat.author = value,
                    (Some("header"), "homepage") => dat.homepage = value,
                    (Some("header"), "url") => dat.url = value,
                    (Some("game" | "machine"), "description") => {
                        if let Some(g) = dat.games.last_mut() {
                            g.description = value;
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if !saw_root {
        return Err(Error::Dat("not a Logiqx XML DAT (no <datafile>)".into()));
    }
    Ok(dat)
}

fn xml_err(e: impl std::fmt::Display) -> Error {
    Error::Dat(e.to_string())
}

fn attr(e: &BytesStart, name: &str) -> Result<Option<String>, Error> {
    for a in e.attributes() {
        let a = a.map_err(xml_err)?;
        if a.key.local_name().as_ref() == name {
            let v = a.normalized_value(XmlVersion::Implicit1_0).map_err(xml_err)?;
            return Ok(Some(v.trim().to_owned()));
        }
    }
    Ok(None)
}

fn hash_attr(e: &BytesStart, name: &str) -> Result<Option<String>, Error> {
    Ok(attr(e, name)?
        .map(|h| h.to_ascii_lowercase())
        .filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit())))
}

fn parse_size(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAT: &str = r#"<?xml version="1.0"?>
<!DOCTYPE datafile PUBLIC "-//Logiqx//DTD ROM Management Datafile//EN" "http://www.logiqx.com/Dats/datafile.dtd">
<datafile>
  <header>
    <name>Nintendo - Nintendo Entertainment System (Headerless)</name>
    <description>Test &amp; more</description>
    <version>20240101</version>
    <clrmamepro header="No-Intro_NES.xml"/>
  </header>
  <game name="A (USA)">
    <description>A (USA)</description>
    <rom name="A (USA).nes" size="1024" crc="ABCDEF01" md5="00112233445566778899AABBCCDDEEFF" sha1="0123456789abcdef0123456789abcdef01234567" status="verified"/>
  </game>
  <machine name="b"><rom name="b.bin" size="0x10" crc="00000001"/></machine>
</datafile>"#;

    #[test]
    fn parses_header_games_and_roms() {
        let dat = parse(DAT.as_bytes()).unwrap();
        assert_eq!(dat.name, "Nintendo - Nintendo Entertainment System (Headerless)");
        assert_eq!(dat.description.as_deref(), Some("Test & more"));
        assert_eq!(dat.version.as_deref(), Some("20240101"));
        assert_eq!(dat.header_hint.as_deref(), Some("No-Intro_NES.xml"));
        assert_eq!(dat.games.len(), 2);
        let rom = &dat.games[0].roms[0];
        assert_eq!(rom.size, Some(1024));
        assert_eq!(rom.crc32.as_deref(), Some("abcdef01"));
        assert_eq!(rom.md5.as_deref(), Some("00112233445566778899aabbccddeeff"));
        assert_eq!(rom.status.as_deref(), Some("verified"));
        assert_eq!(dat.games[1].roms[0].size, Some(16));
        assert_eq!(dat.games[1].roms[0].sha1, None);
    }

    #[test]
    fn rejects_non_dat() {
        assert!(parse("<html></html>".as_bytes()).is_err());
    }
}
