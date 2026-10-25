//! Emulator Settings: settings for every emulator at once (Global) and for
//! each one, an emulator taking the global value unless it sets its own.
//!
//! One table of settings, [`SETTINGS`] (like `headers.rs`): the ones
//! romforge applies when it starts an emulator, and the ones written into
//! romforge's own copy of each emulator's config, in each one's own words
//! (milestone 21). Stored in the database's `settings`
//! table as `emu.<global or emulator>.<key>`, so the CLI and the app share
//! them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::launch::{self, Env, KNOWN_EMULATORS, KnownEmulator, LaunchOptions, RETROARCH};
use crate::{Error, systems};

mod per_game;
pub use per_game::*;
pub mod raw;
mod scope;
mod table;
pub use table::*;
use table::{applies, choices_for, setting};

/// Setting key → value; a setting not in it isn't set.
pub type Values = BTreeMap<String, String>;

/// Everything set: for every emulator, and per emulator (by its name in
/// [`launch::known`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EmulatorSettings {
    #[serde(default)]
    pub global: Values,
    #[serde(default)]
    pub emulators: BTreeMap<String, Values>,
}

/// An emulator that has a tab: one with settings of its own.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EmulatorTab {
    pub name: &'static str,
    pub label: &'static str,
    /// How it is started, when installed.
    pub found: Option<String>,
    /// The platforms it runs (names).
    pub systems: Vec<&'static str>,
    /// The keys of [`SETTINGS`] that apply to it.
    pub settings: Vec<&'static str>,
    /// For a choice it has only some values of: those.
    pub choices: BTreeMap<&'static str, Vec<&'static str>>,
    /// Its setup in its usual folders, which "Import my existing setup"
    /// copies into romforge's (with a RomForge folder, for emulators
    /// romforge sets up).
    pub existing: Option<launch::existing::ExistingSetup>,
    /// Its developers make no Linux build and it runs as the fallback (the
    /// Linux build most used): why, for its tab.
    #[serde(default)]
    pub fallback: Option<String>,
    /// It keeps game data outside romforge's config that can be moved into
    /// RomForge (`launch::move_in`).
    #[serde(default)]
    pub movable: bool,
}

/// What the Emulator Settings dialog shows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EmulatorSettingsView {
    pub settings: &'static [Setting],
    pub values: EmulatorSettings,
    /// Installed emulators, RetroArch first.
    pub installed: Vec<EmulatorTab>,
    /// Known emulators not installed.
    pub not_installed: Vec<NotInstalled>,
}

/// A known emulator that isn't installed, and where its official build
/// can be installed from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NotInstalled {
    pub name: &'static str,
    pub label: &'static str,
    /// Its official Flatpak can be installed: from where ("Flathub", ...).
    pub flatpak: Option<&'static str>,
    /// A Flatpak of it is installed that isn't its developers' (romforge
    /// doesn't use it).
    #[serde(default)]
    pub unofficial_flatpak: bool,
    /// Its last release, when discontinued: never installed by romforge.
    pub discontinued: Option<&'static str>,
    /// Its official AppImage can be downloaded (`obtain::install_appimage`).
    #[serde(default)]
    pub appimage: bool,
}

const PREFIX: &str = "emu.";
const GLOBAL: &str = "global";

/// Emulators with settings of their own (not ones sharing another's).
fn with_settings() -> impl Iterator<Item = &'static KnownEmulator> {
    std::iter::once(&RETROARCH)
        .chain(KNOWN_EMULATORS)
        .filter(|e| e.shares.is_none())
}

pub fn load(conn: &Connection) -> Result<EmulatorSettings, Error> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE key LIKE ?1")?;
    let rows = stmt.query_map([format!("{PREFIX}%")], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = EmulatorSettings::default();
    for row in rows {
        let (key, value) = row?;
        let Some((scope, key)) = key.strip_prefix(PREFIX).and_then(|k| k.rsplit_once('.')) else {
            continue;
        };
        let values = if scope == GLOBAL {
            &mut out.global
        } else {
            out.emulators.entry(scope.to_owned()).or_default()
        };
        values.insert(key.to_owned(), value);
    }
    Ok(out)
}

