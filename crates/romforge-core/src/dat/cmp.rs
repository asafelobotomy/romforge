use super::*;

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
            // A quoted string ends at a `"` followed by whitespace, `)` or
            // the end, so stray quotes inside survive
            // (`developer ""Natsume", Vicarious Visions"`).
            '"' => {
                let body = &self.s[1..];
                let end = body.match_indices('"').map(|(i, _)| i).find(|&i| {
                    body[i + 1..]
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_whitespace() || c == ')')
                });
                match end {
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

/// How deep `( ... )` blocks may nest. Real DATs nest two or three deep;
/// a limit keeps a malformed or hostile file from overflowing the stack.
const MAX_DEPTH: usize = 32;

/// One block's items; `depth` 0 is the whole file.
fn cmp_block<'a>(tokens: &mut std::iter::Peekable<Tokens<'a>>, depth: usize) -> Result<Cmp<'a>, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::Dat(format!(
            "clrmamepro: blocks nested over {MAX_DEPTH} deep"
        )));
    }
    let top = depth == 0;
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
            Some("(") => cmp_block(tokens, depth + 1)?,
            Some(")") | None => return Err(Error::Dat(format!("clrmamepro: {key:?} has no value"))),
            Some(v) => Cmp::Atom(v),
        };
        items.push((key, value));
    }
}

/// Parse a clrmamepro text DAT.
pub fn parse_cmp(text: &str) -> Result<Dat, Error> {
    let root = cmp_block(&mut Tokens { s: text }.peekable(), 0)?;
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
            cloneof: g.get_owned("cloneof"),
            region: g.get_owned("region"),
            category: g.get_owned("category"),
            serials: g.get("serial").map(split_serials).unwrap_or_default(),
            roms: g
                .blocks("rom")
                .map(|r| Rom {
                    name: r.get_owned("name").unwrap_or_default(),
                    size: r.get("size").and_then(parse_size),
                    crc32: cmp_hash(r.get("crc")),
                    md5: cmp_hash(r.get("md5")),
                    sha1: cmp_hash(r.get("sha1")),
                    status: r.get_owned("status").or_else(|| r.get_owned("flags")),
                    disk: false,
                })
                .chain(g.blocks("disk").map(|d| Rom {
                    name: format!("{}.chd", d.get_owned("name").unwrap_or_default()),
                    sha1: cmp_hash(d.get("sha1")),
                    md5: cmp_hash(d.get("md5")),
                    status: d.get_owned("status").or_else(|| d.get_owned("flags")),
                    disk: true,
                    ..Rom::default()
                }))
                .collect(),
        });
    }
    Ok(dat)
}

/// One `game ( ... )` entry of a clrmamepro metadata file (libretro's
/// `metadat/<field>/` DATs): its title (`name` or `comment`), its plain
/// fields, and the CRC32s and serials of its `rom` blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub title: Option<String>,
    pub fields: Vec<(String, String)>,
    pub crcs: Vec<String>,
    pub serials: Vec<String>,
}

/// libretro writes some lists as several quoted strings
/// (`developer "Ape", "HAL Laboratory"`), which the tokenizer keeps as one
/// atom with the inner quotes: make that a plain `Ape, HAL Laboratory`.
fn list_value(a: &str) -> String {
    a.replace("\", \"", ", ")
        .replace("\", ", ", ")
        .trim_matches('"')
        .to_owned()
}

/// Parse a clrmamepro metadata file into [`Record`]s.
pub fn parse_cmp_records(text: &str) -> Result<Vec<Record>, Error> {
    let root = cmp_block(&mut Tokens { s: text }.peekable(), 0)?;
    let mut out = Vec::new();
    for g in root.blocks("game") {
        let Cmp::Block(items) = g else { continue };
        let mut rec = Record {
            title: g.get_owned("name").or_else(|| g.get_owned("comment")),
            ..Record::default()
        };
        for (k, v) in items {
            if let Cmp::Atom(a) = v
                && !matches!(*k, "name" | "comment")
                && !a.is_empty()
            {
                rec.fields.push((k.to_ascii_lowercase(), list_value(a)));
            }
        }
        for r in g.blocks("rom") {
            if let Some(crc) = cmp_hash(r.get("crc")) {
                rec.crcs.push(crc);
            }
            if let Some(serial) = r.get_owned("serial") {
                rec.serials.push(serial);
            }
        }
        out.push(rec);
    }
    Ok(out)
}

/// A `game ( ... )` block of a clrmamepro-style file as plain pairs: its
/// own fields (lower-case keys) and each `rom ( ... )` block's.
pub(crate) struct CmpGame {
    pub fields: std::collections::HashMap<String, String>,
    pub roms: Vec<CmpRom>,
}

/// One `rom ( ... )` block's pairs, in order, keys in lower case.
pub(crate) struct CmpRom(Vec<(String, String)>);

impl CmpRom {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.0.iter()
    }
}

/// Every `game` block of a clrmamepro-style file, whatever its header
/// (ScummVM's detection list has its own).
pub(crate) fn cmp_games(text: &str) -> Result<Vec<CmpGame>, Error> {
    let root = cmp_block(&mut Tokens { s: text }.peekable(), 0)?;
    let pairs = |b: &Cmp| -> Vec<(String, String)> {
        match b {
            Cmp::Block(items) => items
                .iter()
                .filter_map(|(k, v)| match v {
                    Cmp::Atom(a) => Some((k.to_ascii_lowercase(), (*a).to_owned())),
                    Cmp::Block(_) => None,
                })
                .collect(),
            Cmp::Atom(_) => Vec::new(),
        }
    };
    Ok(root
        .blocks("game")
        .map(|g| CmpGame {
            fields: pairs(g).into_iter().collect(),
            roms: g.blocks("rom").map(|r| CmpRom(pairs(r))).collect(),
        })
        .collect())
}

fn cmp_hash(h: Option<&str>) -> Option<String> {
    h.map(str::to_ascii_lowercase)
        .filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
}
