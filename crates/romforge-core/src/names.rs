//! What a name says: title, regions, languages, version, release status,
//! disc, and dump / hack / translation flags (milestone 12.4).
//!
//! DAT names and well-kept files follow No-Intro, Redump (No-Intro's
//! form), TOSEC or GoodTools naming, parsed by `shiratsu-naming`. Anything
//! else ("Dragon Quest III (English v2.0)[Addendum]", "zelda3") gets a
//! loose reading: the text before the first bracket is the title, and the
//! groups are looked at for translation and hack markers. A name is never
//! evidence of a match by itself (house rule 8).

use shiratsu_naming::naming::TokenizedName;
use shiratsu_naming::naming::goodtools::{GoodToolsName, GoodToolsToken};
use shiratsu_naming::naming::nointro::{NoIntroName, NoIntroToken};
use shiratsu_naming::naming::tosec::{TOSECLanguage, TOSECName, TOSECToken};
use shiratsu_naming::region::Region;

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

fn codes(regions: &[Region]) -> Vec<String> {
    regions.iter().map(|r| r.as_ref().to_owned()).collect()
}

fn from_nointro(n: &NoIntroName) -> Name {
    let mut out = Name {
        convention: Convention::NoIntro,
        ..Name::default()
    };
    for t in n.iter() {
        match t {
            NoIntroToken::Title(s) => out.title = (*s).to_owned(),
            NoIntroToken::Region {
                region_strings,
                regions,
            } => {
                out.regions = region_strings.iter().map(|s| (*s).to_owned()).collect();
                out.region_codes = codes(regions);
            }
            NoIntroToken::Languages(l) => out.languages = l.iter().map(|l| l.code.to_owned()).collect(),
            NoIntroToken::Version(v) => {
                out.version = v.first().map(|v| match v.minor {
                    Some(m) => format!("{} {}.{m}", v.version_type, v.major),
                    None if v.version_type == "v" => format!("v{}", v.major),
                    None => format!("{} {}", v.version_type, v.major),
                })
            }
            NoIntroToken::Release { status, number } => {
                out.status = Some(match number {
                    Some(n) => format!("{status} {n}"),
                    None => (*status).to_owned(),
                })
            }
            NoIntroToken::Media { number, .. } => out.disc = Some((*number).to_owned()),
            NoIntroToken::Flag { flag, .. } => flag_words(&mut out, flag),
            NoIntroToken::Scene { .. } => {}
        }
    }
    out
}

