//! Scan folder, its dry run, and Refresh.

use std::path::{Path, PathBuf};

use crate::{
    Error, FolderScan, Library, PreviewGame, RefreshResult, ScanOptions, ScanPreview, ScanProgress, Scope,
    home, settings, view,
};

impl Library {
    /// The app's Scan folder: scan `root` (any folder; it isn't kept as a
    /// setting), except that the first folder scanned while no Source folder
    /// is set becomes the Source. Returns the report and whether it did.
    pub fn scan_folder(&mut self, root: impl AsRef<Path>) -> Result<FolderScan, Error> {
        self.scan_folder_with(root, &ScanOptions::default(), |_| {})
    }

    /// [`Library::scan_folder`] with options (threads, cancel) and progress.
    pub fn scan_folder_with(
        &mut self,
        root: impl AsRef<Path>,
        opts: &ScanOptions,
        progress: impl FnMut(ScanProgress),
    ) -> Result<FolderScan, Error> {
        let root = root.as_ref();
        let mut became_source = None;
        let mut s = settings::load(&self.conn)?;
        if s.source_dir.is_none() {
            s.source_dir = Some(root.to_owned());
            let home = self.home.is_some();
            became_source = Some(match settings::save_in(&mut self.conn, &s, home) {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            });
        }
        let scope = self.kept_scope(root)?;
        let report = self.scan_with(root, scope, opts, progress)?;
        Ok(FolderScan {
            report,
            became_source,
        })
    }

    /// Scan Folder's dry run: what scanning `root` would find, worked out on
    /// a copy of the library that is thrown away; the library itself is
    /// unchanged (scanning never touches ROM files either). The caller
    /// scans for real if the owner accepts (the files are read again then).
    pub fn scan_preview(&mut self, root: impl AsRef<Path>) -> Result<ScanPreview, Error> {
        self.scan_preview_with(root, &ScanOptions::default(), |_| {})
    }

    /// [`Library::scan_preview`] with options (threads, cancel) and progress.
    pub fn scan_preview_with(
        &mut self,
        root: impl AsRef<Path>,
        opts: &ScanOptions,
        progress: impl FnMut(ScanProgress),
    ) -> Result<ScanPreview, Error> {
        let root = std::fs::canonicalize(root.as_ref())?;
        let before: std::collections::HashSet<String> = view::library(&self.conn)?
            .rows
            .into_iter()
            .map(|r| r.id)
            .collect();
        let tmp = std::env::temp_dir().join(format!("romforge-preview-{}.db", std::process::id()));
        let clear = |p: &Path| {
            for ext in ["", "-wal", "-shm", "-journal", "-view.json"] {
                let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
            }
        };
        clear(&tmp);
        self.conn.execute("VACUUM INTO ?1", [tmp.to_string_lossy()])?;
        let result = (|| -> Result<ScanPreview, Error> {
            let mut copy = Library::open(&tmp)?;
            // The library's own lists (ScummVM's), read where they are.
            copy.databases = self.extdb_dir();
            let scope = self.kept_scope(&root)?;
            let report = copy.scan_with(&root, scope, opts, progress)?;
            let under: std::collections::HashSet<i64> = copy
                .conn
                .prepare("SELECT file_id, path FROM files")?
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .filter(|r| r.as_ref().map_or(true, |(_, p)| Path::new(p).starts_with(&root)))
                .map(|r| r.map(|(id, _)| id))
                .collect::<Result<_, _>>()?;
            let mut games: Vec<PreviewGame> = view::library(&copy.conn)?
                .rows
                .into_iter()
                // A ScummVM game has no rows for its files: by its place.
                .filter(|r| {
                    r.file_ids().iter().any(|f| under.contains(f))
                        || r.folder
                            .as_deref()
                            .is_some_and(|d| Path::new(d).starts_with(&root))
                })
                .map(|r| PreviewGame {
                    new: !before.contains(&r.id),
                    title: r.label.clone().unwrap_or(r.title),
                    system: r.system_name,
                    status: r.status,
                })
                .collect();
            games.sort_by(|a, b| (a.status, &a.title).cmp(&(b.status, &b.title)));
            Ok(ScanPreview { report, games })
        })();
        clear(&tmp);
        result
    }

    /// The scope Scan folder uses for `root`: the one it (or the deepest
    /// scanned folder holding it) was scanned with, so a folder scanned for
    /// one system or DAT isn't widened by scanning it again; else all DATs.
    fn kept_scope(&self, root: &Path) -> Result<Scope, Error> {
        let Ok(root) = std::fs::canonicalize(root) else {
            return Ok(Scope::All);
        };
        let mut known: Vec<(String, Option<i64>, Option<i64>)> = self
            .conn
            .prepare("SELECT path, system_id, dat_id FROM scan_roots")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        known.retain(|(p, _, _)| root.starts_with(p));
        Ok(known
            .into_iter()
            .max_by_key(|(p, _, _)| p.len())
            .map_or(Scope::All, |(_, s, d)| Scope::from_columns(s, d)))
    }

