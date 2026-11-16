//! The library as a read-only summary, for uQ Settings and scripts
//! (`overview --json`, `games --json`). It only reads: nothing here scans,
//! downloads or writes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::view::{GameRow, LibraryView, RowStatus, SortBy, sort_rows};

/// How many "recently added" games `overview` lists at most.
pub const RECENT_MAX: usize = 12;

/// Which rows `games` lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    All,
    /// Every data ROM matched by hash.
    Verified,
    /// Matched by hash, but the game isn't complete.
    Incomplete,
    /// Not matched to a game yet (BIOS files are not games and never listed).
    Unidentified,
}

impl StatusFilter {
    pub fn parse(s: &str) -> Option<StatusFilter> {
        match s.to_ascii_lowercase().as_str() {
            "all" => Some(StatusFilter::All),
            "verified" => Some(StatusFilter::Verified),
            "incomplete" => Some(StatusFilter::Incomplete),
            "unidentified" => Some(StatusFilter::Unidentified),
            _ => None,
        }
    }

    fn keeps(self, r: &GameRow) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Verified => r.status == RowStatus::Verified,
            StatusFilter::Incomplete => r.status == RowStatus::Incomplete,
            StatusFilter::Unidentified => r.unmatched,
        }
    }
}

/// What `games` takes.
#[derive(Debug, Clone)]
pub struct GamesQuery {
    /// A platform key (any case); `None`: every platform.
    pub platform: Option<String>,
    pub status: StatusFilter,
    /// Words the title must contain (any case, accents ignored).
    pub search: Option<String>,
    pub sort: SortBy,
    pub descending: bool,
    pub limit: usize,
    pub offset: usize,
}

impl Default for GamesQuery {
    fn default() -> Self {
        GamesQuery {
            platform: None,
            status: StatusFilter::All,
            search: None,
            sort: SortBy::Title,
            descending: false,
            limit: 200,
            offset: 0,
        }
    }
}

