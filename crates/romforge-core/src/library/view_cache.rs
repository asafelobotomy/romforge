//! The library view and its caches.

use std::path::PathBuf;

use crate::{Error, Library, view};

/// The list view kept on disk beside the database
/// (`<db>-view.json`), so opening the library doesn't build it again when
/// nothing changed: `key` says which generation of which database, built
/// by which romforge.
#[derive(serde::Serialize, serde::Deserialize)]
struct ViewSnapshot {
    key: String,
    view: view::LibraryView,
    inner: Vec<view::RowInner>,
}

impl Library {
    /// One row per game (unidentified files as their own rows) and the
    /// platforms they're on, for the app's library screen.
    pub fn library_view(&self) -> Result<view::LibraryView, Error> {
        Ok((*self.cached_view()?).clone())
    }

    /// Everything known about one row of [`Library::library_view`].
    pub fn game_detail(&self, id: &str) -> Result<view::GameDetail, Error> {
        let v = self.cached_view()?;
        view::detail_in(&self.conn, &v, id)
    }

    /// The list view's generation: it changes only when something the list
    /// shows does ([`view::fingerprint`]), so an app can keep the view it
    /// has until then.
    pub fn view_generation(&self) -> Result<i64, Error> {
        // 52 bits: a JavaScript number holds it exactly.
        Ok((self.view_key()?.1 & ((1 << 52) - 1)) as i64)
    }

    /// (what SQLite says about writes, the view's fingerprint): the
    /// fingerprint is worked out again only after a write.
    fn view_key(&self) -> Result<((i64, u64), u64), Error> {
        let version: i64 = self.conn.pragma_query_value(None, "data_version", |r| r.get(0))?;
        let writes = (version, self.conn.total_changes());
        {
            let cache = self.view_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((w, f, _)) = cache.as_ref()
                && *w == writes
            {
                return Ok((writes, *f));
            }
        }
        Ok((writes, view::fingerprint(&self.conn)?))
    }

    /// The library view if its generation isn't `known` (with the
    /// generation either way): an app asks again after each action and
    /// gets the whole view only when it changed.
    pub fn library_view_since(&self, known: Option<i64>) -> Result<(i64, Option<view::LibraryView>), Error> {
        let generation = self.view_generation()?;
        if known == Some(generation) {
            return Ok((generation, None));
        }
        let v = self.cached_view()?;
        Ok((generation, Some((*v).clone())))
    }

    /// The library view built afresh, ignoring every cache (to check them).
    pub fn library_view_fresh(&self) -> Result<view::LibraryView, Error> {
        view::listing(&self.conn)
    }

    /// The library view, built again only when its fingerprint changed:
    /// from memory, else the snapshot beside the database, else built (and
    /// the snapshot written).
    pub(super) fn cached_view(&self) -> Result<std::sync::Arc<view::LibraryView>, Error> {
        let (writes, fingerprint) = self.view_key()?;
        let mut cache = self.view_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, f, v)) = cache.as_ref()
            && *f == fingerprint
        {
            let v = v.clone();
            *cache = Some((writes, fingerprint, v.clone()));
            return Ok(v);
        }
        let key = self.snapshot_key(fingerprint as i64);
        let stored = self.view_snapshot().and_then(|p| {
            let snap: ViewSnapshot = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
            let mut v = snap.view;
            (Some(&snap.key) == key.as_ref() && v.restore(snap.inner)).then_some(v)
        });
        let v = match stored {
            Some(v) => std::sync::Arc::new(v),
            None => {
                let v = std::sync::Arc::new(view::listing(&self.conn)?);
                if let (Some(path), Some(key)) = (self.view_snapshot(), key) {
                    // A snapshot that can't be written only costs a rebuild.
                    let snap = ViewSnapshot {
                        key,
                        view: (*v).clone(),
                        inner: v.inner(),
                    };
                    if let Ok(bytes) = serde_json::to_vec(&snap) {
                        let _ = crate::atomic::write(&path, &bytes);
                    }
                }
                v
            }
        };
        *cache = Some((writes, fingerprint, v.clone()));
        Ok(v)
    }

    /// Where the view's snapshot lives: `<db>-view.json`. None in memory.
    fn view_snapshot(&self) -> Option<PathBuf> {
        let p = self.path.as_ref()?;
        Some(PathBuf::from(format!("{}-view.json", p.display())))
    }

    /// What a snapshot must say to be used: the generation, this database,
    /// and this build of romforge (its program file's time), since a new
    /// romforge may list things differently.
    fn snapshot_key(&self, generation: i64) -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        let built = exe
            .metadata()
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some(format!(
            "{generation}|{}|{}|{built}|{}",
            self.path.as_ref()?.display(),
            exe.display(),
            env!("CARGO_PKG_VERSION")
        ))
    }
}
