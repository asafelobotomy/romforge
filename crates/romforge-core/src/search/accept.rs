//! Taking the suggestions of several rows at once (the app's "Name as
//! suggested"): each row's strongest suggestion, when it is a name match or
//! better (a "possible" one is never taken unasked), named as the owner
//! would in Search: identified by you, or a translation or hack of the DAT
//! game when the file's name says so. Never verified (house rule 8).

use std::path::Path;

use rusqlite::Connection;

use super::{Need, identify, queue};
use crate::suggest::Band;
use crate::{Error, view};

/// What became of one row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Accepted {
    pub row_id: String,
    /// The row's title (its file name, for an unknown file).
    pub file: String,
    /// What it is named now.
    pub named: Option<String>,
    /// Why it wasn't.
    pub why_not: Option<String>,
}

/// Name each of `ids` by its strongest suggestion. `databases`: the
/// downloaded databases' folder (LaunchBox's names help the suggestions).
/// `progress(done, total)` follows each row; asked to stop, the rows not
/// reached stay as they are (and aren't listed).
pub fn accept(
    conn: &mut Connection,
    v: &view::LibraryView,
    ids: &[String],
    databases: Option<&Path>,
    cancel: &crate::job::Cancel,
    mut progress: impl FnMut(usize, usize),
) -> Result<Vec<Accepted>, Error> {
    let items = queue(conn, v, Some(ids), databases)?;
    let mut out = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        progress(n, ids.len());
        if cancel.is_set() {
            break;
        }
        let file = v
            .rows
            .iter()
            .find(|r| r.id == *id)
            .map(|r| r.title.clone())
            .unwrap_or_default();
        let item = items.iter().find(|q| q.row_id == *id && q.need == Need::Identify);
        let mut done = Accepted {
            row_id: id.clone(),
            file,
            named: None,
            why_not: None,
        };
        match item.and_then(|q| q.suggestion.as_ref()) {
            Some(s) if s.band <= Band::Suggested => {
                let title = s
                    .derived
                    .as_ref()
                    .map_or_else(|| s.title.clone(), view::Derived::title);
                identify(conn, v, id, &s.system, &title, s.derived.as_ref())?;
                done.named = Some(title);
            }
            Some(s) => done.why_not = Some(format!("only possibly {}: pick it in Search", s.title)),
            None if item.is_some() => done.why_not = Some("no suggestion: name it in Search".into()),
            None => done.why_not = Some("already identified".into()),
        }
        out.push(done);
    }
    Ok(out)
}
