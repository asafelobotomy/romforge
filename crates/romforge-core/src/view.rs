//! The library as the app shows it: one row per game, unidentified files as
//! their own rows, and what has been gathered for each (milestone 6).
//!
//! A row is a game you have complete (house rule 8: every data ROM matched
//! by size + SHA1/MD5), keyed by system and DAT name so the same title in
//! two DATs is one row. Every other file is a row of its own: incomplete,
//! candidate, wrong system or unknown. Members of an archive that holds a
//! game (a cue that differs, a readme) stay with that game's row.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use crate::Error;

mod detail;
mod folders;
mod links;
mod patches;
mod pictures;
mod platforms;
mod rows;
mod snapshot;
mod sort;
#[cfg(test)]
mod tests;
mod versions;

pub use detail::{detail, detail_in};
pub(crate) use folders::is_program;
pub use links::{links, patch_links, shown_value};
pub use platforms::short_name;
pub(crate) use snapshot::RowInner;
pub use snapshot::fingerprint;
pub use sort::{SortBy, search_title, sort_rows};

use patches::patch_rows;
use platforms::finish;
use rows::rows_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    /// Every data ROM matched by hash.
    Verified,
    /// Matched by hash, but the game isn't complete.
    Incomplete,
    /// Only size + CRC32 agree.
    Candidate,
    /// Matched by hash outside the folder's scope: another system, or
    /// another DAT when the folder is scoped to one.
    WrongSystem,
    /// A copy of something another file already provides.
    Duplicate,
    /// No hash match; named by the owner in Search (never verified).
    ByYou,
    /// A translation or hack of a DAT game, named so by the owner (never
    /// verified; see [`GameDetail::derived`]).
    Derivative,
    /// A folder game named by ScummVM's own detection list (file sizes and
    /// MD5s of their first bytes; never counted as verified).
    Detected,
    Unknown,
    /// Listed only ([`listing`]): the original of versions you have
    /// (translations, hacks, patches) when you don't have it.
    Missing,
    /// Listed only ([`listing`]): a patch file a scan found.
    Patch,
}

/// Where a version sits under its game's original in [`listing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Another region or revision of the game.
    Region,
    /// A special edition, re-release, or not a retail release (demo,
    /// beta, prototype).
    Edition,
    /// A translated copy (named so by you or by romforge, never verified).
    Translation,
    /// A hacked copy.
    Hack,
    /// A translation patch file.
    PatchTranslation,
    /// A hack patch file.
    PatchHack,
}

impl Category {
    /// In the order the list shows them under a game's original.
    pub const ORDER: [Category; 6] = [
        Category::Region,
        Category::Edition,
        Category::Translation,
        Category::Hack,
        Category::PatchTranslation,
        Category::PatchHack,
    ];

    /// The heading the list shows it under.
    pub fn heading(self) -> &'static str {
        match self {
            Category::Region => "Other regions",
            Category::Edition => "Editions",
            Category::Translation | Category::PatchTranslation => "Translations",
            Category::Hack | Category::PatchHack => "Hacks",
        }
    }

    /// A patch file, listed under "Patches".
    pub fn is_patch(self) -> bool {
        matches!(self, Category::PatchTranslation | Category::PatchHack)
    }
}

impl RowStatus {
    /// Files nothing has matched to a game yet: unknown, a CRC32 alone
    /// (a candidate), or a match on another platform or DAT. Unidentified
    /// lists them (with patches paired with no game); every other status
    /// is a game.
    pub fn unmatched(self) -> bool {
        matches!(
            self,
            RowStatus::Unknown | RowStatus::Candidate | RowStatus::WrongSystem
        )
    }

    /// Identified (by hash, by you, or by ScummVM's list): it can be
    /// played and imported.
    pub fn identified(self) -> bool {
        matches!(
            self,
            RowStatus::Verified | RowStatus::ByYou | RowStatus::Derivative | RowStatus::Detected
        )
    }
}

