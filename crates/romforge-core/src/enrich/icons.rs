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

/// The sidebar's platform icon styles: RetroArch's flat controllers
/// (`flatui`, the default) or its consoles (`systematic`), or each
/// platform's own logo from Wikidata (`logo`, a Commons file).
pub const PLATFORM_ICON_THEMES: &[&str] = &["flatui", "systematic", "logo"];

/// Where a platform's icon of `theme` is kept, without its extension:
/// `<cache>/platform-icons/<theme>/<key>`.
pub fn platform_icon_stem(cache_dir: &Path, theme: &str, system: &systems::System) -> PathBuf {
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
/// -> file. RetroArch's themes come from retroarch-assets (`base`) by the
/// platform's libretro name; logos from Wikidata (`wikidata`, its SPARQL
/// endpoint) by the platform's item, as small PNGs Commons renders. A
/// platform with none is remembered (a `.missing` file) and not asked
/// again. Errors (offline) don't forget anything; the first is returned
/// with what there is.
pub fn platform_icons(
    cache_dir: &Path,
    base: &str,
    wikidata: &str,
    theme: &str,
    systems: &[&'static systems::System],
) -> (BTreeMap<String, PathBuf>, Option<Error>) {
    let mut out = BTreeMap::new();
    let mut first_error: Option<Error> = None;
    if !PLATFORM_ICON_THEMES.contains(&theme) {
        return (out, Some(Error::Settings(format!("no icon theme {theme:?}"))));
    }
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
            .filter_map(|(s, _)| s.wikidata.first().map(|q| (*q).to_owned()))
            .collect();
        let found = match crate::wikidata::logos(wikidata, &qids) {
            Ok(f) => f,
            Err(e) => return (out, Some(e)),
        };
        for (system, stem) in &todo {
            let Some(url) = system.wikidata.first().and_then(|q| found.get(*q)) else {
                missing(stem);
                continue;
            };
            let sep = if url.contains('?') { '&' } else { '?' };
            match fetch::get_optional(&format!("{url}{sep}width=256")) {
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
        let Some(name) = libretro_name(system) else {
            missing(stem);
            continue;
        };
        let url = format!(
            "{base}/{theme}/png/{}",
            fetch::encode_segment(&format!("{name}.png"))
        );
        match fetch::get_optional(&url) {
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
