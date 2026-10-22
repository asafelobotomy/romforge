//! A manual's marks: highlighted text and notes on an area of a page (an
//! image, a map), placed as fractions of the page so they hold at any zoom.

use rusqlite::{Connection, params};

use crate::Error;

/// A rectangle on a page, as fractions of its width and height.
pub type Rect = [f64; 4];

/// The colours a mark may have (the UK SNES's buttons, and a plain one).
pub const COLORS: &[&str] = &["yellow", "red", "green", "blue"];

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    /// Selected text.
    Highlight,
    /// A box drawn on the page.
    Area,
}

/// A mark as the reader sends it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct NewMark {
    pub page: u32,
    pub kind: MarkKind,
    #[serde(default)]
    pub text: String,
    pub rects: Vec<Rect>,
    pub color: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Mark {
    pub id: i64,
    pub page: u32,
    pub kind: MarkKind,
    pub text: String,
    pub rects: Vec<Rect>,
    pub color: String,
    pub note: String,
    pub created_at: String,
}

fn kind_str(k: &MarkKind) -> &'static str {
    match k {
        MarkKind::Highlight => "highlight",
        MarkKind::Area => "area",
    }
}

/// Every mark in a game's manual, by page.
pub fn marks(conn: &Connection, system: &str, game: &str) -> Result<Vec<Mark>, Error> {
    let mut stmt = conn.prepare(
        "SELECT mark_id, page, kind, text, rects, color, note, created_at FROM manual_marks
         WHERE system = ?1 AND game = ?2 ORDER BY page, mark_id",
    )?;
    let rows = stmt.query_map(params![system, game], |r| {
        let kind: String = r.get(2)?;
        let rects: String = r.get(4)?;
        Ok(Mark {
            id: r.get(0)?,
            page: r.get::<_, i64>(1)? as u32,
            kind: if kind == "area" {
                MarkKind::Area
            } else {
                MarkKind::Highlight
            },
            text: r.get(3)?,
            rects: serde_json::from_str(&rects).unwrap_or_default(),
            color: r.get(5)?,
            note: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Keep a mark; returns its id.
pub fn add_mark(conn: &Connection, system: &str, game: &str, m: &NewMark) -> Result<i64, Error> {
    let bad = |s: &str| Err(Error::Manual(s.into()));
    if m.page == 0 {
        return bad("a mark needs a page");
    }
    if m.rects.is_empty()
        || m.rects
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(-0.01..=1.01).contains(v))
    {
        return bad("a mark's place is outside its page");
    }
    if !COLORS.contains(&m.color.as_str()) {
        return bad("no such mark colour");
    }
    if m.kind == MarkKind::Highlight && m.text.trim().is_empty() {
        return bad("a highlight needs some text");
    }
    let rects = serde_json::to_string(&m.rects).map_err(|e| Error::Manual(e.to_string()))?;
    conn.execute(
        concat!(
            "INSERT INTO manual_marks (system, game, page, kind, text, rects, color, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ",
            crate::db::now!(),
            ")"
        ),
        params![
            system,
            game,
            m.page,
            kind_str(&m.kind),
            m.text.trim(),
            rects,
            m.color,
            m.note.trim()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Change a mark's note (and colour).
pub fn edit_mark(conn: &Connection, id: i64, note: &str, color: &str) -> Result<(), Error> {
    if !COLORS.contains(&color) {
        return Err(Error::Manual("no such mark colour".into()));
    }
    conn.execute(
        "UPDATE manual_marks SET note = ?2, color = ?3 WHERE mark_id = ?1",
        params![id, note.trim(), color],
    )?;
    Ok(())
}

pub fn remove_mark(conn: &Connection, id: i64) -> Result<(), Error> {
    conn.execute("DELETE FROM manual_marks WHERE mark_id = ?1", [id])?;
    Ok(())
}