/// One game (its original: versions are counted, not listed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameEntry {
    pub id: String,
    /// The name to show: an arcade set's description, else the title.
    pub title: String,
    pub system: Option<String>,
    pub system_name: Option<String>,
    pub status: RowStatus,
    pub year: Option<i32>,
    /// Versions of the game (regions, revisions, translations, hacks).
    pub versions: usize,
    pub files: usize,
    pub size: u64,
    /// Identified, so Play may start it.
    pub playable: bool,
    /// Details (and art) were gathered for it.
    pub metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Games {
    /// Games matching the filters, before `limit` and `offset`.
    pub total: usize,
    pub games: Vec<GameEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Totals {
    /// Games (originals), not counting BIOS files.
    pub games: usize,
    pub verified: usize,
    pub incomplete: usize,
    /// Files nothing has matched to a game.
    pub unidentified: usize,
    /// BIOS files found among the scanned files.
    pub bios_files: usize,
    pub files: i64,
    /// Bytes of the games' files.
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformCount {
    pub system: String,
    pub name: String,
    pub short: String,
    pub manufacturer: String,
    pub games: usize,
    pub verified: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recent {
    #[serde(flatten)]
    pub game: GameEntry,
    /// When its newest file was scanned (UTC, as the database keeps it).
    pub added: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BiosGap {
    pub system: String,
    pub name: String,
    /// The emulator Play would start for it.
    pub emulator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListGap {
    pub system: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lists {
    /// DATs in the library.
    pub dats: usize,
    /// Systems you have games for that have no DAT yet.
    pub missing: Vec<ListGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Overview {
    /// The RomForge folder, when there is one.
    pub home: Option<String>,
    /// The Source and Library folders of Settings.
    pub source: Option<String>,
    pub library: Option<String>,
    pub totals: Totals,
    pub platforms: Vec<PlatformCount>,
    pub recent: Vec<Recent>,
    /// Systems whose emulator lacks a required BIOS file.
    pub bios: Vec<BiosGap>,
    pub lists: Lists,
}

fn entry(r: &GameRow) -> GameEntry {
    let title = match (&r.label, r.name.title.is_empty()) {
        (Some(l), _) => l.clone(),
        (None, false) => r.name.title.clone(),
        (None, true) => r.title.clone(),
    };
    GameEntry {
        id: r.id.clone(),
        title,
        system: r.system.clone(),
        system_name: r.system_name.clone(),
        status: r.status,
        year: r.year,
        versions: r.versions,
        files: r.files,
        size: r.size,
        playable: r.playable,
        metadata: r.metadata,
    }
}

fn folded(s: &str) -> String {
    crate::extdb::fold_accents(s).to_lowercase()
}

/// The games of `view` that `q` asks for.
pub fn games(view: &LibraryView, q: &GamesQuery) -> Games {
    let mut rows: Vec<GameRow> = view
        .main_rows(q.platform.as_deref())
        .filter(|r| {
            !matches!(
                r.status,
                RowStatus::Firmware | RowStatus::Patch | RowStatus::Missing
            ) && q.status.keeps(r)
        })
        .cloned()
        .collect();
    if let Some(words) = q.search.as_deref().map(folded).filter(|w| !w.trim().is_empty()) {
        let words: Vec<&str> = words.split_whitespace().collect();
        rows.retain(|r| {
            let t = folded(&entry(r).title);
            words.iter().all(|w| t.contains(w))
        });
    }
    sort_rows(&mut rows, q.sort, q.descending);
    Games {
        total: rows.len(),
        games: rows.iter().skip(q.offset).take(q.limit).map(entry).collect(),
    }
}

/// Everything `overview` shows, from the pieces the library keeps.
pub(crate) fn overview(
    view: &LibraryView,
    newest: &HashMap<i64, String>,
    files: i64,
    folders: [Option<String>; 3],
    bios: Vec<BiosGap>,
    dats: usize,
    missing: Vec<ListGap>,
) -> Overview {
    let inner = view.inner();
    let mut by_platform: BTreeMap<&str, PlatformCount> = BTreeMap::new();
    let mut totals = Totals {
        games: 0,
        verified: 0,
        incomplete: 0,
        unidentified: 0,
        bios_files: 0,
        files,
        size: 0,
    };
    let mut recent: Vec<Recent> = Vec::new();
    for (r, (_, file_ids, _)) in view.rows.iter().zip(&inner) {
        match r.status {
            RowStatus::Firmware => {
                totals.bios_files += 1;
                continue;
            }
            RowStatus::Patch | RowStatus::Missing => continue,
            _ => {}
        }
        if r.unmatched {
            totals.unidentified += 1;
            continue;
        }
        if !r.main {
            continue;
        }
        totals.games += 1;
        totals.size += r.size;
        match r.status {
            RowStatus::Verified => totals.verified += 1,
            RowStatus::Incomplete => totals.incomplete += 1,
            _ => {}
        }
        if let Some(key) = r.system.as_deref() {
            let p = by_platform.entry(key).or_insert_with(|| {
                let sys = crate::systems::by_key(key);
                PlatformCount {
                    system: key.to_owned(),
                    name: r.system_name.clone().unwrap_or_else(|| key.to_owned()),
                    short: crate::view::short_name(key),
                    manufacturer: sys.map(|s| s.manufacturer.to_owned()).unwrap_or_default(),
                    games: 0,
                    verified: 0,
                }
            });
            p.games += 1;
            p.verified += usize::from(r.status == RowStatus::Verified);
        }
        if let Some(added) = file_ids.iter().filter_map(|i| newest.get(i)).max() {
            recent.push(Recent {
                game: entry(r),
                added: added.clone(),
            });
        }
    }
    recent.sort_by(|a, b| {
        b.added
            .cmp(&a.added)
            .then_with(|| a.game.title.cmp(&b.game.title))
    });
    recent.truncate(RECENT_MAX);
    let mut platforms: Vec<PlatformCount> = by_platform.into_values().collect();
    platforms.sort_by(|a, b| a.name.cmp(&b.name));
    let [home, source, library] = folders;
    Overview {
        home,
        source,
        library,
        totals,
        platforms,
        recent,
        bios,
        lists: Lists { dats, missing },
    }
}

/// The systems of `have` that `with_dats` doesn't hold, named.
pub(crate) fn gaps(have: &BTreeSet<&'static str>, with_dats: &BTreeSet<String>) -> Vec<ListGap> {
    let mut out: Vec<ListGap> = have
        .iter()
        .filter(|k| !with_dats.contains(**k))
        .filter_map(|k| crate::systems::by_key(k))
        .map(|s| ListGap {
            system: s.key.to_owned(),
            name: s.name.to_owned(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
