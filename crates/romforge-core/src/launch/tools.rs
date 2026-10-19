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
    let (program, args) = command(&runner, args.to_vec(), grant);
    let out = std::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| Error::Launch(format!("{}: {e}", program.display())))?;
    Ok((program, out))
}

/// The installed MAME's own DAT (`-listxml`: every machine with its ROMs'
/// and disks' SHA1) and its version (`-version`).
pub fn mame_listxml(env: &Env) -> Result<(Vec<u8>, String), Error> {
    let (_, v) = run_known(env, "mame", "MAME", &["-version".to_owned()], None)?;
    let version = String::from_utf8_lossy(&v.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("?")
        .to_owned();
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
    let mut child = std::process::Command::new(&spec.program)
        .args(&spec.args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::Launch(format!("{}: {e}", spec.program.display())))?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(pid)
}
