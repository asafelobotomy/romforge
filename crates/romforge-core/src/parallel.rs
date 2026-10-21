//! Doing items on several threads while the database stays on one.
//!
//! A rusqlite connection is not `Sync`: workers get what they need read
//! beforehand and send results back; the calling thread (which owns the
//! connection) writes them. Results come back over a bounded channel, so a
//! slow writer slows the workers instead of filling memory.

use std::sync::{Mutex, mpsc};

/// Threads for a setting: 0 picks one per CPU, at most 8.
pub fn threads(setting: usize) -> usize {
    match setting {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get().min(8)),
        n => n,
    }
}

/// Run `work` on every item, on up to `threads` threads; whatever it sends
/// reaches `each` on the calling thread, as it comes. With one thread (or
/// one item) everything runs on the calling thread. `work` may stop early
/// (a cancel) by returning without sending.
pub fn stream<T: Send, R: Send>(
    items: Vec<T>,
    threads: usize,
    work: impl Fn(T, &mut dyn FnMut(R)) + Sync,
    mut each: impl FnMut(R),
) {
    let threads = threads.max(1).min(items.len());
    if threads <= 1 {
        for item in items {
            work(item, &mut each);
        }
        return;
    }
    let queue = Mutex::new(items.into_iter());
    let (tx, rx) = mpsc::sync_channel::<R>(256);
    std::thread::scope(|s| {
        for _ in 0..threads {
            let tx = tx.clone();
            let (queue, work) = (&queue, &work);
            s.spawn(move || {
                loop {
                    let next = queue.lock().unwrap_or_else(|e| e.into_inner()).next();
                    let Some(item) = next else { break };
                    work(item, &mut |r| {
                        let _ = tx.send(r);
                    });
                }
            });
        }
        drop(tx);
        for r in rx {
            each(r);
        }
    });
}

/// `work` on every item on up to `threads` threads; `each` gets each
/// result on the calling thread **in item order** (with its index), as
/// soon as it and every earlier one are done.
pub fn map_ordered<T: Send, R: Send>(
    items: Vec<T>,
    threads: usize,
    work: impl Fn(T) -> R + Sync,
    mut each: impl FnMut(usize, R),
) {
    let mut waiting = std::collections::BTreeMap::new();
    let mut next = 0;
    stream(
        items.into_iter().enumerate().collect(),
        threads,
        |(i, item), send| send((i, work(item))),
        |(i, r)| {
            waiting.insert(i, r);
            while let Some(r) = waiting.remove(&next) {
                each(next, r);
                next += 1;
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_ordered_keeps_order_on_any_thread_count() {
        for threads in [1, 4] {
            let mut got = Vec::new();
            map_ordered(
                (0..50u64).collect(),
                threads,
                |n| {
                    // Later items finish first.
                    std::thread::sleep(std::time::Duration::from_micros(50 - n));
                    n * 2
                },
                |i, r| got.push((i, r)),
            );
            let want: Vec<(usize, u64)> = (0..50).map(|n| (n as usize, n * 2)).collect();
            assert_eq!(got, want, "{threads} threads");
        }
    }

    #[test]
    fn stream_runs_every_item_once() {
        let mut got = Vec::new();
        stream(
            (0..20).collect(),
            3,
            |n: i32, send| {
                send(n);
                send(-n);
            },
            |r| got.push(r),
        );
        got.sort();
        let mut want: Vec<i32> = (0..20).flat_map(|n| [n, -n]).collect();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(threads(3), 3);
        assert!((1..=8).contains(&threads(0)));
    }
}
