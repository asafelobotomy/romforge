//! Milestone 27, stage 2 in the app: the Saves page (each system's saves,
//! each with its game's row when the save was matched to one) and the
//! line in a game's Details. Built from [`super::index`]'s list.

use std::collections::BTreeMap;

use super::index::Save;
use crate::{systems, view};

/// One save on the Saves page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SaveRow {
    #[serde(flatten)]
    pub save: Save,
    /// The library row of its game: set only when the save was matched
    /// to a game and exactly one row of the system has that title.
    pub row: Option<String>,
}

/// One system's tab on the Saves page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SystemSaves {
    /// Its folder in `Saves/` (a system key, else the folder's own name).
    pub system: String,
    /// The system's name ("Nintendo Game Boy"), else the folder's.
    pub name: String,
    pub saves: Vec<SaveRow>,
    /// How many of `saves` were matched to a game.
    pub matched: usize,
}

/// A game's saves, for its Details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct GameSaves {
    pub count: usize,
    /// The newest one's modification time, in seconds since 1970 (0 when
    /// unknown or none).
    pub latest: u64,
}

/// (system key, title) -> its row, when exactly one row has them.
fn rows_by_title(v: &view::LibraryView) -> BTreeMap<(String, String), Option<String>> {
    let mut out: BTreeMap<(String, String), Option<String>> = BTreeMap::new();
    for r in &v.rows {
        let Some(system) = &r.system else { continue };
        out.entry((system.to_ascii_lowercase(), r.title.clone()))
            .and_modify(|seen| *seen = None)
            .or_insert_with(|| Some(r.id.clone()));
    }
    out
}

/// Group `saves` (sorted by system) into tabs, each save with its row.
pub fn by_system(saves: Vec<Save>, v: &view::LibraryView) -> Vec<SystemSaves> {
    let rows = rows_by_title(v);
    let mut out: Vec<SystemSaves> = Vec::new();
    for save in saves {
        let row = save.game.as_ref().and_then(|title| {
            rows.get(&(save.system.to_ascii_lowercase(), title.clone()))
                .cloned()
                .flatten()
        });
        if out.last().is_none_or(|t| t.system != save.system) {
            let name =
                systems::by_key(&save.system).map_or_else(|| save.system.clone(), |s| s.name.to_owned());
            out.push(SystemSaves {
                system: save.system.clone(),
                name,
                saves: Vec::new(),
                matched: 0,
            });
        }
        if let Some(tab) = out.last_mut() {
            tab.matched += usize::from(save.game.is_some());
            tab.saves.push(SaveRow { save, row });
        }
    }
    out
}

/// The saves of `title` on `system` among `saves`.
pub fn of_game(saves: &[Save], system: &str, title: &str) -> GameSaves {
    saves
        .iter()
        .filter(|s| s.system.eq_ignore_ascii_case(system) && s.game.as_deref() == Some(title))
        .fold(GameSaves::default(), |acc, s| GameSaves {
            count: acc.count + 1,
            latest: acc.latest.max(s.modified),
        })
}
