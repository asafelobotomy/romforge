use super::*;

/// libretro's name for a system (its database and thumbnail folders).
pub fn libretro_name(system: &systems::System) -> Option<&'static str> {
    (!system.libretro.is_empty())
        .then_some(system.libretro)
        .or_else(|| system.nointro.first().or(system.dat_names.first()).copied())
}

/// Where RetroArch's menu icons live (retroarch-assets, CC BY 4.0): one
/// picture per system in each theme, named like the system's playlist.
pub const PLATFORM_ICONS_BASE: &str =
    "https://raw.githubusercontent.com/libretro/retroarch-assets/master/xmb";

/// Where platform pictures come from: RetroArch's icons
/// ([`PLATFORM_ICONS_BASE`]), Wikidata's SPARQL endpoint (the logos'
/// items) and Commons (a logo the systems table names by file).
#[derive(Debug, Clone, Copy)]
pub struct IconSources<'a> {
    pub assets: &'a str,
    pub wikidata: &'a str,
    /// Where a Commons file is served by name (as Wikidata's P154 gives it).
    pub commons: &'a str,
}

impl IconSources<'static> {
    /// The real ones.
    pub const LIVE: Self = IconSources {
        assets: PLATFORM_ICONS_BASE,
        wikidata: crate::wikidata::ENDPOINT,
        commons: "https://commons.wikimedia.org/wiki/Special:FilePath/",
    };
}

/// Bumped when the systems table's icons or logos change: the kept icons
/// and `.missing` marks are then fetched again.
const ICONS_REVISION: &str = "2";

/// romforge's own icon of a platform: system key, PNG.
macro_rules! own {
    ($key:literal) => {
        (
            $key,
            include_bytes!(concat!("../../assets/platform-icons/", $key, ".png")),
        )
    };
}

/// romforge's own icons in flatui's style, for platforms flatui lacks:
/// drawn by hand (SVG sources beside them) where neither RetroArch theme
/// has one, generated (GUIDE.md, Licences and credits) for the most played
/// of the rest.
const OWN_ICONS: &[(&str, &[u8])] = &[
    own!("PC"),
    own!("VIS"),
    own!("Lindbergh"),
    own!("Nintendo Switch"),
    own!("PlayStation 3"),
    own!("Xbox 360"),
    own!("Neo Geo"),
    own!("Xbox"),
    own!("Amiga"),
    own!("ZX Spectrum"),
    own!("PlayStation Vita"),
    own!("PlayStation 4"),
    own!("ScummVM"),
];

/// The platforms romforge draws its own icon for.
pub fn own_icon_keys() -> impl Iterator<Item = &'static str> {
    OWN_ICONS.iter().map(|(k, _)| *k)
}

/// The name of a platform's picture in RetroArch's icon themes.
fn icon_name(system: &systems::System) -> Option<&'static str> {
    (!system.icon.is_empty())
        .then_some(system.icon)
        .or_else(|| libretro_name(system))
}

