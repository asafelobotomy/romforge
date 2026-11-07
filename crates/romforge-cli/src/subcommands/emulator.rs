//! `emulator`: the emulators romforge knows, adds, installs and sets up.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum EmulatorCommand {
    /// Emulators romforge knows, and which are installed
    List {
        /// Also those not installed
        #[arg(long)]
        all: bool,
    },
    /// Add (or replace) an emulator for a system; tried before the defaults
    Add {
        name: String,
        #[arg(long)]
        system: String,
        /// Executable (name on PATH, or a full path)
        #[arg(long)]
        program: String,
        /// Arguments as one string, split at spaces; {rom} is the file
        /// (e.g. --args "--fullscreen {rom}")
        #[arg(long, allow_hyphen_values = true, default_value = "{rom}")]
        args: String,
        /// It opens ROMs inside zip/7z itself
        #[arg(long)]
        archives: bool,
    },
    /// Emulator Settings: what is set for every emulator and for each one
    Settings,
    /// Set one of Emulator Settings (e.g. `set global fullscreen false`,
    /// `set duckstation program /opt/DuckStation.AppImage`); no value: unset
    Set {
        /// `global`, or an emulator (retroarch, duckstation, ...)
        emulator: String,
        /// fullscreen, program or args
        key: String,
        #[arg(allow_hyphen_values = true)]
        value: Option<String>,
    },
    /// Remove an emulator you added
    Remove { name: String },
    /// The RetroArch cores romforge's platforms use, and what the libretro
    /// buildbot has
    Cores {
        /// Fetch the buildbot's list and the core info first
        #[arg(long)]
        refresh: bool,
    },
    /// Download a core from the buildbot into RomForge/Emulators/cores
    GetCore {
        core: String,
        /// Accept a non-commercial licence
        #[arg(long)]
        yes: bool,
    },
    /// Remove a core romforge downloaded
    RemoveCore {
        core: String,
        /// Confirm removal
        #[arg(long)]
        yes: bool,
    },
    /// Install an emulator's official Flatpak (from Flathub or its
    /// developers' own repository) into your own Flatpak installation
    ///
    /// Shows what it would do unless --apply.
    Install {
        emulator: String,
        /// Run `flatpak install` now
        #[arg(long)]
        apply: bool,
        /// Download its official AppImage into RomForge/Emulators/ instead
        /// (RPCS3's; it is preferred to the Flatpak)
        #[arg(long)]
        appimage: bool,
    },
    /// Copy an emulator's existing setup (memory cards, saves, controller
    /// mappings, settings) from its usual folders into romforge's
    ///
    /// Copies nothing unless --apply (or --undo).
    ImportSetup {
        emulator: String,
        /// Copy the setup now
        #[arg(long)]
        apply: bool,
        /// Take back the last import instead
        #[arg(long)]
        undo: bool,
    },
    /// Move an emulator's game data into RomForge (RPCS3's devices, Cemu's
    /// MLC, xemu's disk and EEPROM, the 3DS emulators' NAND and SD card):
    /// each file copied and checked, then the originals to the trash
    /// (shows the plan unless --apply)
    MoveData {
        emulator: String,
        /// Move it now
        #[arg(long)]
        apply: bool,
        /// Take back the last move instead (until the trash is emptied)
        #[arg(long)]
        undo: bool,
    },
    /// Every setting in romforge's copy of an emulator's config, or change
    /// some (refused while it runs; journaled for --undo)
    Config {
        emulator: String,
        /// FILE:KEY=VALUE, e.g. `duckstation/settings.ini:[Audio]Backend=SDL`
        #[arg(long)]
        set: Vec<String>,
        /// FILE:KEY: take it out (the emulator's own default applies)
        #[arg(long)]
        unset: Vec<String>,
        /// Take back the last change instead
        #[arg(long)]
        undo: bool,
        /// Have MAME write all its settings first (`-createconfig`)
        #[arg(long)]
        write_defaults: bool,
    },
    /// A game's own settings (RetroArch and DuckStation): list them, or set
    /// KEY to VALUE (no value: the emulator's setting)
    GameSettings {
        /// The platform (e.g. `PlayStation`)
        system: String,
        title: String,
        key: Option<String>,
        value: Option<String>,
    },
}
