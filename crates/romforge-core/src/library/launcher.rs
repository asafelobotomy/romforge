//! The library for uQ's launcher ([`crate::uq::launcher`]): games by name,
//! on the list the app shows.

use crate::uq::launcher::{self, Hit};
use crate::{Error, Library};

impl Library {
    /// The best games for what was typed in uQ's launcher (at most 20).
    pub fn launcher_search(&self, query: &str) -> Result<Vec<Hit>, Error> {
        if launcher::query(query).is_none() {
            return Ok(Vec::new());
        }
        Ok(launcher::search(&*self.cached_view()?, query))
    }

    /// The game a launcher row's id names, if Play may start it.
    pub fn launcher_game(&self, id: &str) -> Result<Option<Hit>, Error> {
        if !launcher::id_shape(id) {
            return Ok(None);
        }
        Ok(launcher::find(&*self.cached_view()?, id))
    }
}
