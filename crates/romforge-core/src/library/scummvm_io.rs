//! ScummVM's lists and MAME's own DAT.

use crate::{Error, ImportedDat, Library, fetch, launch, layout, scummvm};

/// What [`Library::scummvm_import`] kept: entries in the list, ScummVM
/// games found in the scanned folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ScummvmList {
    pub entries: usize,
    pub games: usize,
    /// Folders ScummVM's --detect named (including ones its list does).
    pub asked: usize,
}

impl Library {
    /// Keep ScummVM's detection list (as `scummvm
    /// --dump-all-detection-entries` writes it) in the databases folder and
    /// name the ScummVM games in every scanned folder by it.
    pub fn scummvm_import(&mut self, list: &[u8]) -> Result<ScummvmList, Error> {
        let entries = scummvm::parse(&String::from_utf8_lossy(list))?.len();
        let dir = self
            .extdb_dir()
            .ok_or_else(|| Error::Settings("an in-memory library keeps no ScummVM list".into()))?;
        let path = dir.join(layout::SCUMMVM_LIST);
        crate::atomic::write(&path, list)?;
        let games = scummvm::refresh(&self.conn, Some(&path))?;
        Ok(ScummvmList {
            entries,
            games,
            asked: 0,
        })
    }

    /// Import the installed MAME's own DAT (`mame -listxml`, made on this
    /// machine, never shipped) for the Arcade system; a newer MAME's
    /// replaces it.
    pub fn mame_dat(&mut self, env: &launch::Env) -> Result<ImportedDat, Error> {
        let (xml, version) = launch::mame_listxml(env)?;
        self.import_dat_bytes(
            &xml,
            &format!("mame -listxml ({version})"),
            Some(crate::systems::key!("Arcade")),
        )
    }

    /// Keep libretro's `ScummVM.dat` (its names for ScummVM game ids) in
    /// the databases folder, to title ScummVM games by. Returns how many
    /// game ids it names.
    pub fn scummvm_titles(&mut self, dat: &[u8]) -> Result<usize, Error> {
        let n = scummvm::titles(&String::from_utf8_lossy(dat))?.len();
        let dir = self
            .extdb_dir()
            .ok_or_else(|| Error::Settings("an in-memory library keeps no ScummVM titles".into()))?;
        crate::atomic::write(&dir.join(layout::SCUMMVM_TITLES), dat)?;
        let list = dir.join(layout::SCUMMVM_LIST);
        scummvm::refresh(&self.conn, Some(&list))?;
        Ok(n)
    }

    /// Download libretro's `ScummVM.dat` ([`Library::scummvm_titles`]).
    pub fn scummvm_titles_download(&mut self) -> Result<usize, Error> {
        let dat = fetch::get(&format!("{}/ScummVM.dat", fetch::LIBRETRO_DAT_BASE))?;
        self.scummvm_titles(&dat)
    }

    /// Ask the installed ScummVM for its detection list, then
    /// [`Library::scummvm_import`] it; then ask it what it finds in the
    /// scanned folders (`--detect`), for the engines its list leaves out.
    pub fn scummvm_read(&mut self, env: &launch::Env) -> Result<ScummvmList, Error> {
        let list = launch::scummvm_list(env)?;
        let mut r = self.scummvm_import(&list)?;
        let mut found = Vec::new();
        for root in scummvm::top_roots(&self.conn)? {
            found.extend(scummvm::parse_detect(&launch::scummvm_detect(env, &root)?, &root));
        }
        r.asked = scummvm::remember(&self.conn, &found)?;
        let list = self.extdb_dir().map(|d| d.join(layout::SCUMMVM_LIST));
        r.games = scummvm::refresh(&self.conn, list.as_deref())?;
        Ok(r)
    }
}
