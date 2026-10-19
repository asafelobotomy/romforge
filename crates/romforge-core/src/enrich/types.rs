use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtKind {
    Boxart,
    Title,
    Snap,
    /// A wide banner (hero) image.
    Hero,
    Logo,
    Icon,
    /// A 3D render of the box.
    Box3d,
}

impl ArtKind {
    pub const ALL: [ArtKind; 7] = [
        ArtKind::Boxart,
        ArtKind::Title,
        ArtKind::Snap,
        ArtKind::Hero,
        ArtKind::Logo,
        ArtKind::Icon,
        ArtKind::Box3d,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ArtKind::Boxart => "boxart",
            ArtKind::Title => "title",
            ArtKind::Snap => "snap",
            ArtKind::Hero => "hero",
            ArtKind::Logo => "logo",
            ArtKind::Icon => "icon",
            ArtKind::Box3d => "box3d",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
    }

    /// libretro's thumbnail folder; it has only these three kinds.
    pub(super) fn folder(self) -> Option<&'static str> {
        match self {
            ArtKind::Boxart => Some("Named_Boxarts"),
            ArtKind::Title => Some("Named_Titles"),
            ArtKind::Snap => Some("Named_Snaps"),
            _ => None,
        }
    }
}

/// One image a source has for a game: the best first among a source's
/// images of a kind. Enrich keeps the first that downloads; the art picker
/// offers them all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArtFound {
    pub kind: ArtKind,
    pub url: String,
    /// A smaller copy to preview, where the source has one.
    pub thumb: Option<String>,
    /// What tells it apart: its region, size or style.
    pub label: Option<String>,
}

impl ArtFound {
    pub fn new(kind: ArtKind, url: String) -> Self {
        ArtFound {
            kind,
            url,
            thumb: None,
            label: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichOptions {
    /// Where art is saved (`<cache>/art/<system>/<kind>/<game>.png`).
    pub cache_dir: PathBuf,
    pub art: Vec<ArtKind>,
    /// Only this system key.
    pub system: Option<String>,
    /// Only the game with this title (with `system`, one game: Search
    /// fetches what you asked for right after you pick a result).
    pub title: Option<String>,
    /// Fetch again what was fetched (or found missing) before.
    pub refresh: bool,
    /// libretro-database's `metadat` URL ([`fetch::LIBRETRO_BASE`]).
    pub metadata_base: String,
    /// libretro-database's own DATs ([`fetch::LIBRETRO_DAT_BASE`]): DOS,
    /// ScummVM and PC-98 metadata.
    pub dats_base: String,
    /// The thumbnail server ([`THUMBNAILS_BASE`]).
    pub thumbnails_base: String,
    /// Wikidata's SPARQL endpoint ([`crate::wikidata::ENDPOINT`]); `None`
    /// skips Wikidata.
    pub wikidata: Option<String>,
    /// Where the keyed art sources live.
    pub sources: crate::artsources::Bases,
    /// The folder of databases the user downloaded (OpenVGDB, LaunchBox);
    /// `None`: the library's own (`databases/` next to it), if any.
    pub extdb: Option<PathBuf>,
    /// LaunchBox's image server ([`crate::extdb::Bases`]).
    pub launchbox_images: String,
    /// Where art is saved (`<dir>/<system>/<kind>/<game>.png`); `None`:
    /// the RomForge folder's `Media/` if the library has one, else
    /// `<cache>/art`.
    pub art_dir: Option<PathBuf>,
}

impl EnrichOptions {
    /// Box art only, from libretro, saved under `cache_dir`.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            art: vec![ArtKind::Boxart],
            system: None,
            title: None,
            refresh: false,
            metadata_base: fetch::LIBRETRO_BASE.to_owned(),
            dats_base: fetch::LIBRETRO_DAT_BASE.to_owned(),
            thumbnails_base: THUMBNAILS_BASE.to_owned(),
            wikidata: Some(crate::wikidata::ENDPOINT.to_owned()),
            sources: crate::artsources::Bases::default(),
            extdb: None,
            art_dir: None,
            launchbox_images: crate::extdb::Bases::default().launchbox_images,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct EnrichReport {
    pub systems: usize,
    pub games: usize,
    pub games_with_metadata: usize,
    pub art_saved: usize,
    pub art_missing: usize,
    pub art_cached: usize,
    /// Wikidata matching (when it ran).
    pub wikidata: Option<crate::wikidata::WikidataReport>,
    /// Games given fields by the downloaded databases (OpenVGDB, LaunchBox).
    pub extdb_games: usize,
    pub errors: Vec<String>,
}

/// What [`enrich`] is doing, for progress output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichEvent {
    /// Wikidata: a platform being matched, or "details" / "series".
    Wikidata { step: &'static str },
    Metadata {
        system: &'static str,
        field: &'static str,
    },
    Art {
        system: &'static str,
        game: String,
        kind: ArtKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GameInfo {
    pub system: String,
    pub title: String,
    pub fields: BTreeMap<String, String>,
    pub art: BTreeMap<ArtKind, PathBuf>,
}

/// `$ROMFORGE_CACHE`, else `$XDG_CACHE_HOME/romforge`, else
/// `~/.cache/romforge`.
pub fn default_cache_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("ROMFORGE_CACHE") {
        return PathBuf::from(p);
    }
    crate::layout::xdg("XDG_CACHE_HOME", ".cache").unwrap_or_else(|| PathBuf::from(crate::layout::APP_DIR))
}
