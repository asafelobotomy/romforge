use super::*;

/// ScummVM's detection list, from the installed ScummVM (on `PATH` or
/// Flatpak): `--dump-all-detection-entries`, which prints it and exits.
pub fn scummvm_list(env: &Env) -> Result<Vec<u8>, Error> {
    let (program, out) = run_scummvm(env, &["--dump-all-detection-entries".to_owned()], None)?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(Error::Launch(format!(
            "{} gave no detection list ({}); it needs a ScummVM with --dump-all-detection-entries",
            program.display(),
            out.status
        )));
    }
    Ok(out.stdout)
}

/// What the installed ScummVM finds in `root` and every folder below it
/// (`--detect --recursive`), as it prints it.
pub fn scummvm_detect(env: &Env, root: &Path) -> Result<String, Error> {
    let args = [
        "--detect".to_owned(),
        "--recursive".to_owned(),
        format!("--path={}", root.display()),
    ];
    let (_, out) = run_scummvm(env, &args, Some(root))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_scummvm(
    env: &Env,
    args: &[String],
    grant: Option<&Path>,
) -> Result<(PathBuf, std::process::Output), Error> {
    run_known(env, "scummvm", "ScummVM", args, grant)
}

/// Run a known emulator to completion (not a game: a list it prints).
fn run_known(
    env: &Env,
    name: &str,
    shown: &str,
    args: &[String],
    grant: Option<&Path>,
) -> Result<(PathBuf, std::process::Output), Error> {
    let e = KNOWN_EMULATORS
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| Error::Launch(format!("{name} is not a known emulator")))?;
    let runner = env
        .known(e)
        .ok_or_else(|| Error::Launch(format!("{shown} isn't installed (on PATH or as a Flatpak)")))?;
    let grants: Vec<PathBuf> = grant.map(Path::to_path_buf).into_iter().collect();
    let (program, args, env) = command(&runner, args.to_vec(), &grants, &[]);
    let mut cmd = std::process::Command::new(&program);
    cmd.args(&args)
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let out =
        output_within(cmd, RUN_LIMIT).map_err(|e| Error::Launch(format!("{}: {e}", program.display())))?;
    Ok((program, out))
}

/// How long a helper run (a list, a version) gets before it is stopped:
/// MAME's whole `-listxml` or ScummVM detecting a big folder takes a
/// while, a hung one takes for ever.
const RUN_LIMIT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Run `cmd` to completion with its output read, killing it if it hasn't
/// ended within `limit` (a `TimedOut` error then).
pub(super) fn output_within(
    mut cmd: std::process::Command,
    limit: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    let mut child = cmd.stdout(std::process::Stdio::piped()).spawn()?;
    let mut stdout = child.stdout.take();
    // Read as it comes, so a long output doesn't fill the pipe and stall it.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let until = std::time::Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("no answer after {} s: stopped", limit.as_secs()),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    Ok(std::process::Output {
        status,
        stdout: reader.join().unwrap_or_default(),
        stderr: Vec::new(),
    })
}

/// Whether the known emulator `name` is installed (Settings path, `PATH`,
/// Flatpak, romforge's Emulators folder).
pub fn installed(env: &Env, name: &str) -> bool {
    KNOWN_EMULATORS
        .iter()
        .find(|e| e.name == name)
        .is_some_and(|e| env.known(e).is_some())
}

/// The installed ScummVM's build, as `--version` says it ("ScummVM
/// 2026.3.0 (Oct 2 2026 12:00:00)"): what its list was made by. Its first
/// run prints "Default configuration file missing, creating a new one"
/// before that: the line naming ScummVM is taken.
pub fn scummvm_version(env: &Env) -> Result<String, Error> {
    let (_, out) = run_scummvm(env, &["--version".to_owned()], None)?;
    scummvm_version_in(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| Error::Launch("ScummVM --version gave no version".into()))
}

pub(crate) fn scummvm_version_in(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with("ScummVM ") && version_in(l).is_some())
        .map(str::to_owned)
}

/// The installed MAME's version (`-version`). A wrapper script may print a
/// line of its own first ("Running MAME for the first time..."): the first
/// word that looks like a version ("0.289") is taken.
pub fn mame_version(env: &Env) -> Result<String, Error> {
    let (_, v) = run_known(env, "mame", "MAME", &["-version".to_owned()], None)?;
    version_in(&String::from_utf8_lossy(&v.stdout))
        .ok_or_else(|| Error::Launch("MAME -version gave no version".into()))
}

/// The first word that is digits and dots with at least one dot.
pub(crate) fn version_in(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.'))
        .find(|w| {
            w.contains('.')
                && w.chars().next().is_some_and(|c| c.is_ascii_digit())
                && w.split('.')
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_owned)
}

/// The installed MAME's own DAT (`-listxml`: every machine with its ROMs'
/// and disks' SHA1) and its version (`-version`).
pub fn mame_listxml(env: &Env) -> Result<(Vec<u8>, String), Error> {
    let version = mame_version(env).unwrap_or_else(|_| "?".to_owned());
    let (program, out) = run_known(env, "mame", "MAME", &["-listxml".to_owned()], None)?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(Error::Launch(format!(
            "{} -listxml gave nothing ({})",
            program.display(),
            out.status
        )));
    }
    Ok((out.stdout, version))
}

