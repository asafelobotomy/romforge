//! One entry of the emulator table and the builders its rows use.

/// Where an emulator's official Flatpak comes from: only a Flatpak from
/// there is used or installed. Checked on 2026-10-25 against Flathub's
/// verification (the app published by its own developers) and each
/// project's own download page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatpakFrom {
    /// Flathub, published there by its developers (or the one its own
    /// site gives as its Linux download: DOSBox-X's).
    Flathub,
    /// Its developers' own Flatpak repository (Dolphin's): the remote's
    /// name romforge adds, its `.flatpakrepo` and the repository's URL.
    Own {
        remote: &'static str,
        repo: &'static str,
        url: &'static str,
    },
    /// Only others build it while its developers make their own Linux
    /// build (Flathub's RPCS3, mGBA, Flycast): never used or installed.
    Others,
    /// Its developers make no Linux build (MAME): Flathub's, the Linux
    /// build most used (Flathub's install counts), is the fallback. Used
    /// after one on `PATH`, installed when asked, always shown as not
    /// official.
    Fallback,
}

/// What romforge can do with an emulator's data besides starting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ability {
    /// Bring its existing setup (settings, saves) into romforge's config
    /// (`launch::existing`).
    ImportSetup,
    /// Move its game data from your folders into RomForge
    /// (`launch::move_in`).
    MoveData,
    /// Gather its saves from the places it used before `Saves/`
    /// (`saves::gather`).
    GatherSaves,
    /// Keep backup copies of its saves that stay in its own data
    /// (`saves::backup`).
    BackUpSaves,
}

/// An emulator romforge knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownEmulator {
    pub name: &'static str,
    /// As its makers write it, for the app.
    pub label: &'static str,
    /// Another entry whose settings it uses (MAME's machines are MAME).
    pub shares: Option<&'static str>,
    /// Executable names to look for on `PATH` and in emulator folders.
    pub programs: &'static [&'static str],
    pub flatpak: Option<&'static str>,
    /// Where that Flatpak must come from to be used.
    pub flatpak_from: FlatpakFrom,
    /// How its AppImage's file name starts (any case), when it isn't
    /// `name`: `<appimage>*.AppImage`.
    pub appimage: Option<&'static str>,
    /// Arguments for a game: `{rom}` the file, `{dir}` its folder, `{set}`
    /// an arcade set's name, `{core}` a RetroArch core.
    pub args: &'static [&'static str],
    /// Put before `args` on every start: fullscreen, without the
    /// emulator's own UI.
    pub fullscreen: &'static [&'static str],
    /// How to run it from a config folder of romforge's (`{config}`, and
    /// `{system}` the game's system; milestone 18, 1.3). Used only for an
    /// emulator romforge writes a config for (`launch/owned.rs`).
    pub config: &'static [&'static str],
    /// Where it finds BIOS files, when the RomForge folder is there:
    /// `{bios}` is `Bios/<settings name>/` (milestone 20). For emulators
    /// that take the folder on the command line rather than in a config
    /// file of romforge's (`launch/owned/standalone.rs`).
    pub bios: &'static [&'static str],
    /// For MAME's machines: the machine it starts, whose ROM set (`<machine>.zip`)
    /// is the BIOS it needs.
    pub machine: Option<&'static str>,
    /// Opens a ROM inside a zip/7z itself.
    pub archives: bool,
    /// File extensions of its system it can't open (lower case).
    pub cannot_open: &'static [&'static str],
    /// Discontinued (PLAN, milestone 21's exception): its last release,
    /// which its entries are pinned to. romforge never installs it; it is
    /// used where the user installed it.
    pub discontinued: Option<&'static str>,
    /// The RetroBIOS profile for its files when it isn't the one of its
    /// own name (`firmware.rs`).
    pub firmware: Option<&'static str>,
    /// Its official AppImage: (GitHub repository of its releases, the end of
    /// the asset's name). romforge can download it (`obtain/appimage.rs`)
    /// and prefers it to the Flatpak when it is there.
    pub official_appimage: Option<(&'static str, &'static str)>,
    /// What else romforge does with its data.
    pub abilities: &'static [Ability],
}

pub(super) const fn emu(
    name: &'static str,
    label: &'static str,
    programs: &'static [&'static str],
    flatpak: &'static str,
    args: &'static [&'static str],
    archives: bool,
) -> KnownEmulator {
    KnownEmulator {
        name,
        label,
        shares: None,
        programs,
        flatpak: Some(flatpak),
        flatpak_from: FlatpakFrom::Flathub,
        appimage: None,
        args,
        fullscreen: &[],
        config: &[],
        bios: &[],
        machine: None,
        archives,
        cannot_open: &[],
        discontinued: None,
        firmware: None,
        official_appimage: None,
        abilities: &[],
    }
}

impl KnownEmulator {
    pub(super) const fn can(mut self, abilities: &'static [Ability]) -> Self {
        self.abilities = abilities;
        self
    }
    /// Whether romforge can do `a` with its data.
    pub fn has(&self, a: Ability) -> bool {
        self.abilities.contains(&a)
    }
    pub(super) const fn fullscreen(mut self, flags: &'static [&'static str]) -> Self {
        self.fullscreen = flags;
        self
    }
    pub(super) const fn config(mut self, flags: &'static [&'static str]) -> Self {
        self.config = flags;
        self
    }
    pub(super) const fn bios(mut self, flags: &'static [&'static str]) -> Self {
        self.bios = flags;
        self
    }
    pub(super) const fn machine(mut self, name: &'static str) -> Self {
        self.machine = Some(name);
        self
    }
    pub(super) const fn appimage(mut self, prefix: &'static str) -> Self {
        self.appimage = Some(prefix);
        self
    }
    pub(super) const fn no_flatpak(mut self) -> Self {
        self.flatpak = None;
        self
    }
    pub(super) const fn cannot_open(mut self, extensions: &'static [&'static str]) -> Self {
        self.cannot_open = extensions;
        self
    }
    pub(super) const fn discontinued(mut self, last: &'static str) -> Self {
        self.discontinued = Some(last);
        self
    }
    pub(super) const fn official_appimage(mut self, repo: &'static str, suffix: &'static str) -> Self {
        self.official_appimage = Some((repo, suffix));
        self
    }
    /// Its Flatpak is only built by others: not used, not installed.
    pub(super) const fn flatpak_by_others(mut self) -> Self {
        self.flatpak_from = FlatpakFrom::Others;
        self
    }
    /// No Linux build by its developers: Flathub's is the fallback.
    pub(super) const fn flatpak_fallback(mut self) -> Self {
        self.flatpak_from = FlatpakFrom::Fallback;
        self
    }
    /// Its Flatpak comes from its developers' own repository.
    pub(super) const fn flatpak_own(
        mut self,
        remote: &'static str,
        repo: &'static str,
        url: &'static str,
    ) -> Self {
        self.flatpak_from = FlatpakFrom::Own { remote, repo, url };
        self
    }
    pub(super) const fn firmware(mut self, profile: &'static str) -> Self {
        self.firmware = Some(profile);
        self
    }
    pub(super) const fn shares(mut self, name: &'static str) -> Self {
        self.shares = Some(name);
        self
    }
    /// The entry whose settings apply to it: [`KnownEmulator::shares`],
    /// else itself.
    pub fn settings_name(&self) -> &'static str {
        self.shares.unwrap_or(self.name)
    }
}
