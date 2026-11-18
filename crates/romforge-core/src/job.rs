//! What a long job shares with whoever started it: how far it is
//! ([`Step`]), and a flag asking it to stop ([`Cancel`]).
//!
//! A job checks [`Cancel`] only at safe points (between files, games,
//! items or sources), so what it already did stays whole: a scan keeps the
//! files it finished (a file cut short is read again next time), a cleanup
//! keeps the items it finished journaled for undo. A reader inside one
//! file sees the flag through [`watching`] / [`stop_asked`].

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::Error;

/// Asks a job to stop at its next safe point. The default never fires;
/// clones share one flag. A [`Cancel::child`] is set by its own flag or
/// its parent's.
#[derive(Debug, Clone, Default)]
pub struct Cancel {
    flag: Option<Arc<AtomicBool>>,
    parent: Option<Arc<AtomicBool>>,
}

impl Cancel {
    /// A flag that [`Cancel::cancel`] can set.
    pub fn new() -> Self {
        Self {
            flag: Some(Arc::new(AtomicBool::new(false))),
            parent: None,
        }
    }

    /// A flag of its own that is also set when this one is (the job can
    /// stop itself without telling whoever started it that it was asked).
    pub fn child(&self) -> Self {
        Self {
            flag: Some(Arc::new(AtomicBool::new(false))),
            parent: self.flag.clone(),
        }
    }

    pub fn cancel(&self) {
        if let Some(f) = &self.flag {
            f.store(true, Ordering::Relaxed);
        }
    }

    pub fn is_set(&self) -> bool {
        [&self.flag, &self.parent]
            .into_iter()
            .flatten()
            .any(|f| f.load(Ordering::Relaxed))
    }

    /// `Err(Error::Cancelled)` once asked to stop.
    pub fn check(&self) -> Result<(), Error> {
        if self.is_set() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

thread_local! {
    /// The flag of the job this thread is reading a file for.
    static WATCHED: RefCell<Cancel> = RefCell::default();
}

/// Run `read` with `cancel` as this thread's flag, for [`stop_asked`].
pub(crate) fn watching<T>(cancel: &Cancel, read: impl FnOnce() -> T) -> T {
    struct Restore(Cancel);
    impl Drop for Restore {
        fn drop(&mut self) {
            WATCHED.with(|w| *w.borrow_mut() = std::mem::take(&mut self.0));
        }
    }
    let _restore = Restore(WATCHED.with(|w| w.replace(cancel.clone())));
    read()
}

/// Inside one file (a big disc image, an archive's members): whether the
/// job reading it was asked to stop. Never, outside [`watching`].
pub(crate) fn stop_asked() -> bool {
    WATCHED.with(|w| w.borrow().is_set())
}

/// [`stop_asked`] as `Err(Error::Cancelled)`.
pub(crate) fn check_stop() -> Result<(), Error> {
    if stop_asked() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

/// What a reader that stopped returns: the scan drops the file's results.
pub(crate) fn stopped() -> std::io::Error {
    std::io::Error::other("stopped")
}

/// Two handles are equal when they share a flag (or both have none).
impl PartialEq for Cancel {
    fn eq(&self, other: &Self) -> bool {
        match (&self.flag, &other.flag) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for Cancel {}

/// How far a job is: `done` of `total` (`total` 0: not known), and what it
/// is on now.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Step {
    pub done: u64,
    pub total: u64,
    pub detail: String,
}

impl Step {
    pub fn new(done: u64, total: u64, detail: impl Into<String>) -> Self {
        Self {
            done,
            total,
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_one_flag() {
        let c = Cancel::new();
        let d = c.clone();
        assert!(c.check().is_ok());
        d.cancel();
        assert!(c.is_set());
        assert!(matches!(c.check(), Err(Error::Cancelled)));
        assert_eq!(c, d);
        assert_ne!(c, Cancel::new());
        Cancel::default().cancel();
        assert!(!Cancel::default().is_set());
    }

    #[test]
    fn a_child_stops_with_its_parent_not_the_other_way() {
        let parent = Cancel::new();
        let child = parent.child();
        child.cancel();
        assert!(child.is_set() && !parent.is_set());
        let child = parent.child();
        parent.cancel();
        assert!(child.is_set());
    }

    #[test]
    fn a_reader_sees_the_flag_only_while_watched() {
        let c = Cancel::new();
        c.cancel();
        assert!(!stop_asked());
        assert!(watching(&c, stop_asked));
        assert!(!stop_asked());
    }
}