/// Start the emulator and return its process id. romforge doesn't wait for
/// it, but a thread reaps it when it exits so no zombie is left behind.
pub fn spawn(spec: &LaunchSpec) -> Result<u32, Error> {
    prepare(&spec.prepare)?;
    let mut child = std::process::Command::new(&spec.program)
        .args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::Launch(format!("{}: {e}", spec.program.display())))?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(pid)
}

/// Install PS3 firmware into RPCS3 as romforge runs it (milestone 20):
/// `rpcs3 --installfw <Bios/rpcs3/PS3UPDAT.PUP> --headless`, with romforge's
/// config, so it lands in the `dev_flash` that config names (the user's own
/// RPCS3's, when romforge points at it). RPCS3 exits with 0 even when the
/// install fails (its errors go to its log), so the firmware's main module
/// is looked for afterwards. Returns the `dev_flash` folder.
pub fn install_ps3_firmware(env: &Env) -> Result<PathBuf, Error> {
    let home = env.home.as_ref().ok_or_else(|| {
        Error::Settings("romforge sets RPCS3 up in a RomForge folder; create one first".into())
    })?;
    let pup = home.bios().join("rpcs3/PS3UPDAT.PUP");
    if !pup.is_file() {
        return Err(Error::Launch(format!(
            "no PS3 firmware at {}: drop your PS3UPDAT.PUP on the BIOS page first",
            pup.display()
        )));
    }
    let e = known("rpcs3").ok_or_else(|| Error::Launch("RPCS3 is not in the table".into()))?;
    let runner = env
        .known(e)
        .ok_or_else(|| Error::Launch("RPCS3 isn't installed".into()))?;
    let system =
        systems::by_key("PlayStation 3").ok_or_else(|| Error::UnknownSystem("PlayStation 3".into()))?;
    let (dir, prep) = owned::owned(e, home, system, &env.options(e), env.user.as_ref())
        .ok_or_else(|| Error::Launch("romforge doesn't set RPCS3 up".into()))?;
    prepare(&prep)?;
    let flash = prep
        .writes
        .iter()
        .flat_map(|w| &w.keys)
        .find(|(k, _)| k == "/dev_flash/")
        .map(|(_, v)| PathBuf::from(v.trim_matches('"')))
        .unwrap_or_else(|| dir.join("rpcs3/dev_flash"));
    let args = vec![
        "--installfw".to_owned(),
        pup.to_string_lossy().into_owned(),
        "--headless".to_owned(),
    ];
    let grants = vec![home.root().to_path_buf(), flash.clone()];
    let (program, args, envs) = command(&runner, args, &grants, &prep.env);
    let out = std::process::Command::new(&program)
        .args(&args)
        .envs(envs)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| Error::Launch(format!("{}: {e}", program.display())))?;
    if !out.status.success() || !flash.join("vsh/module/vsh.self").is_file() {
        let log = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = log.lines().rev().take(3).collect();
        return Err(Error::Launch(format!(
            "RPCS3 didn't install the firmware into {} ({}){}",
            flash.display(),
            out.status,
            if tail.is_empty() {
                String::new()
            } else {
                format!(": {}", tail.join(" / "))
            }
        )));
    }
    Ok(flash)
}

/// Have MAME write every one of its settings into romforge's `mame.ini`
/// (`-createconfig`, which writes the file in the folder it runs in, with
/// the values it read from it kept), for All settings to list.
pub fn mame_write_defaults(env: &Env) -> Result<PathBuf, Error> {
    let home = env
        .home
        .as_ref()
        .ok_or_else(|| Error::Launch("MAME's settings are kept in a RomForge folder: none is set".into()))?;
    let e = known("mame").ok_or_else(|| Error::Launch("MAME is not in the table".into()))?;
    if running(e) {
        return Err(Error::Launch("MAME is running: close it first".into()));
    }
    let runner = env
        .known(e)
        .ok_or_else(|| Error::Launch("MAME isn't installed".into()))?;
    let (dir, prep) = owned::managed(e, home, &env.options(e), env.user.as_ref())
        .ok_or_else(|| Error::Launch("romforge doesn't set MAME up".into()))?;
    prepare(&prep)?;
    let args = vec![
        "-inipath".to_owned(),
        dir.to_string_lossy().into_owned(),
        "-createconfig".to_owned(),
    ];
    let (program, args, envs) = command(&runner, args, std::slice::from_ref(&dir), &prep.env);
    let mut cmd = std::process::Command::new(&program);
    cmd.args(&args)
        .envs(envs)
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let out =
        output_within(cmd, RUN_LIMIT).map_err(|e| Error::Launch(format!("{}: {e}", program.display())))?;
    let ini = dir.join("mame.ini");
    if !out.status.success() || !ini.is_file() {
        return Err(Error::Launch(format!(
            "MAME didn't write {} ({})",
            ini.display(),
            out.status
        )));
    }
    Ok(ini)
}
