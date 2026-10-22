//! Steam's two VDF formats, as far as romforge needs them: the binary one
//! of `shortcuts.vdf` (read and written whole, unknown keys and their order
//! kept), and the text one of `loginusers.vdf` (read only).
//!
//! Binary: a value is a type byte, its key (UTF-8, `00`-ended), then the
//! value: `00` a map (values until `08`), `01` a string (`00`-ended), `02`
//! a little-endian 32-bit int. The file is one map.

use crate::Error;

/// A binary VDF value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Map(Map),
    Str(String),
    Int(u32),
}

/// Keys in file order.
pub type Map = Vec<(String, Value)>;

const MAP: u8 = 0x00;
const STR: u8 = 0x01;
const INT: u8 = 0x02;
const END: u8 = 0x08;

fn bad(why: &str) -> Error {
    Error::Settings(format!("shortcuts.vdf: {why}"))
}

/// Parse a whole binary VDF file (its top map). An empty file is an empty map.
pub fn read(bytes: &[u8]) -> Result<Map, Error> {
    let mut at = 0;
    let map = map(bytes, &mut at, true)?;
    Ok(map)
}

fn cstr(b: &[u8], at: &mut usize) -> Result<String, Error> {
    let end = b[*at..]
        .iter()
        .position(|&c| c == 0)
        .ok_or_else(|| bad("a string isn't ended"))?;
    let s = String::from_utf8_lossy(&b[*at..*at + end]).into_owned();
    *at += end + 1;
    Ok(s)
}

fn map(b: &[u8], at: &mut usize, top: bool) -> Result<Map, Error> {
    let mut out = Map::new();
    loop {
        let Some(&t) = b.get(*at) else {
            // The top map may end with the file (some files lack the last 08).
            return if top {
                Ok(out)
            } else {
                Err(bad("a map isn't ended"))
            };
        };
        *at += 1;
        if t == END {
            return Ok(out);
        }
        let key = cstr(b, at)?;
        let value = match t {
            MAP => Value::Map(map(b, at, false)?),
            STR => Value::Str(cstr(b, at)?),
            INT => {
                let v = b.get(*at..*at + 4).ok_or_else(|| bad("an int is cut short"))?;
                *at += 4;
                Value::Int(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            }
            other => return Err(bad(&format!("unknown value type {other:#04x}"))),
        };
        out.push((key, value));
    }
}

/// The bytes of `top` as a binary VDF file.
pub fn write(top: &Map) -> Vec<u8> {
    let mut out = Vec::new();
    put_map(&mut out, top);
    out
}

fn put_map(out: &mut Vec<u8>, m: &Map) {
    for (k, v) in m {
        let t = match v {
            Value::Map(_) => MAP,
            Value::Str(_) => STR,
            Value::Int(_) => INT,
        };
        out.push(t);
        out.extend_from_slice(k.as_bytes());
        out.push(0);
        match v {
            Value::Map(inner) => put_map(out, inner),
            Value::Str(s) => {
                out.extend_from_slice(s.as_bytes());
                out.push(0);
            }
            Value::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
        }
    }
    out.push(END);
}

/// The value at `key` (any case: Steam's own casing varies).
pub fn get<'a>(m: &'a Map, key: &str) -> Option<&'a Value> {
    m.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

pub fn get_str<'a>(m: &'a Map, key: &str) -> Option<&'a str> {
    match get(m, key) {
        Some(Value::Str(s)) => Some(s),
        _ => None,
    }
}

/// A text VDF file (`"key" "value"`, `"key" { … }`) as nested maps; values
/// are strings. Comments (`//`) and `#include` lines are skipped.
pub fn read_text(text: &str) -> Result<Map, Error> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(match n {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        '"' => break,
                        c => s.push(c),
                    }
                }
                tokens.push(Some(s));
            }
            '{' => tokens.push(None),
            '}' => tokens.push(Some("\u{0}}".into())),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '#' => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    let mut at = 0;
    text_map(&tokens, &mut at)
}

fn text_map(t: &[Option<String>], at: &mut usize) -> Result<Map, Error> {
    let mut out = Map::new();
    while let Some(tok) = t.get(*at) {
        *at += 1;
        let key = match tok {
            Some(k) if k == "\u{0}}" => return Ok(out),
            Some(k) => k.clone(),
            None => return Err(Error::Settings("vdf: a { with no key".into())),
        };
        match t.get(*at) {
            Some(None) => {
                *at += 1;
                out.push((key, Value::Map(text_map(t, at)?)));
            }
            Some(Some(v)) if v != "\u{0}}" => {
                *at += 1;
                out.push((key, Value::Str(v.clone())));
            }
            _ => return Err(Error::Settings(format!("vdf: {key:?} has no value"))),
        }
    }
    Ok(out)
}
