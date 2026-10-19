//! Milestone 19: apply a translation or hack to a game you have. The patch
//! is one you fetched (romforge finds no patches online by itself); the
//! result is a new file in the RomForge folder (or the Library), shown as a
//! version of the game, and the original is never touched.
//!
//! - Cartridges: IPS, BPS, UPS, PPF or xdelta applied in memory to the ROM
//!   in the form the patch wants: BPS and UPS say it (their source's size
//!   and CRC32: the file as it is, or the DAT's form without a copier
//!   header); for the others it is the DAT's form, unless an IPS's name says
//!   it is for a headered ROM. A header taken off goes back on. Written as
//!   a zip like any import.
//! - Discs: xdelta or PPF applied, streamed, to the track it was made from
//!   (or the image), the others kept, and written as a CHD: the track an
//!   xdelta names, else the first that takes it (a PPF's check block or an
//!   xdelta's checksums refuse the others). A CHD is unpacked to staging
//!   first, with a cue sheet from its own track list.
//!
//! The new file is named as a derivative of the game (`<Game> [Translation:
//! <tag>]`, identified by you, never verified: rule 8, unless a hack DAT
//! lists its hash) and remembers which patch made it. Journaled as a
//! cleanup batch, so `cleanup undo` removes it.

mod apply;
mod disc;
mod found;
mod naming;
mod pair;
mod plan;
mod read;
#[cfg(test)]
mod tests;

pub use apply::{Patched, apply};
pub use found::{FoundPatch, beside};
pub(crate) use found::{paired_with, patch_at, record};
pub use naming::PatchedFrom;
pub(crate) use naming::made_by;
pub(crate) use pair::refresh;
pub use plan::{Form, PatchPlan, plan};
pub use read::{PatchFile, read_patch};
