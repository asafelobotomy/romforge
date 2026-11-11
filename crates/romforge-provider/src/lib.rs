//! romforge-provider: romforge's games in uQ's launcher. A small
//! session-bus service on `org.romforge.Provider` serving
//! `org.uq.Launcher1.Provider`, started by the bus when uQ's launcher opens
//! and gone again after a minute without a call (never while a game it
//! started is running).
//!
//! A thin shell, like the CLI and the app: matching and checking are
//! [`romforge_core::uq::launcher`]'s, starting a game is
//! [`romforge_core::launch`]'s.

pub mod games;
pub mod notify;
pub mod provider;

pub use games::Games;
pub use provider::{BUS_NAME, INTERFACE, PATH, Provider};
