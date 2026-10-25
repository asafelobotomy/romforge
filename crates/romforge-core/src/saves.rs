//! Milestone 27: in-game saves (memory cards, battery saves, save folders;
//! not save states) in `RomForge/Saves/<system>/`. Where each emulator is
//! pointed is set at its start (`launch/owned/saves.rs`); the saves it made
//! before, in the places it used until now, come in with [`gather`]; the
//! saves that must stay elsewhere get copies with [`backup`].

pub mod backup;
pub mod gather;
