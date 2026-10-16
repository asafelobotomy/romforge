//! DAT parsing: Logiqx XML (No-Intro, Redump, TOSEC and MAME-style
//! `<machine>`) and the older clrmamepro text format (`game ( rom ( ... ) )`).

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

/// Parse either format, telling them apart by the first non-blank byte.
pub fn parse_any(bytes: &[u8]) -> Result<Dat, Error> {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace());
    match start.map(|i| bytes[i]) {
        Some(b'<') => parse(bytes),
        // Skip a UTF-8 BOM.
        Some(0xef) if bytes[start.unwrap_or(0)..].starts_with(b"\xef\xbb\xbf<") => parse(bytes),
        _ => parse_cmp(&String::from_utf8_lossy(bytes)),
    }
}

/// Parse a Logiqx XML DAT.
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

/// A clrmamepro value: a word or quoted string, or a `( ... )` block of
/// key/value pairs.
enum Cmp<'a> {
    Atom(&'a str),
    Block(Vec<(&'a str, Cmp<'a>)>),
}

impl<'a> Cmp<'a> {
    fn get(&self, key: &str) -> Option<&'a str> {
        match self {
            Cmp::Block(items) => items.iter().find_map(|(k, v)| match v {
                Cmp::Atom(a) if k.eq_ignore_ascii_case(key) => Some(*a),
                _ => None,
            }),
            Cmp::Atom(_) => None,
        }
    }

    fn get_owned(&self, key: &str) -> Option<String> {
        self.get(key).map(str::to_owned).filter(|s| !s.is_empty())
    }

    fn blocks(&self, key: &'static str) -> impl Iterator<Item = &Cmp<'a>> {
        let items = match self {
            Cmp::Block(items) => &items[..],
            Cmp::Atom(_) => &[],
        };
        items
            .iter()
            .filter(move |(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
            .filter(|v| matches!(v, Cmp::Block(_)))
    }
}

/// Splits clrmamepro text into `(`, `)`, quoted strings and bare words.
/// Quoted strings have no escapes in this format.
struct Tokens<'a> {
    s: &'a str,
}

impl<'a> Iterator for Tokens<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        self.s = self.s.trim_start();
        let first = self.s.chars().next()?;
        let (token, rest) = match first {
            '(' | ')' => self.s.split_at(1),
            '"' => {
                let body = &self.s[1..];
                match body.find('"') {
                    Some(end) => (&body[..end], &body[end + 1..]),
                    None => (body, ""),
                }
            }
            // A bare word runs to whitespace and may hold balanced parens
            // (`name ps-22j(v).bin`); an unmatched `)` ends it.
            _ => {
                let mut depth = 0usize;
                let end = self
                    .s
                    .char_indices()
                    .find(|&(_, c)| match c {
                        '(' => {
                            depth += 1;
                            false
                        }
                        ')' if depth == 0 => true,
                        ')' => {
                            depth -= 1;
                            false
                        }
                        c => c.is_whitespace(),
                    })
                    .map_or(self.s.len(), |(i, _)| i);
                self.s.split_at(end)
            }
        };
        self.s = rest;
        Some(token)
    }
}

fn cmp_block<'a>(tokens: &mut std::iter::Peekable<Tokens<'a>>, top: bool) -> Result<Cmp<'a>, Error> {
    let mut items = Vec::new();
    loop {
        let Some(key) = tokens.next() else {
            return if top {
                Ok(Cmp::Block(items))
            } else {
                Err(Error::Dat("clrmamepro: unclosed (".into()))
            };
        };
        if key == ")" {
            return if top {
                Err(Error::Dat("clrmamepro: unbalanced )".into()))
            } else {
                Ok(Cmp::Block(items))
            };
        }
        let value = match tokens.next() {
            Some("(") => cmp_block(tokens, false)?,
            Some(")") | None => return Err(Error::Dat(format!("clrmamepro: {key:?} has no value"))),
            Some(v) => Cmp::Atom(v),
        };
        items.push((key, value));
    }
}

