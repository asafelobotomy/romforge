//! Every system's DATs at once: one-file DATs are downloaded on several
//! threads (each host still at its own pace, [`fetch`]) and imported here
//! in order; TOSEC packs, serial lists and clone lists as before.

use super::{fetch_one, is_one_file};
use crate::job::Cancel;
use crate::{Error, ImportedDat, Library, fetch, systems};

impl Library {
    /// [`Library::download_dats`] for every system that has a DAT source,
    /// in one batch. `each` gets the system key too.
    pub fn download_all(
        &mut self,
        extras: bool,
        each: impl FnMut(&'static str, fetch::Source, Result<Vec<ImportedDat>, Error>),
    ) -> Result<(), Error> {
        self.download_all_with(extras, &Cancel::default(), |_, _| {}, each)
            .map(|_| ())
    }

    /// [`Library::download_all`] that can be stopped between sources (what
    /// is imported stays; `Ok(true)`), with `progress(done, total)` over
    /// every source of every system.
    pub fn download_all_with(
        &mut self,
        extras: bool,
        cancel: &Cancel,
        mut progress: impl FnMut(usize, usize),
        mut each: impl FnMut(&'static str, fetch::Source, Result<Vec<ImportedDat>, Error>),
    ) -> Result<bool, Error> {
        let all: Vec<(&'static str, fetch::Source)> = systems::SYSTEMS
            .iter()
            .flat_map(|s| fetch::sources(s, extras).into_iter().map(|src| (s.key, src)))
            .collect();
        let total = all.len();
        let threads = crate::parallel::threads(crate::settings::load(&self.conn)?.threads).min(4);
        self.batch(|lib| -> Result<bool, Error> {
            let mut done = 0;
            progress(0, total);
            // A few at a time, so only a few downloads wait in memory.
            for chunk in all.chunks(threads * 2) {
                if cancel.is_set() {
                    return Ok(true);
                }
                crate::parallel::map_ordered(
                    chunk.to_vec(),
                    threads,
                    |(key, source)| {
                        let body = is_one_file(source).then(|| fetch_one(&source.url().unwrap_or_default()));
                        (key, source, body)
                    },
                    |_, (key, source, body)| {
                        // One failing does not stop the rest.
                        let result = match body {
                            Some(Ok(b)) => lib.import_one(key, &source.url().unwrap_or_default(), &b),
                            Some(Err(e)) => Err(e),
                            None => lib.download_dat(key, source),
                        };
                        each(key, source, result);
                        done += 1;
                        progress(done, total);
                    },
                );
            }
            Ok(false)
        })
    }
}
