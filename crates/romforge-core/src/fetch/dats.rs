use super::*;

/// A zip member read into memory, refused beyond [`MAX_UNPACKED`] (a
/// small zip can claim to hold anything).
pub(super) fn read_member(member: &mut impl Read) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    member.take(MAX_UNPACKED + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_UNPACKED {
        return Err(Error::Dat(format!(
            "a zip member over {} GiB",
            MAX_UNPACKED >> 30
        )));
    }
    Ok(buf)
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
            let buf = read_member(&mut member)?;
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
