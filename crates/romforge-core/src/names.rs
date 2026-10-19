//! What a name says: title, regions, languages, version, release status,
//! disc, and dump / hack / translation flags (milestone 12.4).
//!
//! DAT names and well-kept files follow No-Intro, Redump (No-Intro's
//! form), TOSEC or GoodTools naming, parsed by `shiratsu-naming`. Anything
//! else ("Dragon Quest III (English v2.0)\[Addendum\]", "zelda3") gets a
//! loose reading: the text before the first bracket is the title, and the
//! groups are looked at for translation and hack markers. A name is never
//! evidence of a match by itself (house rule 8).

use shiratsu_naming::naming::TokenizedName;
use shiratsu_naming::naming::goodtools::{GoodToolsName, GoodToolsToken};
use shiratsu_naming::naming::nointro::NoIntroName;
use shiratsu_naming::naming::tosec::{TOSECName, TOSECToken};
use shiratsu_naming::region::Region;

mod conventions;
#[cfg(test)]
mod tests;

use conventions::{codes, from_goodtools, from_nointro, from_tosec};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Convention {
    /// No-Intro, and Redump (which follows it).
    NoIntro,
    Tosec,
    GoodTools,
    /// None of them: title before the first bracket, groups looked at.
    #[default]
    Loose,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Name {
    pub convention: Convention,
    /// As written, articles where the convention puts them
    /// ("Legend of Zelda, The - A Link to the Past").
    pub title: String,
    /// Regions as written ("USA", "World", "J").
    pub regions: Vec<String>,
    /// The same as two-letter codes ("US", "JP", "EU"); "World" expands.
    pub region_codes: Vec<String>,
    /// Language codes as written ("En", "Ja", "en").
    pub languages: Vec<String>,
    /// "Rev 1", "v1.1", "Rev A".
    pub version: Option<String>,
    /// Not a retail release: "Beta", "Proto", "Demo", "Sample", ...
    pub status: Option<String>,
    /// Disc (or disk, side) number.
    pub disc: Option<String>,
    /// A known bad dump (`[b]`, `[b1]`).
    pub bad_dump: bool,
    /// GoodTools / TOSEC `[!]`: a verified good dump.
    pub verified_dump: bool,
    /// An alternate dump (`[a]`, `[a2]`).
    pub alternate: bool,
    /// A hack, with its tag ("h1C", "Hack", "SMW Hack").
    pub hack: Option<String>,
    /// A translation, with its tag ("Eng2.0_Addendum", "en Addendum",
    /// "English v2.0 Addendum").
    pub translation: Option<String>,
    /// `(Unl)`, `(Aftermarket)` and the like (a pirate copy is a `status`).
    pub unlicensed: bool,
}

/// Release statuses that aren't a retail release (No-Intro's words).
const NOT_RETAIL: &[&str] = &[
    "beta",
    "proto",
    "demo",
    "sample",
    "pirate",
    "kiosk",
    "debug",
    "preview",
    "promo",
    "test program",
];

/// File extensions dropped before parsing ("Game (USA).sfc"): a dot, then
/// at most five letters or digits and nothing after.
fn without_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.bytes().all(|b| b.is_ascii_alphanumeric())
                && !ext.bytes().all(|b| b.is_ascii_digit()) =>
        {
            stem
        }
        _ => name,
    }
}

/// Read a DAT name or file name.
pub fn parse(name: &str) -> Name {
    let name = without_extension(name.trim());
    if let Ok(n) = NoIntroName::try_parse(name) {
        return from_nointro(&n);
    }
    if let Ok(n) = TOSECName::try_parse(name)
        && n.iter().any(|t| matches!(t, TOSECToken::Date { .. }))
    {
        return from_tosec(&n);
    }
    // GoodTools only when it says something only GoodTools says (its
    // parser takes nearly any name, and would drop what it can't read).
    if let Ok(n) = GoodToolsName::try_parse(name)
        && n.iter().any(|t| {
            matches!(
                t,
                GoodToolsToken::Region { .. }
                    | GoodToolsToken::DumpCode { .. }
                    | GoodToolsToken::Translation { .. }
                    | GoodToolsToken::GameHack(_)
            )
        })
    {
        return from_goodtools(&n);
    }
    loose(name)
}

/// What a free-form bracket group says: hack, translation, bad dump,
/// release status, unlicensed.
fn flag_words(out: &mut Name, flag: &str) {
    let f = flag.trim();
    let lower = f.to_lowercase();
    let first = lower.split([' ', ',']).next().unwrap_or("");
    match lower.as_str() {
        "b" => out.bad_dump = true,
        "!" => out.verified_dump = true,
        "a" => out.alternate = true,
        "unl" | "aftermarket" | "bootleg" => out.unlicensed = true,
        _ if NOT_RETAIL.contains(&first) || NOT_RETAIL.contains(&lower.as_str()) => {
            out.status = Some(f.to_owned())
        }
        _ if lower.contains("hack") || first == "h" => out.hack = Some(f.to_owned()),
        _ if is_translation(&lower) => out.translation = Some(f.to_owned()),
        _ => {}
    }
}

