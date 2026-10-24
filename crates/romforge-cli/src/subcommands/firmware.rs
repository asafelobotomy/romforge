//! `firmware` (milestone 20: BIOS files).

use clap::Subcommand;

#[derive(Subcommand)]
pub enum FirmwareCommand {
    /// Fetch RetroBIOS's list of the files each emulator loads (the list
    /// only, never BIOS files)
    Download,
    /// Check the RomForge folder's Bios/ against it
    Check {
        /// Only this system (as `dat systems` names it)
        #[arg(long)]
        system: Option<String>,
        /// Only this emulator or core (`retroarch:<core>`, or as `emulator
        /// list` names it)
        #[arg(long)]
        core: Option<String>,
        /// Show every file, not only required ones and ones you have
        #[arg(long)]
        all: bool,
    },
    /// Put BIOS files from folders, files, zips or 7z where each emulator
    /// finds them, matched by contents (shows the plan unless --apply)
    Import {
        #[arg(required = true)]
        paths: Vec<std::path::PathBuf>,
        /// Do it (copies; never replaces a file; `firmware undo` takes it back)
        #[arg(long)]
        apply: bool,
        /// Also place this file whose name fits but whose hash isn't known
        /// (as the plan shows it); may be given several times
        #[arg(long)]
        also: Vec<String>,
    },
    /// Look for BIOS files in their usual places on this computer
    /// (RetroArch's system folder, EmuDeck's, RetroDECK's, the emulators'
    /// own) and import them like `import` (shows the plan unless --apply).
    /// The originals stay where they are unless the setting
    /// `--bios-leave-in-place false` is set
    Find {
        /// Do it
        #[arg(long)]
        apply: bool,
        /// Also place this file whose name fits but whose hash isn't known
        #[arg(long)]
        also: Vec<String>,
    },
    /// Take away what the last import wrote (and put back originals it
    /// moved)
    Undo {
        /// Confirm removal
        #[arg(long)]
        yes: bool,
    },
    /// Install the PS3 firmware from Bios/rpcs3/PS3UPDAT.PUP into RPCS3
    ///
    /// Shows what it would do unless --apply.
    InstallPs3 {
        /// Run RPCS3's firmware installer now
        #[arg(long)]
        apply: bool,
    },
}
