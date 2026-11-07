//! Running an emulator as a helper (a list it prints, a version, an
//! install): to completion, its output read as it comes, stopped if it
//! hangs. A Flatpak's program is stopped itself, not just its wrapper.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a helper run (a list, a version) gets before it is stopped:
/// MAME's whole `-listxml` or ScummVM detecting a big folder takes a
/// while, a hung one takes for ever.
pub const RUN_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How much of stderr is kept: its end, where the reason is.
const STDERR_KEPT: usize = 4096;

/// After the program ends, how long its output may take to close: a
/// process it left behind can hold the pipe open for ever.
const DRAIN: Duration = Duration::from_secs(2);

/// A pipe read on a thread into a buffer that can be taken at any time.
struct Drain {
    buf: Arc<Mutex<Vec<u8>>>,
    done: Arc<std::sync::atomic::AtomicBool>,
}

impl Drain {
    /// Read `pipe` to its end; with `keep`, only its last `keep` bytes.
    fn start(pipe: Option<impl Read + Send + 'static>, keep: Option<usize>) -> Drain {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(pipe.is_none()));
        if let Some(mut pipe) = pipe {
            let (b, d) = (Arc::clone(&buf), Arc::clone(&done));
            std::thread::spawn(move || {
                let mut chunk = [0u8; 8192];
                while let Ok(n @ 1..) = pipe.read(&mut chunk) {
                    let mut b = b.lock().unwrap_or_else(|e| e.into_inner());
                    b.extend_from_slice(&chunk[..n]);
                    if let Some(k) = keep
                        && b.len() > 2 * k
                    {
                        let cut = b.len() - k;
                        b.drain(..cut);
                    }
                }
                d.store(true, std::sync::atomic::Ordering::Release);
            });
        }
        Drain { buf, done }
    }

    /// What was read, waiting up to `until` for the pipe to close.
    fn take(self, until: Instant, keep: Option<usize>) -> Vec<u8> {
        while !self.done.load(std::sync::atomic::Ordering::Acquire) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut b = std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(k) = keep
            && b.len() > k
        {
            b.drain(..b.len() - k);
        }
        b
    }
}

/// Run `cmd` to completion with its output read, killing it if it hasn't
/// ended within `limit` (a `TimedOut` error then). stderr's last few KiB
/// are kept, for [`stderr_tail`].
pub fn output_within(mut cmd: Command, limit: Duration) -> std::io::Result<Output> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Read as it comes, so a long output doesn't fill the pipe and stall it.
    let stdout = Drain::start(child.stdout.take(), None);
    let stderr = Drain::start(child.stderr.take(), Some(STDERR_KEPT));
    let until = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= until {
            // A Flatpak's program first: killing its wrapper leaves it.
            #[cfg(unix)]
            for p in super::sandbox::app_procs(child.id()) {
                // SAFETY: `kill` touches no memory; the pids were read just
                // now below our unreaped child, so they are its program's.
                unsafe {
                    libc::kill(p.pid as libc::pid_t, libc::SIGKILL);
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("no answer after {} s: stopped", limit.as_secs()),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let drained = Instant::now() + DRAIN;
    Ok(Output {
        status,
        stdout: stdout.take(drained, None),
        stderr: stderr.take(drained, Some(STDERR_KEPT)),
    })
}

/// The last lines of a run's stderr, as ": a / b / c" for an error
/// message, or nothing.
pub fn stderr_tail(out: &Output) -> String {
    let log = String::from_utf8_lossy(&out.stderr);
    let mut tail: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .rev()
        .take(3)
        .collect();
    tail.reverse();
    if tail.is_empty() {
        String::new()
    } else {
        format!(": {}", tail.join(" / "))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_process_left_holding_the_pipe_does_not_hold_the_run() {
        let t = Instant::now();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo why >&2; sleep 30 & exit 3"]);
        let out = output_within(cmd, Duration::from_secs(20)).unwrap();
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(out.stdout, b"out\n");
        assert_eq!(stderr_tail(&out), ": why");
    }
}
