//! Long jobs the window queues: each command it starts that way gets a job
//! id, sends `job-progress` events as the core reports progress, and can be
//! stopped with `job_cancel` (at the core's next safe point).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use romforge_core::job::{Cancel, Step};
use tauri::{Emitter, Manager};

/// The running jobs' stop flags, by id.
#[derive(Default)]
pub struct Jobs(Mutex<HashMap<u64, Cancel>>);

#[derive(Clone, serde::Serialize)]
struct Progress {
    job: u64,
    #[serde(flatten)]
    step: Step,
}

/// One running job: its stop flag, and where its progress goes. Dropping
/// it forgets the flag.
pub struct Job {
    pub cancel: Cancel,
    app: tauri::AppHandle,
    id: Option<u64>,
    last: Option<Instant>,
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
                .insert(id, cancel.clone());
        }
        Job {
            cancel,
            app: app.clone(),
            id,
            last: None,
        }
    }

    /// Send how far the job is; at most ten times a second, except the
    /// first step and the last.
    pub fn step(&mut self, step: Step) {
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

    /// `done` of `total` things, and what it is on.
    pub fn count(&mut self, done: usize, total: usize, detail: impl Into<String>) {
        self.step(Step::new(done as u64, total as u64, detail));
    }

    /// Run `f` with the downloads on this thread shown (bytes) and
    /// stoppable.
    pub fn watch<T>(&self, what: &str, f: impl FnOnce() -> T) -> T {
        let mut job = Job {
            cancel: self.cancel.clone(),
            app: self.app.clone(),
            id: self.id,
            last: None,
        };
        // Not registered: this copy must not forget the flag when dropped.
        let id = job.id.take();
        let cancel = self.cancel.clone();
        let what = what.to_owned();
        romforge_core::fetch::watching(
            move |done, total| {
                job.id = id;
                job.step(Step::new(
                    done,
                    total.unwrap_or(0),
                    format!("{what}: {}", mib(done)),
                ));
                job.id = None;
                !cancel.is_set()
            },
            f,
        )
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.app
                .state::<Jobs>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
        }
    }
}

fn mib(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1e6)
}

/// Ask job `job` to stop at its next safe point.
#[tauri::command]
pub fn job_cancel(jobs: tauri::State<'_, Jobs>, job: u64) {
    if let Some(c) = jobs.0.lock().unwrap_or_else(|e| e.into_inner()).get(&job) {
        c.cancel();
    }
}
