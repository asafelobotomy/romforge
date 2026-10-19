//! Milestone 26: a game's manual, as a PDF read in romforge.
//!
//! Manuals are copyrighted scans, so they are treated like rule 10's
//! unlicensed data: added or fetched by the user, never shipped. Kept as
//! `<Media>/<System>/manual/<Title>.pdf` (the RomForge folder's `Media/`,
//! else the art cache), by system key and title like play time.
//!
//! Where one comes from, in order:
//! 1. the user's own PDF;
//! 2. the Vimm's Lair Manual Project, found through Hasheous by the game's
//!    SHA1 (`api/v1/Lookup/ByHash/sha1/<sha1>`, no key or account; the
//!    attribute `VIMMManualId`; by hash, so house rule 8 holds), and
//!    downloaded only when the user clicks for it, one at a time:
//!    `dl.vimm.net`'s robots.txt asks for no automated fetching.

use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

const HASHEOUS: &str = "https://hasheous.org/api/v1/Lookup/ByHash/sha1/";
/// Where Vimm's Lair serves a manual's PDF (its manual page's own form).
const VIMM_PDF: &str = "https://dl.vimm.net/";
/// The page a manual has on Vimm's Lair.
pub const VIMM_PAGE: &str = "https://vimm.net/manual/";

/// What romforge knows of a game's manual.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ManualInfo {
    pub path: Option<PathBuf>,
    /// "added by you", "Vimm's Lair, downloaded by you"…
    pub source: Option<String>,
    /// Its Vimm's Lair manual id, when Hasheous knows one.
    pub vimm_id: Option<String>,
    /// Hasheous has been asked.
    pub checked: bool,
    /// The page last read (from 1).
    pub last_page: u32,
}

/// Where a game's manual goes under `media`.
pub fn manual_path(media: &Path, system: &str, game: &str) -> PathBuf {
    media
        .join(crate::organize::sanitize(system))
        .join("manual")
        .join(format!("{}.pdf", crate::organize::sanitize(game)))
}

pub fn manual(conn: &Connection, system: &str, game: &str) -> Result<ManualInfo, Error> {
    let row = conn
        .query_row(
            "SELECT path, source, vimm_id, checked_at IS NOT NULL, last_page FROM manuals WHERE system = ?1 AND game = ?2",
            params![system, game],
            |r| {
                Ok(ManualInfo {
                    path: r.get::<_, Option<String>>(0)?.map(PathBuf::from),
                    source: r.get(1)?,
                    vimm_id: r.get(2)?,
                    checked: r.get(3)?,
                    last_page: r.get::<_, i64>(4)?.max(1) as u32,
                })
            },
        )
        .optional()?;
    let mut info = row.unwrap_or(ManualInfo {
        last_page: 1,
        ..ManualInfo::default()
    });
    // A file removed by hand is no manual.
    if info.path.as_ref().is_some_and(|p| !p.is_file()) {
        info.path = None;
    }
    Ok(info)
}

/// Whether `bytes` start like a PDF.
fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF")
}

/// Keep `pdf` as the game's manual (a copy; romforge's earlier copy is
/// replaced). Returns where it is.
pub fn add_manual(
    conn: &Connection,
    media: &Path,
    system: &str,
    game: &str,
    pdf: &Path,
    source: &str,
) -> Result<PathBuf, Error> {
    let mut head = [0u8; 4];
    std::fs::File::open(pdf)?.read_exact(&mut head).ok();
    if !is_pdf(&head) {
        return Err(Error::Launch(format!("{} isn't a PDF", pdf.display())));
    }
    let dest = manual_path(media, system, game);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if dest != pdf {
        crate::atomic::write_new(&dest, |mut f| {
            std::io::copy(&mut std::fs::File::open(pdf)?, &mut f)?;
            Ok(())
        })?;
    }
    conn.execute(
        "INSERT INTO manuals (system, game, path, source) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(system, game) DO UPDATE SET path = excluded.path, source = excluded.source, last_page = 1",
        params![system, game, dest.to_string_lossy(), source],
    )?;
    Ok(dest)
}