    /// Rescan every folder romforge knows (the Source and Library folders of
    /// the settings, and every scanned folder that isn't inside another),
    /// each with the scope it was scanned with. Folders that are gone are
    /// reported, not forgotten.
    pub fn refresh(
        &mut self,
        opts: &ScanOptions,
        progress: impl FnMut(&Path, ScanProgress),
    ) -> Result<Vec<RefreshResult>, Error> {
        self.refresh_roots(true, opts, progress)
    }

    /// The app's Refresh: rescan only the folders Settings names (Source and
    /// Library) and the RomForge folder's Games, not folders scanned once
    /// with Scan folder.
    pub fn refresh_settings(
        &mut self,
        opts: &ScanOptions,
        progress: impl FnMut(&Path, ScanProgress),
    ) -> Result<Vec<RefreshResult>, Error> {
        self.refresh_roots(false, opts, progress)
    }

    /// Refresh as the app and CLI run it: first the lists games are
    /// identified by ([`Library::update_lists`]), then the folders
    /// (`all`: every folder ever scanned, else the Settings ones,
    /// [`Library::refresh_settings`]), then the online lists of systems
    /// this scan found first. `progress` follows all of it, in
    /// thousandths: the lists a fifth, the folders the rest.
    pub fn refresh_with_lists(
        &mut self,
        all: bool,
        opts: &ScanOptions,
        lists: &crate::ListOptions,
        mut progress: impl FnMut(crate::job::Step),
    ) -> Result<(crate::ListsReport, Vec<RefreshResult>), Error> {
        use crate::job::Step;
        let before = super::lists::systems_with_files(&self.conn)?;
        let mut report = self.update_lists(lists, |s| {
            progress(Step::new(0, 1000, format!("Lists: {}", s.detail)))
        })?;
        opts.cancel.check()?;
        let results = self.refresh_roots(all, opts, |_, p| {
            let s = p.step();
            let done = 200 + 800 * s.done / s.total.max(1);
            progress(Step::new(done, 1000, s.detail))
        })?;
        opts.cancel.check()?;
        self.update_new_lists(&before, lists, &mut report, |s| {
            progress(Step::new(1000, 1000, format!("Lists: {}", s.detail)))
        })?;
        Ok((report, results))
    }

    /// Whether Refresh has a folder to rescan (a Source or Library folder,
    /// or the RomForge folder).
    pub fn can_refresh(&self) -> Result<bool, Error> {
        let s = settings::load(&self.conn)?;
        Ok(s.source_dir.is_some() || s.library_dir.is_some() || self.home.is_some())
    }

    fn refresh_roots(
        &mut self,
        all: bool,
        opts: &ScanOptions,
        mut progress: impl FnMut(&Path, ScanProgress),
    ) -> Result<Vec<RefreshResult>, Error> {
        let known: Vec<(String, Option<i64>, Option<i64>)> = self
            .conn
            .prepare("SELECT path, system_id, dat_id FROM scan_roots ORDER BY length(path), path")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        let s = settings::load(&self.conn)?;
        let wanted: Vec<String> = [
            s.source_dir.clone(),
            s.library_dir.clone(),
            self.home.as_ref().map(home::Home::games),
        ]
        .into_iter()
        .flatten()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
        // Everything scanned before, or (Settings only) just those folders
        // with the scope they were scanned with.
        let mut roots: Vec<(String, Option<i64>, Option<i64>)> = known
            .into_iter()
            .filter(|r| all || wanted.contains(&r.0))
            .collect();
        // The Source and Library folders (and the RomForge folder's Games)
        // are scanned even before their first scan, with no scope.
        for dir in wanted {
            if !roots.iter().any(|r| r.0 == dir) {
                roots.push((dir, None, None));
            }
        }
        roots.sort_by(|a, b| (a.0.len(), &a.0).cmp(&(b.0.len(), &b.0)));
        let mut top: Vec<&(String, Option<i64>, Option<i64>)> = Vec::new();
        for r in &roots {
            let inside = top
                .iter()
                .any(|t| Path::new(&r.0).starts_with(&t.0) && r.0 != t.0);
            if !inside {
                top.push(r);
            }
        }
        let mut out = Vec::new();
        let folders = top.len();
        for (folder, (path, system_id, dat_id)) in top.into_iter().enumerate() {
            let path = PathBuf::from(path);
            if !path.exists() {
                out.push(Err((path, "folder not found".into())));
                continue;
            }
            let scope = Scope::from_columns(*system_id, *dat_id);
            let at = |p: ScanProgress| ScanProgress { folder, folders, ..p };
            match self.scan_with(&path, scope, opts, |p| progress(&path, at(p))) {
                Ok(r) => out.push(Ok(r)),
                // Stopped: the folders after it aren't scanned either.
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(e) => out.push(Err((path, e.to_string()))),
            }
        }
        Ok(out)
    }
}