fn from_tosec(n: &TOSECName) -> Name {
    let mut out = Name {
        convention: Convention::Tosec,
        ..Name::default()
    };
    for t in n.iter() {
        match t {
            TOSECToken::Title(s) => out.title = (*s).to_owned(),
            TOSECToken::Region {
                region_strings,
                regions,
            } => {
                out.regions = region_strings.iter().map(|s| (*s).to_owned()).collect();
                out.region_codes = codes(regions);
            }
            TOSECToken::Languages(TOSECLanguage::Single(l)) => out.languages = vec![(*l).to_owned()],
            TOSECToken::Languages(TOSECLanguage::Double(a, b)) => {
                out.languages = vec![(*a).to_owned(), (*b).to_owned()]
            }
            TOSECToken::Version {
                version_type,
                major,
                minor,
            } => {
                out.version = Some(match minor {
                    Some(m) => format!("{version_type}{major}.{m}"),
                    None => format!("{version_type}{major}"),
                })
            }
            TOSECToken::Demo(_) => out.status = Some("Demo".into()),
            TOSECToken::Development(d) => out.status = Some((*d).to_owned()),
            TOSECToken::Media(m) => out.disc = m.first().map(|m| m.number.to_owned()),
            TOSECToken::DumpInfo { code, number, info } => {
                let tag = || {
                    [Some(*code), *number, *info]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                match *code {
                    "b" => out.bad_dump = true,
                    "!" => out.verified_dump = true,
                    "a" => out.alternate = true,
                    "h" => out.hack = Some(tag()),
                    "tr" | "t" => out.translation = Some(info.unwrap_or(code).to_owned()),
                    "p" => out.status = Some("Pirate".into()),
                    _ => {}
                }
            }
            TOSECToken::Flag { flag, .. } => flag_words(&mut out, flag),
            _ => {}
        }
    }
    out
}

fn from_goodtools(n: &GoodToolsName) -> Name {
    let mut out = Name {
        convention: Convention::GoodTools,
        ..Name::default()
    };
    for t in n.iter() {
        match t {
            GoodToolsToken::Title(s) => out.title = (*s).to_owned(),
            GoodToolsToken::Region {
                region_strings,
                regions,
            } => {
                out.regions = region_strings.iter().map(|s| (*s).to_owned()).collect();
                out.region_codes = codes(regions);
            }
            GoodToolsToken::Translation { tags, .. } => out.translation = Some((*tags).to_owned()),
            GoodToolsToken::Version { prefix, major, minor } => {
                out.version = Some(match minor {
                    Some(m) => format!("{prefix}{major}.{m}"),
                    None => format!("{prefix}{major}"),
                })
            }
            GoodToolsToken::GameHack(h) => out.hack = Some(h.unwrap_or("Hack").to_owned()),
            GoodToolsToken::Media { number, .. } => out.disc = Some((*number).to_owned()),
            GoodToolsToken::DumpCode {
                code,
                number,
                code_type,
                ..
            } => match *code {
                "b" => out.bad_dump = true,
                "!" => out.verified_dump = true,
                "a" => out.alternate = true,
                "h" => {
                    out.hack = Some(
                        [Some("h"), *number, *code_type]
                            .into_iter()
                            .flatten()
                            .collect::<String>(),
                    )
                }
                "p" => out.status = Some("Pirate".into()),
                _ => {}
            },
            GoodToolsToken::Flag { flag, .. } => flag_words(&mut out, flag),
            _ => {}
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nointro_and_redump() {
        let n = parse("Legend of Zelda, The - A Link to the Past (USA).sfc");
        assert_eq!(n.convention, Convention::NoIntro);
        assert_eq!(n.title, "Legend of Zelda, The - A Link to the Past");
        assert_eq!(n.regions, ["USA"]);
        assert_eq!(n.region_codes, ["US"]);

        let n = parse("Streets of Rage (World) (En,Ja)");
        assert_eq!(n.regions, ["World"]);
        assert_eq!(n.languages, ["En", "Ja"]);

        let n = parse("Super Mario World (USA) (Rev 1) [b]");
        assert_eq!(n.version.as_deref(), Some("Rev 1"));
        assert_eq!(n.revision(), [1]);
        assert!(n.bad_dump);

        let n = parse("Final Fantasy VII (USA) (Disc 2)");
        assert_eq!(n.disc.as_deref(), Some("2"));

        let n = parse("Silent Hill (USA) (Beta) (1998-12-01)");
        assert!(!n.is_retail(), "{n:?}");
        assert!(!parse("101 Dalmatas (USA) (Pirate)").is_retail());
        let n = parse("16 BIT XMAS 2011 - Christmas Craze (World) (Aftermarket) (Unl)");
        assert!(n.is_retail() && n.unlicensed, "{n:?}");
    }

    #[test]
    fn goodtools_and_tosec() {
        let n = parse("Dragon Quest III (J) [T+Eng2.0_Addendum].smc");
        assert_eq!(n.convention, Convention::GoodTools);
        assert_eq!(n.title, "Dragon Quest III");
        assert_eq!(n.region_codes, ["JP"]);
        assert_eq!(n.translation.as_deref(), Some("Eng2.0_Addendum"));

        let n = parse("Street Fighter II (E) [h1C]");
        assert_eq!(n.hack.as_deref(), Some("h1C"));

        let n = parse("Super Mario World (U) [!]");
        assert!(n.verified_dump);
        assert_eq!(n.region_codes, ["US"]);

        let n = parse("Dragon Quest III (1996)(Enix)(JP)[tr en Addendum]");
        assert_eq!(n.convention, Convention::Tosec);
        assert_eq!(n.title, "Dragon Quest III");
        assert_eq!(n.translation.as_deref(), Some("en Addendum"));
    }

    #[test]
    fn loose_names() {
        let n = parse("Dragon Quest III (English v2.0)[Addendum].sfc");
        assert_eq!(n.convention, Convention::Loose);
        assert_eq!(n.title, "Dragon Quest III");
        assert_eq!(n.translation.as_deref(), Some("English v2.0 Addendum"));

        let n = parse("Super Metroid (Hack) (Redesign)");
        assert_eq!(n.title, "Super Metroid");
        assert!(n.hack.is_some());

        let n = parse("Post Mortem (Ukraine) (Disc 2)");
        assert_eq!(n.regions, ["Ukraine"], "{n:?}");
        assert_eq!(n.disc.as_deref(), Some("2"));
        assert_eq!(parse("ps-22j(v)").title, "ps-22j");
        let n = parse("Ancient Chozo [Hack by Albert V.]");
        assert_eq!(
            (n.title.as_str(), n.hack.as_deref()),
            ("Ancient Chozo", Some("Hack by Albert V."))
        );

        let n = parse("zelda3.smc");
        assert_eq!(n.title, "zelda3");
        assert_eq!(n.translation, None);
    }

    #[test]
    fn extensions_only_when_they_look_like_one() {
        assert_eq!(without_extension("Game (USA).sfc"), "Game (USA)");
        assert_eq!(
            without_extension("Final Fantasy VII (USA) (v1.1)"),
            "Final Fantasy VII (USA) (v1.1)"
        );
        assert_eq!(without_extension("Doom 1.9"), "Doom 1.9");
    }
}
