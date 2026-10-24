//! An emulator's official AppImage, from its own project's GitHub releases
//! (`KnownEmulator::official_appimage`; RPCS3's), into the RomForge
//! folder's `Emulators/`, where romforge looks for AppImages. The release's
//! newest asset whose name ends as the table says is downloaded, checked
//! against the SHA-256 GitHub publishes for it, and made executable.

use std::path::PathBuf;

use crate::Error;
use crate::home::Home;
use crate::launch;

/// The biggest AppImage downloaded.
const MAX: u64 = 2 << 30;

/// Download `name`'s official AppImage into `Emulators/`. Returns where.
pub fn install_appimage(home: &Home, name: &str) -> Result<PathBuf, Error> {
    let e = launch::known(name).ok_or_else(|| Error::Launch(format!("no emulator named {name:?}")))?;
    let (repo, suffix) = e
        .official_appimage
        .ok_or_else(|| Error::Launch(format!("{} has no official AppImage romforge knows", e.label)))?;
    let body = crate::fetch::get(&format!("https://api.github.com/repos/{repo}/releases/latest"))?;
    let release: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|err| Error::Launch(format!("{}'s release list: {err}", e.label)))?;
    let asset = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(suffix)))
        .ok_or_else(|| Error::Launch(format!("{}'s latest release has no {suffix}", e.label)))?;
    let file = asset["name"].as_str().unwrap_or_default();
    let url = asset["browser_download_url"].as_str().unwrap_or_default();
    // Only a plain name, into Emulators/ and nowhere else.
    if file.is_empty()
        || file.contains('/')
        || file.starts_with('.')
        || !url.starts_with("https://github.com/")
    {
        return Err(Error::Launch(format!("{}'s release asset looks wrong", e.label)));
    }
    let digest = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or_else(|| Error::Launch(format!("{}'s release gives no SHA-256 to check it by", e.label)))?
        .to_ascii_lowercase();
    let dest = home.emulators().join(file);
    if dest.exists() {
        return Ok(dest); // the latest is there already
    }
    let part = dest.with_extension("AppImage.download");
    crate::fetch::download_to(url, &part, MAX, |_, _| {})?;
    let got = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        let mut f = std::fs::File::open(&part)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = std::io::Read::read(&mut f, &mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        crate::hash::hex(&h.finalize())
    };
    if got != digest {
        let _ = std::fs::remove_file(&part);
        return Err(Error::Launch(format!(
            "{file} didn't download whole (its SHA-256 doesn't match)"
        )));
    }
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}