/// Check `s` and store it in place of what was set. An empty value means
/// not set (an emulator then takes the global value). Returns what was
/// stored (programs as absolute paths).
pub fn save(conn: &mut Connection, s: &EmulatorSettings) -> Result<EmulatorSettings, Error> {
    let s = validated(s)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM settings WHERE key LIKE ?1", [format!("{PREFIX}%")])?;
    {
        let mut put = tx.prepare("INSERT INTO settings (key, value) VALUES (?1, ?2)")?;
        let scopes =
            std::iter::once((GLOBAL, &s.global)).chain(s.emulators.iter().map(|(n, v)| (n.as_str(), v)));
        for (scope, values) in scopes {
            for (key, value) in values {
                put.execute(params![format!("{PREFIX}{scope}.{key}"), value])?;
            }
        }
    }
    tx.commit()?;
    Ok(s)
}

fn validated(s: &EmulatorSettings) -> Result<EmulatorSettings, Error> {
    let bad = |m: String| Error::Settings(m);
    let mut out = EmulatorSettings::default();
    for (key, value) in &s.global {
        let def = setting(key).filter(|d| d.global);
        let def = def.ok_or_else(|| bad(format!("{key:?} can't be set for every emulator")))?;
        if let Some(v) = checked(def, value).map_err(bad)? {
            out.global.insert(key.clone(), v);
        }
    }
    for (name, values) in &s.emulators {
        let e = launch::known(name)
            .filter(|e| e.shares.is_none())
            .ok_or_else(|| bad(format!("no emulator named {name:?}")))?;
        let mut kept = Values::new();
        for (key, value) in values {
            let def = setting(key).filter(|d| applies(d, e));
            let def = def.ok_or_else(|| bad(format!("{} has no setting {key:?}", e.label)))?;
            if let Some(v) = checked(def, value).map_err(|err| bad(format!("{}: {err}", e.label)))? {
                if choices_for(def, e).is_some_and(|c| !c.contains(&v.as_str())) {
                    return Err(bad(format!("{} has no {} {v:?}", e.label, def.label)));
                }
                kept.insert(key.clone(), v);
            }
        }
        if !kept.is_empty() {
            out.emulators.insert(e.name.to_owned(), kept);
        }
    }
    Ok(out)
}

/// `value` for `def`, tidied; `None` when empty.
fn checked(def: &Setting, value: &str) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    match def.kind {
        Kind::Bool if value == "true" || value == "false" => Ok(Some(value.to_owned())),
        Kind::Bool => Err(format!("{}: {value:?} is not true or false", def.label)),
        Kind::Choice if def.choices.iter().any(|(v, _)| *v == value) => Ok(Some(value.to_owned())),
        Kind::Choice => Err(format!("{}: {value:?} is not one of its choices", def.label)),
        Kind::Path => {
            let p = std::fs::canonicalize(value).map_err(|e| format!("{} {value}: {e}", def.label))?;
            if !p.is_file() {
                return Err(format!("{} is not a program", p.display()));
            }
            Ok(Some(p.to_string_lossy().into_owned()))
        }
        Kind::Args => Ok(Some(value.split_whitespace().collect::<Vec<_>>().join(" "))),
    }
}

/// The value of `key` for `e`: its own, else the global one, else the
/// default.
fn value<'a>(s: &'a EmulatorSettings, e: &KnownEmulator, key: &str) -> Option<&'a str> {
    let own = s.emulators.get(e.settings_name()).and_then(|v| v.get(key));
    own.or_else(|| s.global.get(key))
        .map(String::as_str)
        .or_else(|| setting(key).map(|d| d.default))
}

/// Whether romforge's game pad hotkeys are on (the global setting; on
/// unless turned off).
pub fn hotkeys_on(s: &EmulatorSettings) -> bool {
    let set = s.global.get("hotkeys").map(String::as_str);
    set.or_else(|| setting("hotkeys").map(|d| d.default)) == Some("true")
}