fn game_id(system_id: Option<i64>, title: &str) -> String {
    match system_id {
        Some(s) => format!("g:{s}:{title}"),
        None => format!("g:-:{title}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct GameRow {
    /// `g:<system id>:<title>` for games (`g:-:<title>` from a DAT of no
    /// known system), `m:<system id>:<title>` for games you named,
    /// `f:<file id>` for lone files.
    pub id: String,
    pub title: String,
    pub system: Option<String>,
    pub system_name: Option<String>,
    pub status: RowStatus,
    pub cue_differs: bool,
    /// The differing cue sheet still names exactly the game's tracks (a
    /// rewritten sheet, file status `sheet`).
    pub cue_fits: bool,
    pub files: usize,
    pub size: u64,
    pub metadata: bool,
    /// Art kinds cached for the game (`boxart`, `title`, `snap`).
    pub art: Vec<String>,
    /// Identified (by hash, or by the owner), so Play may start it.
    pub playable: bool,
    /// Versions of one game (regions, revisions, translations, hacks)
    /// share a group (`v:<system id>:<key>`); `None` for a game with no
    /// other version here.
    pub group: Option<String>,
    /// The version the list shows for its group (the best region in
    /// Settings); every row not in a group is its own main row.
    pub main: bool,
    /// On a main row: how many versions its group has (1 without one).
    pub versions: usize,
    /// For a game that is a whole folder (DOS, ScummVM): that folder, or
    /// the archive holding it. Import and Play take all of it.
    pub folder: Option<String>,
    /// What to show instead of the title: an arcade set's DAT description
    /// ("Street Fighter II: The World Warrior (World 910522)"); the title
    /// stays its set name.
    pub label: Option<String>,
    /// In [`listing`], on a version: which kind of version it is (`None`
    /// on the original, and on rows not in a group).
    pub category: Option<Category>,
    /// On a translation, hack or patch: the row of the game it is made from
    /// (or for), when you have it.
    pub base_id: Option<String>,
    /// Its release year (a translation, hack or patch: its game's), from
    /// the metadata Enrich gathered.
    pub year: Option<i32>,
    /// Pictures to show it by in the list's picture views: art kind
    /// (`boxart`, `box3d`, `hero`, `logo`) -> file. A translation, hack,
    /// patch or missing original borrows its game's.
    pub pictures: BTreeMap<String, String>,
    /// Its box's front, width / height in hundredths, by platform and
    /// region ([`systems::box_aspect`]): the picture views draw its box art
    /// so.
    pub box_aspect: u16,
    /// Not matched to a game yet ([`RowStatus::unmatched`], or no platform
    /// found at all): listed under Unidentified, not with the games.
    pub unmatched: bool,
    #[serde(skip)]
    pub(crate) system_id: Option<i64>,
    #[serde(skip)]
    file_ids: Vec<i64>,
    /// A patch row's game, and whether it is a translation or a hack.
    #[serde(skip)]
    patch_of: Option<(String, Relation)>,
}

/// What a derivative is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Translation,
    Hack,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Translation => "translation",
            Relation::Hack => "hack",
        }
    }

    pub fn parse(s: &str) -> Option<Relation> {
        match s {
            "translation" => Some(Relation::Translation),
            "hack" => Some(Relation::Hack),
            _ => None,
        }
    }
}

/// A translation or hack of a DAT game (milestone 12.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Derived {
    pub relation: Relation,
    /// The DAT game it is made from.
    pub base: String,
    /// What its name says ("English v2.0 Addendum", "h1C").
    pub tag: Option<String>,
}

impl Derived {
    /// The title it is listed under: the base game's, then the tag
    /// ("Dragon Quest III - ... (Japan) [Translation: English v2.0]").
    pub fn title(&self) -> String {
        let kind = match self.relation {
            Relation::Translation => "Translation",
            Relation::Hack => "Hack",
        };
        match &self.tag {
            Some(t) => format!("{} [{kind}: {t}]", self.base),
            None => format!("{} [{kind}]", self.base),
        }
    }

