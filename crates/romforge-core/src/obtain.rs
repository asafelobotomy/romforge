//! Milestone 18, 1.4: getting emulators and cores from inside romforge.
//! Like house rule 10's "the user downloads it": nothing is shipped, each
//! is fetched from its own project when the user asks.
//!
//! - RetroArch cores from the libretro buildbot (what RetroArch's own
//!   updater uses; no published terms, `robots.txt` sets none), into
//!   `RomForge/Emulators/cores/`, which romforge looks in first. Each
//!   core's licence comes from libretro's core info and is shown first;
//!   non-commercial ones are flagged.
//! - RetroArch and standalone emulators from Flathub, in the user's own
//!   Flatpak installation (no root), with the `flatpak` tool the way
//!   romforge already starts Flatpaks. romforge never bundles an emulator.

mod cores;
pub use cores::*;
mod flathub;
pub use flathub::*;
