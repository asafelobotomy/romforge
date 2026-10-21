//! Milestone 9, first stage: Wikidata (free, no account, CC0 data).
//!
//! For the games you have, Wikidata's own search (run inside the SPARQL
//! service, 20 titles per query) finds the video games on that platform
//! with those words; each comes back with its labels, aliases and release
//! dates (English, or the language-neutral "mul" labels Wikidata uses for
//! names that are the same everywhere, such as "Super Mario World"). What
//! was found and which titles were searched are kept per platform, so a
//! run stopped by a rate limit resumes where it stopped. A game is matched
//! when exactly one candidate has the same title (case, punctuation, region
//! tags and a trailing ", The" ignored), narrowed by release year when
//! several do. Ambiguous and unmatched games are recorded as such for the
//! Search queue (milestone 8) to settle; nothing is guessed.
//!
//! For matched games: developer, publisher, country of origin, director,
//! composer, designer, genre, game modes, the series and its ordinal, IDs
//! for MobyGames, IGDB, HowLongToBeat and RetroAchievements, and the English
//! Wikipedia article. Then every member of those series, in order, for the
//! Series tab.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Error, fetch, systems};

mod facts;
mod images;
mod matching;
mod run;
mod series;
#[cfg(test)]
mod tests;

use facts::*;
pub use images::*;
pub use matching::*;
pub use run::*;
use series::*;

pub const ENDPOINT: &str = "https://query.wikidata.org/sparql";

/// Item-valued properties stored by label, and their `game_meta` field.
pub const LABEL_FIELDS: &[(&str, &str)] = &[
    ("P178", "developer"),
    ("P123", "publisher"),
    ("P495", "country"),
    ("P57", "director"),
    ("P86", "composer"),
    ("P287", "designer"),
    ("P136", "genre"),
    ("P404", "game_mode"),
];

/// Label fields naming who made a game (studios and people), kept with
/// their Wikidata items in `credits` for the Development tab's pictures.
pub const CREDIT_FIELDS: &[&str] = &["developer", "publisher", "director", "designer", "composer"];

/// External-ID properties and their `game_meta` field.
pub const ID_FIELDS: &[(&str, &str)] = &[
    ("P11688", "mobygames_id"),
    ("P1933", "mobygames_slug"),
    ("P5794", "igdb_slug"),
    ("P2816", "hltb_id"),
    ("P11393", "retroachievements_id"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct WikidataReport {
    pub games: usize,
    pub matched: usize,
    pub ambiguous: usize,
    pub unmatched: usize,
    pub errors: Vec<String>,
}

/// A video game on one platform, as listed for matching.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    pub qid: String,
    pub label: String,
    pub years: BTreeSet<i32>,
    #[serde(skip)]
    pub names: BTreeSet<String>,
    /// For an edition (Q60997816), the game it is an edition of (P629).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
}

/// Wikidata's classes for a game: video game, and video game edition (a
/// "Director's Cut" or "Greatest Hits" release with an item of its own).
pub(crate) const GAME: &str = "Q7889";
pub(crate) const EDITION: &str = "Q60997816";