    /// "translation of Dragon Quest III - ... (Japan)".
    pub fn label(&self) -> String {
        format!("{} of {}", self.relation.as_str(), self.base)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Platform {
    /// `None` for files no system could be found for.
    pub system: Option<String>,
    pub name: String,
    /// A label of up to 5 characters for the platform's icon.
    pub short: String,
    pub manufacturer: Option<String>,
    pub games: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct LibraryView {
    pub platforms: Vec<Platform>,
    pub rows: Vec<GameRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowFile {
    /// `path` or `path#member`.
    pub path: String,
    pub status: String,
    pub size: u64,
    pub sha1: String,
    pub header_kind: Option<String>,
    /// What the file's own header or boot files say ([`crate::ident`]);
    /// evidence, never a match.
    pub game_id: Option<crate::ident::GameId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
    /// The game's own page (from a Wikidata ID), not a search.
    pub direct: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameDetail {
    pub row: GameRow,
    pub fields: BTreeMap<String, String>,
    /// kind -> cached file
    pub art: BTreeMap<String, String>,
    /// kind -> where it came from ("libretro", "screenscraper", …)
    pub art_sources: BTreeMap<String, String>,
    /// field -> the downloaded database it came from ("openvgdb",
    /// "launchbox"); fields from libretro or Wikidata aren't listed.
    pub field_sources: BTreeMap<String, String>,
    pub files: Vec<RowFile>,
    /// DATs listing the game.
    pub dats: Vec<String>,
    pub links: Vec<Link>,
    /// Games of the same franchise romforge has metadata for, oldest first.
    pub series: Vec<SeriesEntry>,
    /// What the row's title (a DAT name, or a file name) says: regions,
    /// languages, version, flags.
    pub name: crate::names::Name,
    /// For a game romforge made by patching one of yours: which patch.
    pub patched: Option<crate::patcher::PatchedFrom>,
    /// Where to look for a translation or hack of it (an identified game).
    pub patch_links: Vec<Link>,
    /// Patch files a scan found for it (an identified game), ready to apply.
    pub patches: Vec<crate::patcher::FoundPatch>,
    /// For [`RowStatus::Derivative`]: what it is made from. Its metadata and
    /// art fall back to the base game's.
    pub derived: Option<Derived>,
    /// For a derivative romforge named itself: the patch beside it that,
    /// applied to your verified copy of the base game, gives exactly it.
    pub proof: Option<String>,
    /// Who made it (Wikidata): studios and people, with their pictures.
    pub credits: Vec<Credit>,
}

/// A studio or person credited for a game, from Wikidata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Credit {
    /// "developer", "publisher", "director", "designer" or "composer".
    pub role: String,
    pub name: String,
    pub qid: String,
    /// The cached picture (a logo or a photo), if one was fetched.
    pub picture: Option<String>,
    /// "logo" or "photo".
    pub kind: Option<String>,
    /// The picture's page on Wikimedia Commons (its author and licence).
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeriesEntry {
    pub title: String,
    /// Platform key when the entry is one of your games.
    pub system: String,
    pub year: Option<String>,
    /// Place in the series (Wikidata's series ordinal), when known.
    pub ordinal: Option<String>,
    /// The game being shown.
    pub current: bool,
    /// A game you have.
    pub in_library: bool,
}

struct FileRec {
    id: i64,
    path: String,
    member: String,
    status: String,
    size: u64,
    sha1: String,
}

pub fn library(conn: &Connection) -> Result<LibraryView, Error> {
    finish(conn, rows_of(conn)?, false)
}

/// The library as the app and `games` list it: [`library`], plus the patch
/// files a scan found, every game's versions under its original by
/// [`Category`], and the original itself ([`RowStatus::Missing`]) when you
/// have only translations, hacks or patches of it.
pub fn listing(conn: &Connection) -> Result<LibraryView, Error> {
    let mut rows = rows_of(conn)?;
    rows.extend(patch_rows(conn)?);
    finish(conn, rows, true)
}
