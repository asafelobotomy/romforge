//! Long jobs the window queues: each command it starts that way gets a job
//! id, sends `job-progress` events as the core reports progress, and can be
//! stopped with `job_cancel` (at the core's next safe point).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use romforge_core::job::{Cancel, Step};
use tauri::{Emitter, Manager};

/// The running jobs' stop flags, by id (a list: should the window give
/// one id twice, stopping it stops both and neither forgets the other's).
#[derive(Default)]
pub struct Jobs(Mutex<HashMap<u64, Vec<Cancel>>>);

#[derive(Clone, serde::Serialize)]
struct Progress {
    job: u64,
    #[serde(flatten)]
    step: Step,
}

/// Where a job's progress goes (`job-progress` events, none without an
/// id). Owns nothing: any number can report for one job.
struct Reporter {
    app: tauri::AppHandle,
    id: Option<u64>,
    last: Option<Instant>,
}

impl Reporter {
    /// At most ten steps a second, except the first and the last.
    fn step(&mut self, step: Step) {
        let Some(job) = self.id else { return };
        let end = step.total > 0 && step.done >= step.total;
        if !end
            && self
                .last
                .is_some_and(|t| t.elapsed() < Duration::from_millis(100))
        {
            return;
        }
        self.last = Some(Instant::now());
        let _ = self.app.emit("job-progress", Progress { job, step });
    }
}

/// One running job: its stop flag, registered under its id, and where its
/// progress goes. Dropping it forgets the flag.
pub struct Job {
    pub cancel: Cancel,
    report: Reporter,
}

impl Job {
    /// The job `id` the window gave (none: nothing is sent, nothing can
    /// stop it).
    pub fn start(app: &tauri::AppHandle, id: Option<u64>) -> Job {
        let cancel = Cancel::new();
        if let Some(id) = id {
            app.state::<Jobs>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(id)
                .or_default()
                .push(cancel.clone());
        }
        Job {
            cancel,
            report: Reporter {
                app: app.clone(),
                id,
                last: None,
            },
        }
    }

    /// Send how far the job is; at most ten times a second, except the
    /// first step and the last.
    pub fn step(&mut self, step: Step) {
        self.report.step(step);
    }

    /// `done` of `total` things, and what it is on.
    pub fn count(&mut self, done: usize, total: usize, detail: impl Into<String>) {
        self.step(Step::new(done as u64, total as u64, detail));
    }

    /// Run `f` with the downloads on this thread shown (bytes) and
    /// stoppable.
    pub fn watch<T>(&self, what: &str, f: impl FnOnce() -> T) -> T {
        let mut report = Reporter {
            app: self.report.app.clone(),
            id: self.report.id,
            last: None,
        };
        let cancel = self.cancel.clone();
        let what = what.to_owned();
        romforge_core::fetch::watching(
            move |done, total| {
                report.step(Step::new(
                    done,
                    total.unwrap_or(0),
                    format!("{what}: {}", mib(done)),
                ));
                !cancel.is_set()
            },
            f,
        )
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let Some(id) = self.report.id else { return };
        let jobs = self.report.app.state::<Jobs>();
        let mut jobs = jobs.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(flags) = jobs.get_mut(&id) {
            flags.retain(|c| *c != self.cancel);
            if flags.is_empty() {
                jobs.remove(&id);
            }
        }
    }
}

fn mib(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1e6)
}

/// Ask job `job` to stop at its next safe point.
#[tauri::command]
pub fn job_cancel(jobs: tauri::State<'_, Jobs>, job: u64) {
    let jobs = jobs.0.lock().unwrap_or_else(|e| e.into_inner());
    for c in jobs.get(&job).into_iter().flatten() {
        c.cancel();
    }
}
