//! OCR for a manual's scanned pages (milestone 26). The recognising runs in
//! the app (Tesseract's WebAssembly build, tesseract.js 7, Apache-2.0),
//! which turns a page's pixels into words; romforge keeps those words per
//! page and language here, and the language files Tesseract reads.
//!
//! Language files come from Tesseract's `tessdata_fast` (Apache-2.0), one
//! language when the user asks for it, into the cache folder (which the
//! app's `art://` scheme serves to the recognising worker). None is
//! shipped.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

const TESSDATA: &str = "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/";

/// The languages offered: Tesseract's code and a name.
pub const OCR_LANGS: &[(&str, &str)] = &[
    ("eng", "English"),
    ("jpn", "Japanese"),
    ("jpn_vert", "Japanese (vertical)"),
    ("fra", "French"),
    ("deu", "German"),
    ("spa", "Spanish"),
    ("ita", "Italian"),
    ("por", "Portuguese"),
    ("nld", "Dutch"),
    ("swe", "Swedish"),
    ("kor", "Korean"),
    ("chi_sim", "Chinese (simplified)"),
    ("chi_tra", "Chinese (traditional)"),
];

/// A recognised word: its text, its box as fractions of the page
/// (left, top, width, height) and how sure Tesseract was (0–100).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrWord {
    pub text: String,
    pub rect: [f64; 4],
    pub conf: f64,
    /// The last word of its line.
    #[serde(default)]
    pub eol: bool,
}

/// Where language files are kept.
pub fn ocr_dir(cache: &Path) -> PathBuf {
    cache.join("ocr")
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OcrLang {
    pub code: &'static str,
    pub name: &'static str,
    pub downloaded: bool,
}

/// The languages offered, and which are here.
pub fn ocr_langs(cache: &Path) -> Vec<OcrLang> {
    let dir = ocr_dir(cache);
    OCR_LANGS
        .iter()
        .map(|(code, name)| OcrLang {
            code,
            name,
            downloaded: dir.join(format!("{code}.traineddata")).is_file(),
        })
        .collect()
}

/// Download a language's file (on the user's asking). Returns its path.
pub fn download_ocr_lang(cache: &Path, code: &str) -> Result<PathBuf, Error> {
    if !OCR_LANGS.iter().any(|(c, _)| *c == code) {
        return Err(Error::Manual(format!("no OCR language {code:?}")));
    }
    let bytes = crate::fetch::request(&crate::fetch::Request {
        max_body: Some(64 << 20),
        ..crate::fetch::Request::get(&format!("{TESSDATA}{code}.traineddata"))
    })?
    .ok_or_else(|| Error::Manual(format!("no Tesseract data for {code}")))?;
    if bytes.len() < 100_000 {
        return Err(Error::Manual(format!("{code}.traineddata came back too small")));
    }
    let path = ocr_dir(cache).join(format!("{code}.traineddata"));
    crate::atomic::write(&path, &bytes)?;
    Ok(path)
}

/// The words recognised on a page, if it has been.
pub fn ocr_page(
    conn: &Connection,
    system: &str,
    game: &str,
    page: u32,
    lang: &str,
) -> Result<Option<Vec<OcrWord>>, Error> {
    let json: Option<String> = conn
        .query_row(
            "SELECT words FROM manual_ocr WHERE system = ?1 AND game = ?2 AND page = ?3 AND lang = ?4",
            params![system, game, page, lang],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json.map(|j| serde_json::from_str(&j).unwrap_or_default()))
}

/// Keep a page's recognised words (an empty list too: a page with no text).
pub fn save_ocr(
    conn: &Connection,
    system: &str,
    game: &str,
    page: u32,
    lang: &str,
    words: &[OcrWord],
) -> Result<(), Error> {
    let bad = words.iter().any(|w| {
        w.rect
            .iter()
            .any(|v| !v.is_finite() || !(-0.01..=1.01).contains(v))
    });
    if page == 0 || bad {
        return Err(Error::Manual("OCR words outside their page".into()));
    }
    let json = serde_json::to_string(words).map_err(|e| Error::Manual(e.to_string()))?;
    conn.execute(
        concat!(
            "INSERT INTO manual_ocr (system, game, page, lang, words, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ",
            crate::db::now!(),
            ") ON CONFLICT(system, game, page, lang) DO UPDATE SET words = excluded.words, created_at = excluded.created_at"
        ),
        params![system, game, page, lang, json],
    )?;
    Ok(())
}

/// Forget a game's OCR (its manual was replaced or removed).
pub fn clear_ocr(conn: &Connection, system: &str, game: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM manual_ocr WHERE system = ?1 AND game = ?2",
        params![system, game],
    )?;
    Ok(())
}

/// The pages of a game's manual recognised in `lang`, and their text (for
/// search).
pub fn ocr_texts(
    conn: &Connection,
    system: &str,
    game: &str,
    lang: &str,
) -> Result<Vec<(u32, String)>, Error> {
    let mut stmt = conn.prepare(
        "SELECT page, words FROM manual_ocr WHERE system = ?1 AND game = ?2 AND lang = ?3 ORDER BY page",
    )?;
    let rows = stmt.query_map(params![system, game, lang], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (page, json) = row?;
        let words: Vec<OcrWord> = serde_json::from_str(&json).unwrap_or_default();
        out.push((
            page as u32,
            words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    Ok(out)
}
