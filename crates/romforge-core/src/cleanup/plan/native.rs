//! Milestone 23: the compressed format a disc platform's emulators read
//! when it isn't CHD: RVZ (GameCube, Dolphin), Z3DS (3DS, Azahar, decrypted
//! games only), WUA (a Wii U title folder, Cemu). Decided from the files
//! and their plain headers only.

use std::path::{Path, PathBuf};

use super::super::parts::open_part;
use super::super::{Format, Part};
use crate::systems::key;

/// Why an eShop download (or, for organize, any Wii U title folder) is
/// left where it is.
pub(crate) const TITLE_FOLDER_STAYS: &str = "a Wii U title folder (code/, content/, meta/, or an eShop download) stays as it is: Cemu reads the whole folder";

/// An eShop download's `title.tmd`: its contents are encrypted, so it stays.
pub(super) fn is_eshop(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("title.tmd"))
}

/// What a disc game becomes.
pub(super) enum Native {
    /// CHD (the platform's emulators read it): decided by the caller.
    Chd,
    Rvz,
    /// Z3DS, with the new file's extension (`zcci`, `zcia`, ...).
    Z3ds(String),
    /// A WUA of this title folder.
    Wua(PathBuf),
}

impl Native {
    /// Its format (none for CHD: the caller picks CD or DVD), the new
    /// file's extension, and the title folder a WUA is made of.
    pub(super) fn split(self) -> (Option<Format>, String, Option<PathBuf>) {
        match self {
            Native::Rvz => (Some(Format::Rvz), "rvz".to_owned(), None),
            Native::Z3ds(e) => (Some(Format::Z3ds), e, None),
            Native::Wua(dir) => (Some(Format::Wua), "wua".to_owned(), Some(dir)),
            Native::Chd => (None, String::new(), None),
        }
    }
}

fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map_or(String::new(), |(_, e)| e.to_ascii_lowercase())
}

/// The format for `system`'s game made of `parts`, or why it stays.
pub(super) fn native(system: &crate::systems::System, parts: &[Part]) -> Result<Native, String> {
    if system.extensions.contains(&"chd") {
        return Ok(Native::Chd);
    }
    let stays = || format!("{} emulators don't read CHD; it stays as it is", system.key);
    let [part] = parts else {
        return Err(stays());
    };
    let e = ext(&part.name);
    const GAMECUBE: &str = key!("GameCube");
    const N3DS: &str = key!("Nintendo 3DS");
    const WII_U: &str = key!("Wii U");
    match system.key {
        GAMECUBE if matches!(e.as_str(), "iso" | "gcm") => Ok(Native::Rvz),
        GAMECUBE if e == "rvz" => Err("it is an RVZ already".into()),
        N3DS if matches!(e.as_str(), "3ds" | "cci" | "cxi" | "cia" | "3dsx") => {
            // Inside a 7z the header can't be read without unpacking.
            let zip = part
                .path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("zip"));
            if !part.member.is_empty() && !zip {
                return Err("a 3DS game inside a 7z isn't compressed: unpack it first".into());
            }
            // A zip member stored with an unusual method is unpacked to a
            // folder of its own while its header is read, then removed.
            let staging = std::env::temp_dir().join(format!("romforge-header-{}", std::process::id()));
            let mut head = vec![0u8; crate::ident::PREFIX_LEN];
            let read = open_part(part, &staging)
                .map_err(|e| e.to_string())
                .and_then(|mut r| crate::chunks::read_full(&mut r, &mut head).map_err(|e| e.to_string()));
            let _ = std::fs::remove_dir_all(&staging);
            let read = read?;
            match crate::ident::z3ds_form(&head[..read]) {
                // Azahar's own names: a card image (`.3ds` or `.cci`) is
                // `.zcci`.
                Some(f) if f.decrypted => Ok(Native::Z3ds(if e == "3ds" {
                    "zcci".into()
                } else {
                    format!("z{e}")
                })),
                Some(_) => Err(
                    "encrypted: Azahar, the only emulator that reads Z3DS, plays decrypted games only".into(),
                ),
                None => Err("its 3DS header isn't one romforge reads".into()),
            }
        }
        N3DS if e.starts_with('z') => Err("it is a Z3DS already".into()),
        WII_U if e == "rpx" && part.member.is_empty() && crate::ident::wup::in_title_folder(&part.path) => {
            let dir = part
                .path
                .parent()
                .and_then(Path::parent)
                .ok_or_else(stays)?
                .to_path_buf();
            let meta = std::fs::read_to_string(dir.join("meta/meta.xml"))
                .map_err(|e| format!("its meta/meta.xml: {e}"))?;
            crate::ident::wup::wua_folder(&meta).ok_or("its meta.xml gives no title ID and version")?;
            // Cemu reads such a name from a folder, not from a WUA: the
            // folder is kept as it plays.
            let long = walkdir::WalkDir::new(&dir)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .find(|e| e.file_name().len() > crate::wua::MAX_NAME);
            if let Some(e) = long {
                return Err(format!(
                    "{} has a name longer than a WUA can hold ({} bytes, Cemu reads at most {}): it stays a folder",
                    e.path().strip_prefix(&dir).unwrap_or(e.path()).display(),
                    e.file_name().len(),
                    crate::wua::MAX_NAME
                ));
            }
            Ok(Native::Wua(dir))
        }
        _ => Err(stays()),
    }
}
