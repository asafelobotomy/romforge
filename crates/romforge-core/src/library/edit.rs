//! Quick edits beside a long job: their own connection, a short wait.

use std::path::Path;
use std::time::Duration;

use crate::{Error, Library};

/// How long a quick edit waits for another connection's write. Enrich,
/// cleanup and the like save item by item, well within it; a scan, a
/// refresh or a DAT import holds its write for the whole run.
pub const EDIT_WAIT: Duration = Duration::from_secs(2);

impl Library {
    /// Run `f` on a connection of its own to the database at `path`, for
    /// a quick edit (forgetting a match, picking art, saving settings)
    /// while a long job holds another connection. Its writes wait at most
    /// [`EDIT_WAIT`] for the job's; a job that holds the library longer
    /// gives [`Error::Busy`] instead of a longer wait.
    pub fn edit<T>(path: &Path, f: impl FnOnce(&mut Library) -> Result<T, Error>) -> Result<T, Error> {
        let quick = || {
            let mut lib = Library::open_at(path, false)?;
            lib.conn.busy_timeout(EDIT_WAIT)?;
            f(&mut lib)
        };
        quick().map_err(|e| if e.is_busy() { Error::Busy } else { e })
    }
}
