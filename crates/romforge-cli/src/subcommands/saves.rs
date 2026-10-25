//! `saves` (milestone 27: in-game saves in `Saves/<system>/`).

use clap::Subcommand;

#[derive(Subcommand)]
pub enum SavesCommand {
    /// Move an emulator's saves from the places it used before into
    /// Saves/<system>/ (shows the plan unless --apply; refused while it
    /// runs). A save whose place in Saves/ is taken stays where it is.
    Gather {
        emulator: String,
        /// Move them now (journaled)
        #[arg(long)]
        apply: bool,
        /// Put the last gathering back instead
        #[arg(long)]
        undo: bool,
    },
}
