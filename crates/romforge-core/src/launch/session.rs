//! Milestone 18, 1.6: a game while it runs. romforge keeps the emulator's
//! process and polls it, never reaping it before it has exited, so a stop
//! can't reach another process that took its id.
//!
//! Stopping asks first, never kills first, or the save is lost: SIGTERM,
//! which RetroArch (1.22.2: its first SIGTERM sets its quit flag), DuckStation,
//! PCSX2, Dolphin and Mednafen handle as a clean quit (read in their
//! source), then a kill only if it is still running after `grace`.

use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use super::{LaunchSpec, prepare};
use crate::Error;

/// What is running.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Playing {
    pub pid: u32,
    pub game: String,
    pub system: String,
    pub emulator: String,
    /// Seconds since 1970 when it started.
    pub started: u64,
}

/// How it ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Ended {
    pub playing: Playing,
    pub seconds: u64,
    /// Its exit code, when it exited on its own.
    pub code: Option<i32>,
}

/// A game being played.
pub struct Session {
    pub playing: Playing,
    /// How romforge reaches it for the hotkeys (`launch::controls`).
    pub control: Option<super::controls::Channel>,
    child: Arc<Mutex<Option<Child>>>,
    began: Instant,
}

impl Session {
    /// Write romforge's config for it, then start it.
    pub fn start(spec: &LaunchSpec) -> Result<Session, Error> {
        prepare(&spec.prepare)?;
        let child = std::process::Command::new(&spec.program)
            .args(&spec.args)
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| Error::Launch(format!("{}: {e}", spec.program.display())))?;
        let started = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Ok(Session {
            playing: Playing {
                pid: child.id(),
                game: spec.game.clone(),
                system: spec.system.clone(),
                emulator: spec.emulator.clone(),
                started,
            },
            control: spec.prepare.control,
            child: Arc::new(Mutex::new(Some(child))),
            began: Instant::now(),
        })
    }

    /// Whether it exited, without reaping it while it hasn't.
    fn exited(&self) -> Option<Option<i32>> {
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let child = guard.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.code()),
            Ok(None) => None,
            Err(_) => Some(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.exited().is_none()
    }

    /// Wait for it to end (polling), and say how long it ran.
    pub fn wait(&self) -> Ended {
        let code = loop {
            if let Some(code) = self.exited() {
                break code;
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        Ended {
            playing: self.playing.clone(),
            seconds: self.began.elapsed().as_secs(),
            code,
        }
    }

    /// Ask it to quit (SIGTERM), and kill it only if it hasn't after
    /// `grace`. Returns whether it had to be killed.
    pub fn stop(&self, grace: Duration) -> Result<bool, Error> {
        {
            // The child's lock is held from the check to the signal, so the
            // thread in `wait` can't reap it (and its pid be given to
            // another process) in between.
            let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
            let Some(child) = guard.as_mut() else {
                return Ok(false);
            };
            if !matches!(child.try_wait(), Ok(None)) {
                return Ok(false);
            }
            #[cfg(unix)]
            // SAFETY: `kill` takes a pid and a signal and touches no memory;
            // the process is ours and not reaped (checked just above, with
            // the lock that reaping needs held until after the signal), so
            // the pid is still its own.
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
        }
        let until = Instant::now() + grace;
        while Instant::now() < until {
            if !self.is_running() {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(child) = guard.as_mut()
            && child.try_wait().ok().flatten().is_none()
        {
            child.kill()?;
            // Reaped here (a killed process goes at once); its status stays
            // for `wait`.
            let _ = child.wait();
            return Ok(true);
        }
        Ok(false)
    }
}
