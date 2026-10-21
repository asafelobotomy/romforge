//! Watching the downloads of one thread: a long job (a core, a manual, a
//! database) shows how far its download is, and can stop it, without every
//! function between the job and [`super::request`] passing a callback on.

use std::cell::RefCell;

type Watcher = Box<dyn FnMut(u64, Option<u64>) -> bool>;

thread_local! {
    static WATCH: RefCell<Option<Watcher>> = const { RefCell::new(None) };
}

/// Run `f` with every download on this thread reported to `on` (bytes so
/// far, the total if the server said); `on` returning `false` stops the
/// download with [`crate::Error::Cancelled`]. Threads `f` starts aren't
/// watched.
pub fn watching<T>(on: impl FnMut(u64, Option<u64>) -> bool + 'static, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Watcher>);
    impl Drop for Reset {
        fn drop(&mut self) {
            let old = self.0.take();
            WATCH.with(|w| *w.borrow_mut() = old);
        }
    }
    let old = WATCH.with(|w| w.borrow_mut().replace(Box::new(on)));
    let _reset = Reset(old);
    f()
}

/// Tell this thread's watcher; `false`: stop.
pub(super) fn report(done: u64, total: Option<u64>) -> bool {
    WATCH.with(|w| w.borrow_mut().as_mut().is_none_or(|on| on(done, total)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watcher_sees_this_thread_only_and_is_reset() {
        assert!(report(1, None), "nobody watching: go on");
        let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        let stop = watching(
            move |d, t| {
                s.borrow_mut().push((d, t));
                d < 10
            },
            || {
                let other = std::thread::spawn(|| report(99, None)).join().unwrap();
                (report(5, Some(20)), report(10, Some(20)), other)
            },
        );
        assert_eq!(stop, (true, false, true));
        assert_eq!(*seen.borrow(), [(5, Some(20)), (10, Some(20))]);
        assert!(report(50, None), "reset afterwards");
    }
}
