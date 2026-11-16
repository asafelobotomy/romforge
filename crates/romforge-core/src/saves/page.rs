//! Milestone 27, stage 2 in the app: the Saves page (one tab per system,
//! one card per game holding its in-game saves and its save states, linked
//! to the game's row when matched to one) and the line in a game's
//! Details. Built from [`super::index`]'s and [`super::states`]' lists.

use std::collections::BTreeMap;

use super::index::{Save, content_stem};
use super::states::State;
use crate::{systems, view};

/// One game's saves and states on the Saves page. Files matched to no
/// game share a card when they are named after the same content.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GameCard {
    /// The game's title, else the content name its files share.
    pub name: String,
    /// The game they were matched to.
    pub game: Option<String>,
    /// The library row of that game: set only when exactly one row of the
    /// system has the title.
    pub row: Option<String>,
    /// That row's box art (a file in the art cache).
    pub cover: Option<String>,
    /// That row's box front, width / height in hundredths (0 without a
    /// row): the card draws the cover in the shape of its box.
    pub box_aspect: u16,
    pub saves: Vec<Save>,
    pub states: Vec<State>,
    /// The newest file's modification time, in seconds since 1970.
    pub latest: u64,
}

/// One system's tab on the Saves page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SystemSaves {
    /// Its folder in `Saves/` (a system key, else the folder's own name).
    pub system: String,
    /// The system's name ("Nintendo Game Boy"), else the folder's.
    pub name: String,
    /// Newest first.
    pub cards: Vec<GameCard>,
    pub saves: usize,
    pub states: usize,
    /// States still in the old `States/` folder.
    pub old_states: usize,
}

/// A game's saves and states, for its Details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct GameSaves {
    pub count: usize,
    pub states: usize,
    /// The newest one's modification time, in seconds since 1970 (0 when
    /// unknown or none).
    pub latest: u64,
}

/// The row a game links to: its id, box art and box shape.
#[derive(Debug, Clone)]
struct Linked {
    id: String,
    cover: Option<String>,
    box_aspect: u16,
}

type Rows = BTreeMap<(String, String), Option<Linked>>;
/// system (lower case) -> (its folder's name, card key -> card).
type Tabs = BTreeMap<String, (String, BTreeMap<(bool, String), GameCard>)>;

/// (system key, title) -> its row, when exactly one row has them.
fn rows_by_title(v: &view::LibraryView) -> Rows {
    let mut out: Rows = BTreeMap::new();
    for r in &v.rows {
        let Some(system) = &r.system else { continue };
        out.entry((system.to_ascii_lowercase(), r.title.clone()))
            .and_modify(|seen| *seen = None)
            .or_insert_with(|| {
                Some(Linked {
                    id: r.id.clone(),
                    cover: r.pictures.get("boxart").cloned(),
                    box_aspect: r.box_aspect,
                })
            });
    }
    out
}

/// The card `s` goes on (made when new): its game's, else the one of the
/// content it is named after.
fn card<'a>(tabs: &'a mut Tabs, rows: &Rows, s: &Save) -> &'a mut GameCard {
    let (_, cards) = tabs
        .entry(s.system.to_lowercase())
        .or_insert_with(|| (s.system.clone(), BTreeMap::new()));
    let key = match &s.game {
        Some(g) => (true, g.clone()),
        None => (false, content_stem(&s.file).unwrap_or_default()),
    };
    cards.entry(key).or_insert_with_key(|(matched, name)| {
        let linked = matched
            .then(|| rows.get(&(s.system.to_ascii_lowercase(), name.clone())))
            .flatten()
            .cloned()
            .flatten();
        GameCard {
            name: name.clone(),
            game: s.game.clone(),
            row: linked.as_ref().map(|l| l.id.clone()),
            cover: linked.as_ref().and_then(|l| l.cover.clone()),
            box_aspect: linked.as_ref().map_or(0, |l| l.box_aspect),
            saves: Vec::new(),
            states: Vec::new(),
            latest: 0,
        }
    })
}

/// Group `saves` and `states` into tabs by system, cards by game.
pub fn by_system(saves: Vec<Save>, states: Vec<State>, v: &view::LibraryView) -> Vec<SystemSaves> {
    let rows = rows_by_title(v);
    let mut tabs = Tabs::new();
    for s in saves {
        let c = card(&mut tabs, &rows, &s);
        c.latest = c.latest.max(s.modified);
        c.saves.push(s);
    }
    for st in states {
        let c = card(&mut tabs, &rows, &st.save);
        c.latest = c.latest.max(st.save.modified);
        c.states.push(st);
    }
    let mut out: Vec<SystemSaves> = tabs
        .into_values()
        .map(|(system, cards)| {
            let mut cards: Vec<GameCard> = cards.into_values().collect();
            cards.sort_by(|a, b| b.latest.cmp(&a.latest).then_with(|| a.name.cmp(&b.name)));
            let count = |f: &dyn Fn(&GameCard) -> usize| cards.iter().map(f).sum::<usize>();
            SystemSaves {
                name: systems::by_key(&system).map_or_else(|| system.clone(), |s| s.name.to_owned()),
                saves: count(&|c| c.saves.len()),
                states: count(&|c| c.states.len()),
                old_states: count(&|c| c.states.iter().filter(|s| s.old_place).count()),
                system,
                cards,
            }
        })
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

/// The saves and states of `title` on `system`.
pub fn of_game(saves: &[Save], states: &[State], system: &str, title: &str) -> GameSaves {
    let mine = |s: &&Save| s.system.eq_ignore_ascii_case(system) && s.game.as_deref() == Some(title);
    let saves: Vec<&Save> = saves.iter().filter(mine).collect();
    let states: Vec<&Save> = states.iter().map(|s| &s.save).filter(mine).collect();
    GameSaves {
        count: saves.len(),
        states: states.len(),
        latest: saves.iter().chain(&states).map(|s| s.modified).max().unwrap_or(0),
    }
}
