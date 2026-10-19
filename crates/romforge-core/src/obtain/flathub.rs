//! Installing an emulator from Flathub into the user's Flatpak
//! installation: `flatpak remote-add --user --if-not-exists flathub …`,
//! then `flatpak install --user --noninteractive flathub <id>`. The ids are
//! the table's, checked on Flathub.

use crate::Error;
use crate::launch::{self, Env};

const FLATHUB_REPO: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

/// Install emulator `name` (as `emulator list` names it, `retroarch`
/// included) from Flathub. Returns its Flatpak id.
pub fn install_flatpak(env: &Env, name: &str) -> Result<String, Error> {
    let e = launch::known(name).ok_or_else(|| Error::Launch(format!("no emulator named {name:?}")))?;
    let id = e
        .flatpak
        .ok_or_else(|| Error::Launch(format!("{} isn't on Flathub", e.label)))?;
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
    run(&["remote-add", "--user", "--if-not-exists", "flathub", FLATHUB_REPO])?;
    run(&["install", "--user", "--noninteractive", "-y", "flathub", id])?;
    Ok(id.to_owned())
}
