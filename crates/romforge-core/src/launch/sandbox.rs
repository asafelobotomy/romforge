//! A Flatpak emulator's own processes. `flatpak run` becomes `bwrap`, and
//! a signal sent to `bwrap` is not passed on: it ends the wrapper and leaves
//! the emulator running (seen with Flatpak 1.18.2, 2026-11-03: SIGINT and
//! SIGTERM to the process romforge started, and to the inner `bwrap`, left
//! the program inside alive). So a stop signals the emulator itself, which
//! sits just below the wrappers. Read from `/proc`; no program is run.

use std::path::Path;

/// Processes that only carry the sandbox.
const WRAPPERS: [&str; 3] = ["bwrap", "flatpak", "xdg-dbus-proxy"];

fn is_wrapper(proc_dir: &Path, pid: u32) -> bool {
    std::fs::read_to_string(proc_dir.join(pid.to_string()).join("comm"))
        .is_ok_and(|c| WRAPPERS.contains(&c.trim()))
}

fn children(proc_dir: &Path, pid: u32) -> Vec<u32> {
    let tasks = proc_dir.join(pid.to_string()).join("task");
    let Ok(entries) = std::fs::read_dir(tasks) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|t| std::fs::read_to_string(t.path().join("children")).ok())
        .flat_map(|s| {
            s.split_whitespace()
                .filter_map(|p| p.parse().ok())
                .collect::<Vec<u32>>()
        })
        .collect()
}

/// The emulator's top processes when `root` (the process romforge started)
/// is a sandbox wrapper, else nothing: a program started directly is signalled
/// itself. Wrappers are passed through, and below the first process that
/// isn't one the emulator is left to its own children.
fn app_pids_in(proc_dir: &Path, root: u32) -> Vec<u32> {
    if !is_wrapper(proc_dir, root) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut queue = children(proc_dir, root);
    while let Some(pid) = queue.pop() {
        if is_wrapper(proc_dir, pid) {
            queue.extend(children(proc_dir, pid));
        } else {
            found.push(pid);
        }
    }
    found.sort_unstable();
    found
}

/// A process as it was seen: its pid and its start time (field 22 of
/// `/proc/<pid>/stat`), so a pid given to another process later is never
/// taken for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Seen {
    pub pid: u32,
    start: u64,
}

fn start_time(proc_dir: &Path, pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(proc_dir.join(pid.to_string()).join("stat")).ok()?;
    // After the command name in parentheses (which may hold spaces), the
    // fields from 3 on: the start time is the 20th of them.
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()
}

/// [`app_pids`], each with its start time.
pub(super) fn app_procs(root: u32) -> Vec<Seen> {
    app_procs_in(Path::new("/proc"), root)
}

fn app_procs_in(proc_dir: &Path, root: u32) -> Vec<Seen> {
    app_pids_in(proc_dir, root)
        .into_iter()
        .filter_map(|pid| {
            Some(Seen {
                pid,
                start: start_time(proc_dir, pid)?,
            })
        })
        .collect()
}

/// Whether `p` is still running as the same process (not a zombie).
pub(super) fn alive(p: &Seen) -> bool {
    alive_in(Path::new("/proc"), p)
}

fn alive_in(proc_dir: &Path, p: &Seen) -> bool {
    let state = std::fs::read_to_string(proc_dir.join(p.pid.to_string()).join("stat"))
        .ok()
        .and_then(|s| {
            s.rsplit_once(')')
                .map(|(_, rest)| rest.trim_start().starts_with('Z'))
        });
    state == Some(false) && start_time(proc_dir, p.pid) == Some(p.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(proc_dir: &Path, pid: u32, comm: &str, kids: &[u32]) {
        let task = proc_dir.join(pid.to_string()).join("task").join(pid.to_string());
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(proc_dir.join(pid.to_string()).join("comm"), format!("{comm}\n")).unwrap();
        stat(proc_dir, pid, comm, 'S', 1000 + u64::from(pid));
        let list: Vec<String> = kids.iter().map(u32::to_string).collect();
        std::fs::write(task.join("children"), list.join(" ")).unwrap();
    }

    #[test]
    fn signals_the_emulator_below_the_wrappers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        process(p, 10, "bwrap", &[11, 12]);
        process(p, 11, "bwrap", &[13]);
        process(p, 12, "xdg-dbus-proxy", &[]);
        process(p, 13, "duckstation-qt", &[14]);
        process(p, 14, "helper", &[]);
        assert_eq!(app_pids_in(p, 10), vec![13]);
    }

    #[test]
    fn a_program_started_directly_has_no_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        process(dir.path(), 20, "retroarch", &[21]);
        process(dir.path(), 21, "child", &[]);
        assert!(app_pids_in(dir.path(), 20).is_empty());
    }

    fn stat(proc_dir: &Path, pid: u32, comm: &str, state: char, start: u64) {
        // pid (comm) state, then 18 fields, then the start time.
        let rest = vec!["0"; 18].join(" ");
        std::fs::write(
            proc_dir.join(pid.to_string()).join("stat"),
            format!("{pid} ({comm} x) {state} {rest} {start} 0 0"),
        )
        .unwrap();
    }

    #[test]
    fn a_process_is_known_by_its_start_time_too() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        process(p, 10, "bwrap", &[13]);
        process(p, 13, "duckstation-qt", &[]);
        let seen = app_procs_in(p, 10);
        assert_eq!(seen.len(), 1);
        assert!(alive_in(p, &seen[0]));
        // The wrapper gone, the emulator is still found by what was seen.
        std::fs::remove_dir_all(p.join("10")).unwrap();
        assert!(alive_in(p, &seen[0]));
        // A zombie, or another process given its pid, is not it.
        stat(p, 13, "duckstation-qt", 'Z', 1013);
        assert!(!alive_in(p, &seen[0]));
        stat(p, 13, "other", 'S', 5000);
        assert!(!alive_in(p, &seen[0]));
    }
}
