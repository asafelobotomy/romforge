//! All settings (milestone 21): every key in romforge's own copy of an
//! emulator's config, named as the emulator names it, to read and change
//! by hand. The keys romforge sets itself (folders, BIOS files, hotkeys,
//! the settings above) are shown locked, with why. A change is refused
//! while the emulator runs, and journaled (`config_edits`) with the value
//! it replaces, so undo puts it back. Values are written as the file has
//! them (a TOML string with its quotes).

use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::{Connection, params};

use super::{SETTINGS, Words};
use crate::Error;
use crate::launch::{self, Env, Format, KnownEmulator};

/// What a value looks like, for the field it gets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RawKind {
    /// One of the emulator's two words for on and off.
    Bool {
        on: String,
        off: String,
    },
    Number,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RawSetting {
    pub key: String,
    pub value: String,
    pub kind: RawKind,
    /// Why it can't be changed here; `None`: it can.
    pub locked: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RawFile {
    /// From romforge's config folder for the emulator.
    pub file: String,
    pub format: Format,
    /// Written yet (the emulator has been started from romforge).
    pub exists: bool,
    pub settings: Vec<RawSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RawView {
    pub emulator: &'static str,
    pub files: Vec<RawFile>,
    /// Running now: nothing can be changed until it closes.
    pub running: bool,
    /// A change that undo can take back.
    pub undo: bool,
    /// It can be asked to write all its settings (MAME's `-createconfig`).
    pub write_defaults: bool,
}

/// One change: `value` `None` takes the key out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawEdit {
    pub file: String,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
}

/// A file of `e`'s romforge writes into, and the keys it sets there.
struct Managed {
    file: String,
    format: Format,
    locked: BTreeMap<String, String>,
}

fn emulator(name: &str) -> Result<&'static KnownEmulator, Error> {
    launch::known(name)
        .filter(|e| e.shares.is_none())
        .ok_or_else(|| Error::Settings(format!("no emulator named {name:?}")))
}

/// The entry for `file` in `out`, added when missing.
fn slot<'a>(out: &'a mut Vec<Managed>, file: &str, format: Format) -> &'a mut Managed {
    let i = out.iter().position(|m| m.file == file).unwrap_or_else(|| {
        out.push(Managed {
            file: file.to_owned(),
            format,
            locked: BTreeMap::new(),
        });
        out.len() - 1
    });
    &mut out[i]
}

/// `e`'s config folder and the files romforge writes there.
fn managed(env: &Env, e: &KnownEmulator) -> Result<(PathBuf, Vec<Managed>), Error> {
    let home = env.home.as_ref().ok_or_else(|| {
        Error::Settings("emulators' settings are kept in a RomForge folder: none is set".into())
    })?;
    let dir = home.emulator_config(e.settings_name());
    let mut out: Vec<Managed> = Vec::new();
    for s in SETTINGS {
        for k in s.keys.iter().filter(|k| k.emulator == e.settings_name()) {
            let why = format!("set by romforge's {} setting", s.label);
            slot(&mut out, k.file, k.format)
                .locked
                .insert(k.key.to_owned(), why);
        }
    }
    if let Some((_, prep)) = launch::managed(e, home, &env.options(e), env.user.as_ref()) {
        for w in &prep.writes {
            let Ok(rel) = w.path.strip_prefix(&dir) else {
                continue;
            };
            // RetroArch's per-system files hold only romforge's folders.
            if e.settings_name() == "retroarch" && rel.starts_with("systems") {
                continue;
            }
            let m = slot(&mut out, &rel.to_string_lossy(), w.format);
            for key in w.keys.iter().map(|(k, _)| k).chain(&w.remove) {
                m.locked
                    .entry(key.clone())
                    .or_insert_with(|| "romforge sets it at every start".into());
            }
        }
    }
    Ok((dir, out))
}

/// The field for `value`: a toggle when it is one of `e`'s words for on
/// and off.
fn kind_of(e: &KnownEmulator, value: &str) -> RawKind {
    let pairs = SETTINGS
        .iter()
        .flat_map(|s| s.keys)
        .filter(|k| k.emulator == e.settings_name())
        .filter_map(|k| match k.words {
            Words::Bool(on, off) => Some((on, off)),
            _ => None,
        })
        .chain([("true", "false")]);
    for (on, off) in pairs {
        if value == on || value == off {
            return RawKind::Bool {
                on: on.to_owned(),
                off: off.to_owned(),
            };
        }
    }
    if value.parse::<f64>().is_ok() {
        RawKind::Number
    } else {
        RawKind::Text
    }
}

/// What the All settings panel shows for `name`.
pub fn view(conn: &Connection, env: &Env, name: &str) -> Result<RawView, Error> {
    let e = emulator(name)?;
    let (dir, files) = managed(env, e)?;
    let files = files
        .into_iter()
        .map(|m| {
            let text = std::fs::read_to_string(dir.join(&m.file));
            let settings = launch::read(m.format, text.as_deref().unwrap_or(""))
                .into_iter()
                .map(|(key, value)| RawSetting {
                    kind: kind_of(e, &value),
                    locked: m.locked.get(&key).cloned(),
                    key,
                    value,
                })
                .collect();
            RawFile {
                exists: text.is_ok(),
                file: m.file,
                format: m.format,
                settings,
            }
        })
        .collect();
    Ok(RawView {
        emulator: e.name,
        files,
        running: launch::running(e),
        undo: last_batch(conn, e.name)?.is_some(),
        write_defaults: e.name == "mame",
    })
}

