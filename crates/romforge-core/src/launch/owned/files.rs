//! Each emulator's config files, from romforge's config folder for it
//! (`RomForge/Emulators/config/<emulator>/`): the one list, read by the
//! config writers (`launch/owned/`) and Emulator Settings' tables alike,
//! so a key set in either lands in the same file.

pub(crate) const RETROARCH: &str = "retroarch.cfg";
pub(crate) const DS: &str = "duckstation/settings.ini";
pub(crate) const PCSX2: &str = "PCSX2/inis/PCSX2.ini";
pub(crate) const DOLPHIN: &str = "Config/Dolphin.ini";
pub(crate) const GFX: &str = "Config/GFX.ini";
pub(crate) const PPSSPP: &str = "ppsspp/PSP/SYSTEM/ppsspp.ini";
pub(crate) const FLYCAST: &str = "flycast/emu.cfg";
pub(crate) const MGBA: &str = "mgba/config.ini";
pub(crate) const MGBA_QT: &str = "mgba/qt.ini";
pub(crate) const MELONDS: &str = "melonDS/melonDS.toml";
pub(crate) const XEMU: &str = "xemu.toml";
pub(crate) const RPCS3: &str = "rpcs3/config.yml";
pub(crate) const RPCS3_VFS: &str = "rpcs3/vfs.yml";
pub(crate) const RPCS3_GUI: &str = "rpcs3/GuiConfigs/CurrentSettings.ini";
pub(crate) const CEMU: &str = "Cemu/settings.xml";
pub(crate) const MEDNAFEN: &str = "mednafen.cfg";
pub(crate) const STAGING: &str = "dosbox-staging.conf";
pub(crate) const DOSBOX_X: &str = "dosbox-x.conf";
pub(crate) const SCUMMVM: &str = "scummvm.ini";
pub(crate) const MAME: &str = "mame.ini";
/// The Citra line's, in each one's own folder (`azahar-emu/`, …).
pub(crate) const CITRA_QT: &str = "qt-config.ini";
