//! Patch rows, and the empty row they (and missing originals) start from.

use std::collections::BTreeMap;

use rusqlite::Connection;

use super::{GameRow, Relation, RowStatus};
use crate::{Error, systems};

/// A row with nothing in it yet.
pub(super) fn bare_row(id: String, system_id: Option<i64>, title: &str, status: RowStatus) -> GameRow {
    let sys = system_id.and_then(systems::by_id);
    GameRow {
        id,
        title: title.to_owned(),
        system: sys.map(|s| s.key.to_owned()),
        system_name: sys.map(|s| s.name.to_owned()),
        status,
        cue_differs: false,
        cue_fits: false,
        files: 0,
        size: 0,
        metadata: false,
        art: Vec::new(),
        playable: false,
        group: None,
        main: true,
        versions: 1,
        folder: None,
        label: None,
        category: None,
        year: None,
        pictures: BTreeMap::new(),
        box_aspect: 72,
        packed: false,
        likely_system: None,
        unmatched: false,
        base_id: None,
        system_id,
        file_ids: Vec::new(),
        patch_of: None,
    }
}

/// The patch files a scan found, a row each: under their game when paired.
pub(super) fn patch_rows(conn: &Connection) -> Result<Vec<GameRow>, Error> {
    let mut out = Vec::new();
    for r in conn
        .prepare("SELECT path, name, size, relation, system_id, title FROM patch_files ORDER BY name, path")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?
    {
        let (path, name, size, relation, system_id, title) = r?;
        let mut row = bare_row(
            format!("p:{path}"),
            system_id.filter(|_| title.is_some()),
            &name,
            RowStatus::Patch,
        );
        let file = path.rsplit(['/', '\\']).next().unwrap_or(&path);
        if !file.eq_ignore_ascii_case(&name) {
            row.label = Some(format!("{name} ({file})"));
        }
        row.files = 1;
        row.size = size;
        let relation = Relation::parse(&relation).unwrap_or(Relation::Hack);
        row.patch_of = title.map(|t| (t, relation));
        out.push(row);
    }
    Ok(out)
}
