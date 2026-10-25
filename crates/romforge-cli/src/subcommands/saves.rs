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
    /// Copy an emulator's saves that stay outside Saves/ (RPCS3, Cemu, the
    /// 3DS emulators on your own SD card, Dolphin's Wii NAND) into
    /// Saves/<system>/backup/ (each changed one; also done after each game)
    Backup { emulator: String },
    /// List an emulator's backed-up save folders and their copies
    Backups { emulator: String },
    /// Put a save folder back from its copy (shows the plan unless
    /// --apply; the save there now goes to the trash)
    Restore {
        emulator: String,
        /// The save folder, as `saves backups` lists it
        #[arg(required_unless_present = "undo")]
        save: Option<std::path::PathBuf>,
        /// The copy (as listed), else the newest
        #[arg(long)]
        version: Option<String>,
        /// Do it now (journaled)
        #[arg(long)]
        apply: bool,
        /// Take back the last restore instead
        #[arg(long)]
        undo: bool,
    },
}
