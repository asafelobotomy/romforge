use super::*;

/// How to start `game` on `system_key` from `files` (path, archive member).
pub(super) fn start(
    conn: &Connection,
    env: &Env,
    game: &str,
    system_key: &str,
    files: &[(PathBuf, String)],
    emulator: Option<&str>,
) -> Result<LaunchSpec, Error> {
    let game = game.to_owned();
    let system = systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
    let Some(first) = files.first() else {
        return Err(Error::Launch(format!("no file for {game}")));
    };

    // The file to start: an archive (as a whole), a sheet, a CHD, else the
    // first file.
    let in_archive = files.iter().find(|f| !f.1.is_empty()).map(|f| f.0.clone());
    let is_sheet = |p: &Path| {
        p.extension().is_some_and(|e| {
            ["cue", "gdi", "m3u"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
        })
    };
    let sheet = files
        .iter()
        .map(|f| f.0.clone())
        .find(|p| is_sheet(p))
        .or_else(|| own_sheet(&first.0, &is_sheet));
    let (rom, archived) = match (in_archive, sheet) {
        (Some(a), _) => {
            let chd = a.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd"));
            (a, !chd)
        }
        (None, Some(s)) => (s, false),
        (None, None) => (first.0.clone(), false),
    };
    // Azahar and its predecessors (standalone and core) have no loader for
    // a CIA: they install one into their NAND and start the installed title.
    if system.emulators.contains(&"azahar")
        && rom
            .extension()
            .is_some_and(|e| ["cia", "zcia"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
    {
        return Err(Error::Launch(format!(
            "{game}: 3DS emulators install CIA files rather than starting them: install it from the emulator's File menu"
        )));
    }
    // One disk of a set: start the set's playlist, so disks can be swapped.
    let rom = playlist_for(&rom).unwrap_or(rom);

    // The folder the game is in (a folder game's own folder): its other
    // disks, tracks and saves are there.
    let grant = if rom.is_dir() {
        Some(rom.clone())
    } else {
        rom.parent().map(Path::to_path_buf)
    };
    // `uses_home`: romforge sets the emulator up from the RomForge folder
    // (its config, folders or BIOS folder), so a Flatpak is given it too.
    let spec = |emulator: String, runner: &Runner, args: Vec<String>, prepare: Prepare, uses_home: bool| {
        let home = env.home.as_ref().filter(|_| uses_home);
        let grants = owned::grants(grant.as_deref(), home);
        let (program, args, env) = command(runner, args, &grants, &prepare.env);
        LaunchSpec {
            game: game.clone(),
            system: system.key.to_owned(),
            emulator,
            rom: rom.clone(),
            program,
            args,
            env,
            prepare,
        }
    };
    // romforge's own config for a known emulator, when there is a RomForge
    // folder and romforge writes one for it.
    let owned = |e: &KnownEmulator| {
        let options = env.options(e);
        env.home
            .as_ref()
            .and_then(|h| owned::owned(e, h, system, &options, env.user.as_ref()))
    };
    // An arcade game is started by its set's name; anything else, by its file.
    let set = if system.arcade {
        game.clone()
    } else {
        rom.file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    };
    let wanted = |name: &str| emulator.is_none_or(|w| w.eq_ignore_ascii_case(name));
    let mut why_not = Vec::new();

    for u in user_emulators(conn)?
        .into_iter()
        .filter(|u| u.system == system.key)
    {
        if !wanted(&u.name) {
            continue;
        }
        if archived && !u.archives {
            why_not.push(format!("{} can't open archives", u.name));
            continue;
        }
        match env.which(&u.program) {
            Some(p) => {
                // `{bios}`: `Bios/<its name>/`, with a RomForge folder.
                let bios = env.home.as_ref().and_then(|h| user_bios(h, &u.name));
                let uses_home = bios.is_some() && u.args.iter().any(|a| a.contains("{bios}"));
                let vars = Vars {
                    rom: &rom,
                    set: &set,
                    core: None,
                    config: None,
                    bios: bios.as_deref(),
                    system: system.key,
                };
                return Ok(spec(
                    u.name.clone(),
                    &Runner::Program(p),
                    fill(&u.args, &vars),
                    Prepare::default(),
                    uses_home,
                ));
            }
            None => why_not.push(format!("{} ({}) not found", u.name, u.program)),
        }
    }
    if let Some(ra) = env.retroarch() {
        for core in system.cores {
            let name = format!("retroarch:{core}");
            if !wanted(&name) && !wanted("retroarch") {
                continue;
            }
            // A folder goes to a core as the `.scummvm` file in it.
            let content = if rom.is_dir() {
                match scummvm_file(&rom) {
                    Some(f) => f,
                    None => {
                        why_not.push(format!(
                            "{name}: no .scummvm file in the folder (import writes one)"
                        ));
                        continue;
                    }
                }
            } else {
                rom.clone()
            };
            match env.core(core) {
                Some(c) => {
                    let (config, mut prepare) = owned(&RETROARCH).unzip();
                    if let (Some(dir), Some(p)) = (&config, &mut prepare) {
                        owned::per_game::retroarch(conn, dir, core, &content, system, &game, p)?;
                    }
                    let vars = Vars {
                        rom: &content,
                        set: &set,
                        core: Some(&c),
                        config: config.as_deref(),
                        bios: None,
                        system: system.key,
                    };
                    let args = known_args(&RETROARCH, &vars, &env.options(&RETROARCH));
                    let uses_home = prepare.is_some();
                    return Ok(spec(name, &ra, args, prepare.unwrap_or_default(), uses_home));
                }
                None => why_not.push(format!("{name}: core not installed")),
            }
        }
    } else if !system.cores.is_empty() {
        why_not.push("RetroArch not found".into());
    }
    for name in system.emulators {
        let Some(e) = KNOWN_EMULATORS.iter().find(|e| e.name == *name) else {
            continue;
        };
        if !wanted(e.name) {
            continue;
        }
        if archived && !e.archives {
            why_not.push(format!("{} can't open archives", e.name));
            continue;
        }
        let ext = rom.extension().map(|x| x.to_string_lossy().to_ascii_lowercase());
        if let Some(ext) = ext.filter(|x| e.cannot_open.contains(&x.as_str())) {
            why_not.push(format!("{} can't open .{ext}", e.name));
            continue;
        }
        match env.known(e) {
            Some(r) => {
                let (config, mut prepare) = owned(e).unzip();
                if let (Some(dir), Some(p), "duckstation") = (&config, &mut prepare, e.name) {
                    owned::per_game::duckstation(conn, dir, system, &game, p)?;
                }
                // Its BIOS folder's flags only when the folder holds a file:
                // they override the emulator's own paths (a MAME rompath
                // with the machine's set in it), so not for nothing.
                let bios = env
                    .home
                    .as_ref()
                    .map(|h| h.bios().join(e.settings_name()))
                    .filter(|d| holds_file(d));
                let vars = Vars {
                    rom: &rom,
                    set: &set,
                    core: None,
                    config: config.as_deref(),
                    bios: bios.as_deref(),
                    system: system.key,
                };
                let args = known_args(e, &vars, &env.options(e));
                let uses_home = prepare.is_some() || (bios.is_some() && !e.bios.is_empty());
                return Ok(spec(
                    e.name.to_owned(),
                    &r,
                    args,
                    prepare.unwrap_or_default(),
                    uses_home,
                ));
            }
            None => why_not.push(format!("{} not installed", e.name)),
        }
    }
    if let Some(name) = emulator {
        let known = user_emulators(conn)?
            .iter()
            .any(|u| u.name.eq_ignore_ascii_case(name))
            || name.eq_ignore_ascii_case("retroarch")
            || system
                .cores
                .iter()
                .any(|c| name.eq_ignore_ascii_case(&format!("retroarch:{c}")))
            || system.emulators.iter().any(|e| name.eq_ignore_ascii_case(e));
        if !known {
            return Err(Error::Launch(format!(
                "{name:?} is not an emulator for {} (see `emulator list --all`)",
                system.key
            )));
        }
    }
    Err(Error::Launch(format!(
        "no emulator to start {game} [{}]: {}",
        system.key,
        if why_not.is_empty() {
            "none known for this system; add one with `emulator add`".to_owned()
        } else {
            why_not.join("; ")
        }
    )))
}

/// Whether `dir` holds a file, at any depth (a few levels at most).
pub(super) fn holds_file(dir: &Path) -> bool {
    fn walk(dir: &Path, depth: u8) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.filter_map(Result::ok).any(|e| {
            let p = e.path();
            p.is_file() || (depth > 0 && p.is_dir() && walk(&p, depth - 1))
        })
    }
    walk(dir, 8)
}

/// A user-added emulator's BIOS folder, `Bios/<name>/`, when its name is a
/// plain folder name.
fn user_bios(home: &crate::home::Home, name: &str) -> Option<PathBuf> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(std::path::Component::Normal(_)), None) => Some(home.bios().join(name)),
        _ => None,
    }
}

