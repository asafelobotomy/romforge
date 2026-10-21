//! What a long job shares with whoever started it: how far it is
//! ([`Step`]), and a flag asking it to stop ([`Cancel`]).
//!
//! A job checks [`Cancel`] only at safe points (between files, games,
//! items or sources), so what it already did stays whole: a scan's
//! transaction rolls back, a cleanup keeps the items it finished
//! journaled for undo.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::Error;

/// Asks a job to stop at its next safe point. The default never fires;
/// clones share one flag.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Option<Arc<AtomicBool>>);

impl Cancel {
    /// A flag that [`Cancel::cancel`] can set.
    pub fn new() -> Self {
        Self(Some(Arc::new(AtomicBool::new(false))))
    }

    pub fn cancel(&self) {
        if let Some(f) = &self.0 {
            f.store(true, Ordering::Relaxed);
        }
    }

    pub fn is_set(&self) -> bool {
        self.0.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
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

/// Two handles are equal when they share a flag (or both have none).
impl PartialEq for Cancel {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
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
}