/// Forget the game's manual and delete romforge's copy of it.
pub fn remove_manual(conn: &Connection, media: &Path, system: &str, game: &str) -> Result<(), Error> {
    let info = manual(conn, system, game)?;
    if let Some(p) = info.path.filter(|p| p.starts_with(media)) {
        std::fs::remove_file(p)?;
    }
    conn.execute(
        "UPDATE manuals SET path = NULL, source = NULL, last_page = 1 WHERE system = ?1 AND game = ?2",
        params![system, game],
    )?;
    Ok(())
}

pub fn set_page(conn: &Connection, system: &str, game: &str, page: u32) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO manuals (system, game, last_page) VALUES (?1, ?2, ?3)
         ON CONFLICT(system, game) DO UPDATE SET last_page = excluded.last_page",
        params![system, game, page.max(1)],
    )?;
    Ok(())
}

/// The `VIMMManualId` in a Hasheous lookup's answer.
fn vimm_id_in(json: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(json).ok()?;
    v.get("attributes")?.as_array()?.iter().find_map(|a| {
        (a.get("attributeName")?.as_str()? == "VIMMManualId")
            .then(|| a.get("value")?.as_str().map(str::to_owned))
            .flatten()
    })
}

/// Ask Hasheous for the game's Vimm's Lair manual id by each of `sha1s`
/// (its files'), once: the answer, even none, is kept.
pub fn lookup_vimm(
    conn: &Connection,
    system: &str,
    game: &str,
    sha1s: &[String],
) -> Result<Option<String>, Error> {
    let mut found = None;
    for sha1 in sha1s {
        if let Some(body) = crate::fetch::get_optional(&format!("{HASHEOUS}{sha1}"))?
            && let Some(id) = vimm_id_in(&body)
        {
            found = Some(id);
            break;
        }
    }
    conn.execute(
        concat!(
            "INSERT INTO manuals (system, game, vimm_id, checked_at) VALUES (?1, ?2, ?3, ",
            crate::db::now!(),
            ") ON CONFLICT(system, game) DO UPDATE SET vimm_id = excluded.vimm_id, checked_at = excluded.checked_at"
        ),
        params![system, game, found],
    )?;
    Ok(found)
}

/// Download the game's manual from Vimm's Lair at `dpi` (one, on the
/// user's click) into `staging`, and keep it if it is a PDF.
pub fn download_vimm(
    conn: &Connection,
    media: &Path,
    staging: &Path,
    system: &str,
    game: &str,
    dpi: u32,
) -> Result<PathBuf, Error> {
    let id = manual(conn, system, game)?
        .vimm_id
        .ok_or_else(|| Error::Launch(format!("no Vimm's Lair manual is known for {game}")))?;
    let url = format!(
        "{VIMM_PDF}?manualId={id}&category=pdf&destDPI={}",
        dpi.clamp(50, 640)
    );
    let page = format!("{VIMM_PAGE}{id}");
    let req = crate::fetch::Request {
        headers: vec![("Referer", page.clone())],
        max_body: Some(512 << 20),
        ..crate::fetch::Request::get(&url)
    };
    let bytes = crate::fetch::request(&req)?.unwrap_or_default();
    if !is_pdf(&bytes) {
        return Err(Error::Launch(format!(
            "Vimm's Lair didn't send a PDF; download it from {page} and drop it on the Manual tab"
        )));
    }
    std::fs::create_dir_all(staging)?;
    let tmp = staging.join(format!("manual-{id}.pdf"));
    crate::atomic::write(&tmp, &bytes)?;
    let kept = add_manual(conn, media, system, game, &tmp, "Vimm's Lair, downloaded by you");
    let _ = std::fs::remove_file(&tmp);
    kept
}

#[cfg(test)]
mod tests;
