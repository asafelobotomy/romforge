//! Which kinds of DAT the table names for a system.

use super::System;

impl System {
    /// The DAT families the table names for it, in the table's order:
    /// `no-intro`, `redump`, `tosec`, `hacks`, `homebrew`.
    pub fn dat_sources(&self) -> Vec<&'static str> {
        [
            (!self.nointro.is_empty(), "no-intro"),
            (!self.redump.is_empty(), "redump"),
            (!self.tosec.is_empty(), "tosec"),
            (!self.hacks.is_empty(), "hacks"),
            (!self.homebrew.is_empty(), "homebrew"),
        ]
        .into_iter()
        .filter_map(|(has, tag)| has.then_some(tag))
        .collect()
    }
}