fn last_batch(conn: &Connection, name: &str) -> Result<Option<i64>, Error> {
    Ok(conn.query_row(
        "SELECT MAX(batch) FROM config_edits WHERE emulator = ?1 AND undone_at IS NULL",
        [name],
        |r| r.get(0),
    )?)
}

/// Write `sets` and take out `remove` in `file`, returning each key's value
/// before (`None`: it wasn't there).
fn write(
    dir: &std::path::Path,
    m: &Managed,
    sets: &[(String, String)],
    remove: &[String],
) -> Result<BTreeMap<String, Option<String>>, Error> {
    let path = dir.join(&m.file);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let had: BTreeMap<String, String> = launch::read(m.format, &text).into_iter().collect();
    let new = launch::merge(m.format, &text, sets, remove)?;
    crate::atomic::write(&path, new.as_bytes())?;
    Ok(sets
        .iter()
        .map(|(k, _)| k)
        .chain(remove)
        .map(|k| (k.clone(), had.get(k).cloned()))
        .collect())
}

fn refuse_running(e: &KnownEmulator) -> Result<(), Error> {
    if launch::running(e) {
        return Err(Error::Settings(format!(
            "{} is running: close it first, or it writes its own settings over these",
            e.label
        )));
    }
    Ok(())
}

/// Keys to set and keys to take out, in one file.
type Changes = (Vec<(String, String)>, Vec<String>);

/// Make `edits` in `name`'s romforge config, as one batch for undo.
/// Returns the batch.
pub fn edit(conn: &mut Connection, env: &Env, name: &str, edits: &[RawEdit]) -> Result<i64, Error> {
    let e = emulator(name)?;
    refuse_running(e)?;
    let (dir, files) = managed(env, e)?;
    let bad = |m: String| Error::Settings(m);
    let mut by_file: BTreeMap<&str, Changes> = BTreeMap::new();
    for ed in edits {
        let m = files.iter().find(|m| m.file == ed.file).ok_or_else(|| {
            bad(format!(
                "{} has no settings file {:?} romforge keeps",
                e.label, ed.file
            ))
        })?;
        let key = ed.key.trim();
        if key.is_empty() || key.contains('\n') || (key.contains('=') && m.format != Format::NestedYaml) {
            return Err(bad(format!("{key:?} isn't a setting's name")));
        }
        if let Some(why) = m.locked.get(key) {
            return Err(bad(format!("{key}: {why}")));
        }
        let entry = by_file.entry(m.file.as_str()).or_default();
        match &ed.value {
            Some(v) => entry.0.push((key.to_owned(), v.trim().to_owned())),
            None => entry.1.push(key.to_owned()),
        }
    }
    let tx = conn.transaction()?;
    let batch: i64 = tx.query_row("SELECT COALESCE(MAX(batch), 0) + 1 FROM config_edits", [], |r| {
        r.get(0)
    })?;
    for (file, (sets, remove)) in &by_file {
        let m = files.iter().find(|m| m.file == *file).expect("checked above");
        let had = write(&dir, m, sets, remove)?;
        let new: BTreeMap<&str, &str> = sets.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        for (key, old) in had {
            tx.execute(
                concat!(
                    "INSERT INTO config_edits (batch, emulator, file, key, old_value, new_value, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ",
                    crate::db::now!(),
                    ")"
                ),
                params![batch, e.name, file, key, old, new.get(key.as_str())],
            )?;
        }
    }
    tx.commit()?;
    Ok(batch)
}

/// Take back the last batch of changes made to `name`'s config here.
/// Returns how many keys went back.
pub fn undo(conn: &Connection, env: &Env, name: &str) -> Result<usize, Error> {
    let e = emulator(name)?;
    refuse_running(e)?;
    let batch = last_batch(conn, e.name)?
        .ok_or_else(|| Error::Settings(format!("no change to {}'s settings to undo", e.label)))?;
    let (dir, files) = managed(env, e)?;
    let rows: Vec<(String, String, Option<String>)> = conn
        .prepare("SELECT file, key, old_value FROM config_edits WHERE batch = ?1 ORDER BY edit_id DESC")?
        .query_map([batch], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for (file, key, old) in &rows {
        let Some(m) = files.iter().find(|m| m.file == *file) else {
            continue;
        };
        match old {
            Some(v) => write(&dir, m, &[(key.clone(), v.clone())], &[])?,
            None => write(&dir, m, &[], std::slice::from_ref(key))?,
        };
    }
    conn.execute(
        concat!(
            "UPDATE config_edits SET undone_at = ",
            crate::db::now!(),
            " WHERE batch = ?1"
        ),
        [batch],
    )?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests;