/// Put what `s` sets into `env`, for [`launch`] to start emulators by.
pub fn apply(s: &EmulatorSettings, env: &mut Env) {
    for e in with_settings() {
        if let Some(p) = value(s, e, "program").filter(|p| !p.is_empty()) {
            env.user_paths.insert(e.name.to_owned(), PathBuf::from(p));
        }
        let mut options = LaunchOptions {
            windowed: value(s, e, "fullscreen") == Some("false"),
            args: value(s, e, "args")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            hotkeys: hotkeys_on(s),
            ..LaunchOptions::default()
        };
        // Config keys: set ones written in the emulator's words, unset ones
        // (or a value it lacks) taken out, so its own default applies.
        for def in SETTINGS {
            let v = value(s, e, def.key).filter(|v| !v.is_empty());
            for k in def.keys.iter().filter(|k| k.emulator == e.settings_name()) {
                add_key(&mut options.config, k, v.and_then(|v| k.words.of(v)));
            }
        }
        if options != LaunchOptions::default() {
            env.options.insert(e.name.to_owned(), options);
        }
    }
}

/// `k` set to `v` (or taken out, with none) in its file's group of `writes`.
pub(crate) fn add_key(writes: &mut Vec<launch::ConfigWrite>, k: &Key, v: Option<String>) {
    let path = PathBuf::from(k.file);
    let i = match writes.iter().position(|w| w.path == path) {
        Some(i) => i,
        None => {
            writes.push(launch::ConfigWrite {
                path,
                format: k.format,
                keys: Vec::new(),
                remove: Vec::new(),
            });
            writes.len() - 1
        }
    };
    match v {
        Some(v) => writes[i].keys.push((k.key.to_owned(), v)),
        None => writes[i].remove.push(k.key.to_owned()),
    }
}

/// Everything the dialog shows, with `env` (Emulator Settings applied)
/// saying which emulators are installed.
pub fn view(conn: &Connection, env: &Env) -> Result<EmulatorSettingsView, Error> {
    let (mut installed, mut not_installed) = (Vec::new(), Vec::new());
    for e in with_settings() {
        let Some(found) = env.find(e) else {
            not_installed.push(NotInstalled {
                name: e.name,
                label: e.label,
                flatpak: e
                    .flatpak
                    .filter(|_| e.discontinued.is_none())
                    .and(match e.flatpak_from {
                        launch::FlatpakFrom::Flathub => Some("Flathub"),
                        launch::FlatpakFrom::Fallback => Some("Flathub (not official)"),
                        launch::FlatpakFrom::Own { .. } => Some("its developers' own repository"),
                        launch::FlatpakFrom::Others => None,
                    }),
                unofficial_flatpak: env.unofficial_flatpak(e),
                discontinued: e.discontinued,
                appimage: e.official_appimage.is_some() && env.home.is_some(),
            });
            continue;
        };
        let runs = |s: &systems::System| {
            if e.name == RETROARCH.name {
                !s.cores.is_empty()
            } else {
                with_shared(e).any(|n| s.emulators.contains(&n))
            }
        };
        installed.push(EmulatorTab {
            name: e.name,
            label: e.label,
            found: Some(found),
            systems: systems::SYSTEMS
                .iter()
                .filter(|s| runs(s))
                .map(|s| s.name)
                .collect(),
            settings: SETTINGS.iter().filter(|d| applies(d, e)).map(|d| d.key).collect(),
            choices: SETTINGS
                .iter()
                .filter_map(|d| Some((d.key, choices_for(d, e)?)))
                .collect(),
            fallback: env.runs_fallback(e).then(|| {
                format!(
                    "{} makes no Linux build of its own, so romforge uses Flathub's, the Linux build most used (not made by its developers). A {} of your own (your distribution's) is used instead when there is one.",
                    e.label, e.label
                )
            }),
            movable: env.home.is_some() && launch::move_in::movable(e.settings_name()),
            existing: env
                .home
                .as_ref()
                .zip(std::env::var_os("HOME"))
                .filter(|_| launch::existing::importable(e.name))
                .map(|(h, user)| {
                    let user = PathBuf::from(user);
                    launch::existing::existing(e, h, &user, &launch::existing::Xdg::detect(&user), Some(conn))
                }),
        });
    }
    Ok(EmulatorSettingsView {
        settings: SETTINGS,
        values: load(conn)?,
        installed,
        not_installed,
    })
}

/// `e`'s name and those of the entries sharing its settings.
fn with_shared(e: &'static KnownEmulator) -> impl Iterator<Item = &'static str> {
    KNOWN_EMULATORS
        .iter()
        .filter(move |k| k.settings_name() == e.name)
        .map(|k| k.name)
}

#[cfg(test)]
mod tests;
