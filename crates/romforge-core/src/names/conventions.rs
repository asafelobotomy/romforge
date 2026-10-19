//! Names that follow No-Intro (and Redump), TOSEC or GoodTools.

use shiratsu_naming::naming::TokenizedName;
use shiratsu_naming::naming::goodtools::{GoodToolsName, GoodToolsToken};
use shiratsu_naming::naming::nointro::{NoIntroName, NoIntroToken};
use shiratsu_naming::naming::tosec::{TOSECLanguage, TOSECName, TOSECToken};
use shiratsu_naming::region::Region;

use super::*;

pub(super) fn codes(regions: &[Region]) -> Vec<String> {
    regions.iter().map(|r| r.as_ref().to_owned()).collect()
}

pub(super) fn from_nointro(n: &NoIntroName) -> Name {
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

pub(super) fn from_tosec(n: &TOSECName) -> Name {
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

pub(super) fn from_goodtools(n: &GoodToolsName) -> Name {
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