/// Parse a clrmamepro text DAT.
pub fn parse_cmp(text: &str) -> Result<Dat, Error> {
    let root = cmp_block(&mut Tokens { s: text }.peekable(), true)?;
    let header = root
        .blocks("clrmamepro")
        .next()
        .ok_or_else(|| Error::Dat("not a DAT (no <datafile> or clrmamepro header)".into()))?;
    let mut dat = Dat {
        name: header.get_owned("name").unwrap_or_default(),
        description: header.get_owned("description"),
        version: header.get_owned("version"),
        author: header.get_owned("author"),
        homepage: header.get_owned("homepage"),
        url: header.get_owned("url"),
        header_hint: header.get_owned("header"),
        games: Vec::new(),
    };
    let games = root
        .blocks("game")
        .chain(root.blocks("machine"))
        .chain(root.blocks("resource"));
    for g in games {
        dat.games.push(Game {
            name: g.get_owned("name").unwrap_or_default(),
            description: g.get_owned("description"),
            roms: g
                .blocks("rom")
                .map(|r| Rom {
                    name: r.get_owned("name").unwrap_or_default(),
                    size: r.get("size").and_then(parse_size),
                    crc32: cmp_hash(r.get("crc")),
                    md5: cmp_hash(r.get("md5")),
                    sha1: cmp_hash(r.get("sha1")),
                    status: r.get_owned("status").or_else(|| r.get_owned("flags")),
                })
                .collect(),
        });
    }
    Ok(dat)
}

fn cmp_hash(h: Option<&str>) -> Option<String> {
    h.map(str::to_ascii_lowercase)
        .filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
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
        assert!(parse_any(b"<!DOCTYPE html><html></html>").is_err());
        assert!(parse_any(b"hello ( world )").is_err());
        assert!(parse_any(b"clrmamepro ( name \"x\" ").is_err());
    }

    const CMP: &str = r#"clrmamepro (
	name "Sony - PlayStation - BIOS Images"
	description "Sony - PlayStation - BIOS Images (24) (2016-10-21)"
	version 2016-10-21
	author "MESS & redump.org"
)

game (
	name "ps-10j"
	description "SCPH-1000/DTL-H1000 (Version 1.0 J)"
	rom ( name ps-22j(v).bin size 524288 crc 3b601fc8 md5 239665B1a3dade1b5a52c06338011044 sha1 343883a7b555646da8cee54aadd2795b6e7dd070 )
)
game (
	name "Super Mario Bros. (World)"
	region "World"
	rom ( name "Super Mario Bros. (World).nes" size 40976 crc 393A432F flags baddump )
	rom ( name "second (rom).bin" size 0x10 )
)
"#;

    #[test]
    fn parses_clrmamepro_text() {
        let dat = parse_any(CMP.as_bytes()).unwrap();
        assert_eq!(dat.name, "Sony - PlayStation - BIOS Images");
        assert_eq!(dat.version.as_deref(), Some("2016-10-21"));
        assert_eq!(dat.author.as_deref(), Some("MESS & redump.org"));
        assert_eq!(dat.games.len(), 2);
        let bios = &dat.games[0];
        assert_eq!(
            bios.description.as_deref(),
            Some("SCPH-1000/DTL-H1000 (Version 1.0 J)")
        );
        assert_eq!(bios.roms[0].name, "ps-22j(v).bin");
        assert_eq!(
            bios.roms[0].md5.as_deref(),
            Some("239665b1a3dade1b5a52c06338011044")
        );
        let smb = &dat.games[1];
        assert_eq!(smb.roms.len(), 2);
        assert_eq!(smb.roms[0].name, "Super Mario Bros. (World).nes");
        assert_eq!(smb.roms[0].crc32.as_deref(), Some("393a432f"));
        assert_eq!(smb.roms[0].status.as_deref(), Some("baddump"));
        assert_eq!(smb.roms[1].size, Some(16));
    }

    #[test]
    fn xml_still_detected() {
        assert_eq!(parse_any(DAT.as_bytes()).unwrap().games.len(), 2);
    }
}