/// A sheet next to a loose track that lists it: the disc's own cue (or
/// gdi), when it differs from the DAT's and so didn't match.
fn own_sheet(track: &Path, is_sheet: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let dir = track.parent()?;
    let mut sheets: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_sheet(p))
        .collect();
    sheets.sort();
    sheets.into_iter().find(|s| match crate::disc::sheet_files(s) {
        Some(Ok(listed)) => listed.iter().any(|f| f == track),
        _ => false,
    })
}

/// The `.scummvm` file in a game's folder (its game id, for RetroArch).
fn scummvm_file(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("scummvm")))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// A playlist beside `disk` that lists it (import writes one per complete
/// multi-disk set): an `.m3u`, or np2kai's `.cmd` (a command line of
/// quoted disks).
fn playlist_for(disk: &Path) -> Option<PathBuf> {
    let dir = disk.parent()?;
    let mut lists: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("m3u") || e.eq_ignore_ascii_case("cmd"))
        })
        .collect();
    lists.sort();
    lists.into_iter().find(|l| {
        let Ok(text) = std::fs::read_to_string(l) else {
            return false;
        };
        let cmd = l.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
        let entries: Vec<&str> = if cmd {
            text.split('"').skip(1).step_by(2).collect()
        } else {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect()
        };
        entries.iter().any(|e| dir.join(e) == disk)
    })
}
