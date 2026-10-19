use super::*;

/// Parse a Logiqx XML DAT.
pub fn parse(input: impl BufRead) -> Result<Dat, Error> {
    let mut reader = quick_xml::Reader::from_reader(input);
    // Text is trimmed per element at `End`; trimming fragments would eat the
    // spaces around `&amp;`.

    let mut dat = Dat::default();
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut saw_root = false;
    let mut skip_machine = false;
    let mut buf = Vec::new();

    loop {
        let event = reader.read_event_into(&mut buf).map_err(xml_err)?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let tag = e.local_name().as_ref().to_owned();
                text.clear();
                match (stack.last().map(String::as_str), tag.as_str()) {
                    (None, "datafile") => saw_root = true,
                    // MAME's own `-listxml`: no header, its build on the root.
                    (None, "mame") => {
                        saw_root = true;
                        dat.version = attr(e, "build")?;
                        if dat.version.is_some() {
                            dat.name = "MAME".to_owned();
                            dat.description = Some("MAME's own list (-listxml)".to_owned());
                        }
                    }
                    (Some("header"), "clrmamepro") => dat.header_hint = attr(e, "header")?,
                    // MAME's devices are parts of machines, not games.
                    (Some("datafile" | "mame"), "game" | "machine")
                        if attr(e, "isdevice")?.as_deref() == Some("yes") =>
                    {
                        skip_machine = true;
                    }
                    (Some("datafile" | "mame"), "game" | "machine") => {
                        skip_machine = false;
                        dat.games.push(Game {
                            name: attr(e, "name")?.unwrap_or_default(),
                            cloneof: attr(e, "cloneof")?,
                            ..Game::default()
                        })
                    }
                    (Some("game" | "machine"), "rom" | "disk") if !skip_machine => {
                        let disk = tag == "disk";
                        let name = attr(e, "name")?.unwrap_or_default();
                        let optional = attr(e, "optional")?.as_deref() == Some("yes");
                        let rom = Rom {
                            name: if disk { format!("{name}.chd") } else { name },
                            size: if disk {
                                None
                            } else {
                                attr(e, "size")?.and_then(|s| parse_size(&s))
                            },
                            crc32: hash_attr(e, "crc")?,
                            md5: hash_attr(e, "md5")?,
                            sha1: hash_attr(e, "sha1")?,
                            status: match attr(e, "status")? {
                                Some(s) if s == "nodump" => Some(s),
                                _ if optional => Some("optional".to_owned()),
                                s => s,
                            },
                            disk,
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
                    (Some("game" | "machine"), field) if !skip_machine => {
                        if let Some(g) = dat.games.last_mut() {
                            match field {
                                "description" => g.description = value,
                                "category" => g.category = value,
                                "region" => g.region = value,
                                "serial" => g
                                    .serials
                                    .extend(value.as_deref().map(split_serials).unwrap_or_default()),
                                _ => {}
                            }
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

pub(super) fn parse_size(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}
