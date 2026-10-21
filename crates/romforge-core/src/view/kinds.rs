//! Row status, version category, and derivative relation.

use serde::Serialize;

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
    /// verified; see [`super::GameDetail::derived`]).
    Derivative,
    /// A folder game named by ScummVM's own detection list (file sizes and
    /// MD5s of their first bytes; never counted as verified).
    Detected,
    Unknown,
    /// Listed only ([`super::listing`]): the original of versions you have
    /// (translations, hacks, patches) when you don't have it.
    Missing,
    /// Listed only ([`super::listing`]): a patch file a scan found.
    Patch,
}

/// Where a version sits under its game's original in [`super::listing`].
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
