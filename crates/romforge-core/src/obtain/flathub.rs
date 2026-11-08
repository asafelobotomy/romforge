//! Installing an emulator's official Flatpak into the user's Flatpak
//! installation: `flatpak remote-add --user --if-not-exists <remote> …`,
//! then `flatpak install --user --noninteractive <remote> <id>`, from
//! Flathub or its developers' own repository (`FlatpakFrom`), or, for an
//! emulator whose developers make no Linux build, Flathub's as the
//! fallback. One only others build (Flathub's RPCS3, mGBA, Flycast) is
//! installed only when asked, its official AppImage being recommended. The
//! ids are the table's.

use crate::Error;
use crate::launch::{self, Env, FlatpakFrom};

const FLATHUB_REPO: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

/// Install emulator `name`'s official Flatpak (as `emulator list` names
/// it, `retroarch` included). Returns its Flatpak id.
pub fn install_flatpak(env: &Env, name: &str) -> Result<String, Error> {
    let e = launch::known(name).ok_or_else(|| Error::Launch(format!("no emulator named {name:?}")))?;
    if let Some(last) = e.discontinued {
        return Err(Error::Launch(format!(
            "{} is discontinued (last release: {last}): romforge doesn't install it, it uses yours",
            e.label
        )));
    }
    let id = e
        .flatpak
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::Launch(format!("{} has no Flatpak", e.label)))?;
    let (remote, repo) = match e.flatpak_from {
        FlatpakFrom::Flathub | FlatpakFrom::Fallback => ("flathub", FLATHUB_REPO),
        FlatpakFrom::Own { remote, repo, .. } => (remote, repo),
        // Made by others, not its developers: installed only when asked
        // (the Emulators page offers its official AppImage first).
        FlatpakFrom::Others => ("flathub", FLATHUB_REPO),
    };
    let flatpak = env
        .path
        .iter()
        .map(|d| d.join("flatpak"))
        .find(|p| p.is_file())
        .ok_or_else(|| Error::Launch("Flatpak isn't installed on this computer".into()))?;
    let run = |args: &[&str]| -> Result<(), Error> {
        let out = std::process::Command::new(&flatpak)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| Error::Launch(format!("flatpak: {e}")))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        Err(Error::Launch(format!(
            "flatpak {} failed ({}): {last}",
            args[0], out.status
        )))
    };
    run(&["remote-add", "--user", "--if-not-exists", remote, repo])?;
    // One from elsewhere (Flathub's community Dolphin) is replaced by its
    // developers' own.
    let mut install = vec!["install", "--user", "--noninteractive", "-y"];
    if env.unofficial_flatpak(e) && matches!(e.flatpak_from, FlatpakFrom::Own { .. }) {
        install.push("--reinstall");
    }
    install.extend([remote, id]);
    run(&install)?;
    Ok(id.to_owned())
}