/// Forget icons kept under an older [`ICONS_REVISION`]. Under a lock, so
/// that of two lookups at once (the sidebar's icons, a splash's logo) only
/// the first clears the folder, not the icons the other has just saved.
fn renew(cache_dir: &Path) {
    static RENEWING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = RENEWING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = cache_dir.join("platform-icons");
    let stamp = dir.join("revision");
    if std::fs::read_to_string(&stamp).is_ok_and(|r| r.trim() == ICONS_REVISION) {
        return;
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = save(&stamp, ICONS_REVISION.as_bytes());
}

/// The sidebar's platform icon styles: RetroArch's flat controllers
/// (`flatui`, the default) or its consoles (`systematic`), or each
/// platform's own logo from Wikidata (`logo`, a Commons file).
pub const PLATFORM_ICON_THEMES: &[&str] = &["flatui", "systematic", "logo"];

/// Where a platform's icon of `theme` is kept, without its extension:
/// `<cache>/platform-icons/<theme>/<key>`.
fn platform_icon_stem(cache_dir: &Path, theme: &str, system: &systems::System) -> PathBuf {
    cache_dir
        .join("platform-icons")
        .join(theme)
        .join(system.key.replace(['/', '\\'], "_"))
}

/// `stem` with `.ext` added (a key such as "V.Smile" keeps its dot).
fn dotted(stem: &Path, ext: &str) -> PathBuf {
    let mut p = stem.as_os_str().to_owned();
    p.push(".");
    p.push(ext);
    PathBuf::from(p)
}

/// The icon file kept for a platform, if any.
fn kept_icon(stem: &Path) -> Option<PathBuf> {
    ["png", "jpg", "webp"]
        .iter()
        .map(|e| dotted(stem, e))
        .find(|p| p.exists())
}

/// The icons of `theme` for `systems`, fetched the first time: system key
/// -> file. RetroArch's themes come from retroarch-assets by the
/// platform's icon name (flatui's missing ones from systematic); logos by
/// the table's Commons file, else from Wikidata by the platform's items,
/// as small PNGs Commons renders. A
/// platform with none is remembered (a `.missing` file) and not asked
/// again. Errors (offline) don't forget anything; the first is returned
/// with what there is.
pub fn platform_icons(
    cache_dir: &Path,
    from: &IconSources,
    theme: &str,
    systems: &[&'static systems::System],
) -> (BTreeMap<String, PathBuf>, Option<Error>) {
    let mut out = BTreeMap::new();
    let mut first_error: Option<Error> = None;
    if !PLATFORM_ICON_THEMES.contains(&theme) {
        return (out, Some(Error::Settings(format!("no icon theme {theme:?}"))));
    }
    renew(cache_dir);
    let mut todo: Vec<(&'static systems::System, PathBuf)> = Vec::new();
    for system in systems {
        let stem = platform_icon_stem(cache_dir, theme, system);
        match kept_icon(&stem) {
            Some(p) => {
                out.insert(system.key.to_owned(), p);
            }
            None if !dotted(&stem, "missing").exists() => todo.push((system, stem)),
            None => {}
        }
    }
    if todo.is_empty() {
        return (out, None);
    }
    let missing = |stem: &Path| {
        let _ = save(&dotted(stem, "missing"), b"");
    };
    let mut keep = |system: &systems::System, stem: &Path, data: &[u8]| -> Result<(), Error> {
        let Some(ext) = image_ext(data) else {
            missing(stem);
            return Ok(());
        };
        let path = dotted(stem, ext);
        save(&path, data)?;
        out.insert(system.key.to_owned(), path);
        Ok(())
    };
    if theme == "logo" {
        let qids: Vec<String> = todo
            .iter()
            .filter(|(s, _)| s.logo.is_empty())
            .flat_map(|(s, _)| s.wikidata.iter().map(|q| (*q).to_owned()))
            .collect();
        let found = if qids.is_empty() {
            Default::default()
        } else {
            match crate::wikidata::logos(from.wikidata, &qids) {
                Ok(f) => f,
                Err(e) => return (out, Some(e)),
            }
        };
        for (system, stem) in &todo {
            // The table's own pick, else the first item (in the table's
            // order) that has a logo.
            let url = if system.logo.is_empty() {
                system
                    .wikidata
                    .iter()
                    .find_map(|q| found.get(*q).and_then(|f| f.first()).cloned())
            } else {
                Some(format!("{}{}", from.commons, fetch::encode_segment(system.logo)))
            };
            let Some(url) = url else {
                missing(stem);
                continue;
            };
            let sep = if url.contains('?') { '&' } else { '?' };
            match fetch::get_image(&format!("{url}{sep}width=256")) {
                Ok(Some(data)) => {
                    if let Err(e) = keep(system, stem, &data) {
                        first_error.get_or_insert(e);
                    }
                }
                Ok(None) => missing(stem),
                Err(e) => {
                    first_error = Some(e);
                    break;
                }
            }
        }
        return (out, first_error);
    }
    for (system, stem) in &todo {
        let file = icon_name(system).map(|n| fetch::encode_segment(&format!("{n}.png")));
        let mut got = match &file {
            Some(file) => fetch::get_image(&format!("{}/{theme}/png/{file}", from.assets)),
            None => Ok(None),
        };
        // One RetroArch doesn't draw: romforge's own, in flatui's style; else
        // (flatui draws far fewer platforms) systematic's.
        if matches!(got, Ok(None)) {
            if let Some((_, png)) = OWN_ICONS.iter().find(|(k, _)| *k == system.key) {
                got = Ok(Some(png.to_vec()));
            } else if let (Some(file), "flatui") = (&file, theme) {
                got = fetch::get_image(&format!("{}/systematic/png/{file}", from.assets));
            }
        }
        match got {
            Ok(Some(data)) => {
                if let Err(e) = keep(system, stem, &data) {
                    first_error.get_or_insert(e);
                }
            }
            Ok(None) => missing(stem),
            // Offline, or rate-limited: try again another time.
            Err(e) => {
                first_error = Some(e);
                break;
            }
        }
    }
    (out, first_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lookups at once over an old revision: the folder is cleared once,
    /// never under an icon another has saved since.
    #[test]
    fn renew_clears_once_for_lookups_at_once() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("platform-icons");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("revision"), "0").unwrap();
        std::fs::write(dir.join("old.png"), "old").unwrap();
        std::thread::scope(|s| {
            for i in 0..8 {
                let cache = t.path();
                s.spawn(move || {
                    renew(cache);
                    save(&cache.join(format!("platform-icons/logo/{i}.png")), b"png").unwrap();
                });
            }
        });
        assert!(!dir.join("old.png").exists());
        for i in 0..8 {
            assert!(dir.join(format!("logo/{i}.png")).exists(), "{i} was cleared");
        }
        let kept = std::fs::read_to_string(dir.join("revision")).unwrap();
        assert_eq!(kept, ICONS_REVISION);
    }
}