/// "T+Eng", "T-En", "tr en", "English v2.0", "English Translation",
/// "Translated".
fn is_translation(lower: &str) -> bool {
    lower.starts_with("t+")
        || lower.starts_with("t-")
        || lower.starts_with("tr ")
        || lower.contains("translat")
        || lower.split(|c: char| !c.is_ascii_alphabetic()).any(|w| {
            matches!(
                w,
                "english" | "eng" | "spanish" | "french" | "german" | "italian" | "portuguese"
            )
        })
}

/// Anything else: the title is the text before the first bracket; each
/// group is read for flags, a version, a region.
fn loose(name: &str) -> Name {
    let mut out = Name::default();
    let cut = name.find(['(', '[']).unwrap_or(name.len());
    out.title = name[..cut].trim().to_owned();
    let (mut depth, mut cur, mut groups) = (0, String::new(), Vec::new());
    for c in name[cut..].chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                cur.clear();
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                groups.push(std::mem::take(&mut cur));
            }
            c if depth > 0 => cur.push(c),
            _ => {}
        }
    }
    // "(English v2.0)[Addendum]": a bracket right after a translation tag
    // belongs to it.
    let mut i = 0;
    while i < groups.len() {
        let g = groups[i].trim().to_owned();
        if let Ok(regions) = Region::try_from_nointro_region(&g) {
            out.regions = g.split(", ").map(str::to_owned).collect();
            out.region_codes = codes(&regions);
        } else if let Some(n) = disc_of(&g) {
            out.disc = Some(n);
        } else if i == 0 && is_place(&g) {
            // No-Intro's first group is the region, also one shiratsu
            // doesn't know ("Czech", "Ukraine"): kept as written, no code.
            out.regions = g.split(", ").map(str::to_owned).collect();
        } else if let Some(v) = version_of(&g) {
            out.version = Some(v);
        } else {
            flag_words(&mut out, &g);
            if out.translation.as_deref() == Some(g.as_str())
                && let Some(next) = groups.get(i + 1)
                && version_of(next).is_none()
                && Region::try_from_nointro_region(next.trim()).is_err()
            {
                out.translation = Some(format!("{g} {}", next.trim()));
                i += 1;
            }
        }
        i += 1;
    }
    out
}

/// "Disc 2", "Disk 1", "Side B".
fn disc_of(g: &str) -> Option<String> {
    let (kind, n) = g.trim().split_once(' ')?;
    (matches!(kind, "Disc" | "Disk" | "CD" | "Side") && !n.contains(' ')).then(|| n.to_owned())
}

/// Capitalized words only, comma-separated: "Czech", "Latin America".
fn is_place(g: &str) -> bool {
    g.split(", ").all(|part| {
        !part.is_empty()
            && part.split(' ').all(|w| {
                w.chars().next().is_some_and(char::is_uppercase) && w.chars().all(char::is_alphabetic)
            })
    }) && !matches!(
        g,
        "Hack" | "Translated" | "Unl" | "Beta" | "Proto" | "Demo" | "Sample"
    )
}

/// "Rev 1", "Rev A", "v1.1", "V1.0".
fn version_of(g: &str) -> Option<String> {
    let g = g.trim();
    let rest = g.strip_prefix("Rev ").or_else(|| g.strip_prefix(['v', 'V']))?;
    (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')).then(|| g.to_owned())
}

impl Name {
    /// Not a retail release (beta, prototype, demo, ...).
    pub fn is_retail(&self) -> bool {
        self.status.is_none()
    }

    /// Revision numbers for ordering ("Rev 2" > "Rev 1" > none; "v1.1" >
    /// "v1.0"); letters count as their place in the alphabet.
    pub fn revision(&self) -> Vec<u64> {
        let Some(v) = &self.version else {
            return Vec::new();
        };
        let v = v
            .strip_prefix("Rev ")
            .or_else(|| v.strip_prefix(['v', 'V']))
            .unwrap_or(v);
        v.split('.')
            .map(|p| match p.parse::<u64>() {
                Ok(n) => n,
                Err(_) => p
                    .bytes()
                    .next()
                    .filter(u8::is_ascii_alphabetic)
                    .map_or(0, |b| (b.to_ascii_uppercase() - b'A' + 1) as u64),
            })
            .collect()
    }
}
