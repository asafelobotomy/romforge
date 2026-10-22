import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { overall as of } from "./jobmath.js";

export { fraction } from "./jobmath.js";

// Long jobs (scans, Enrich, Rename & Cleanup, downloads...): one runs at a
// time, the rest wait their turn behind it. Each gets an id the backend
// sends its progress under (`job-progress`) and can be stopped by
// (`job_cancel`, at its next safe point). Play and quick edits don't come
// here: they run at once (quick edits through `edit`, which queues one
// only while a scan or the like holds the library).
export const jobs = $state({
  active: null,
  queue: [],
  // Jobs done since the queue was last empty: the footer's bar is over
  // these, the active one and the queue.
  finished: 0,
});

let nextId = 1;
// Where a job's error goes (the app's status line), set by the app.
let onError = () => {};
let onNote = () => {};
export function reportTo(error, note) {
  onError = error;
  onNote = note;
}

// The backend's word for a job that stopped when asked.
const STOPPED = "stopped";

// Commands that stop at a safe point when asked (the rest run to the end:
// undo, installers, MAME's list, patching).
const STOPPABLE = new Set([
  "refresh",
  "scan_folder",
  "scan_preview",
  "enrich",
  "search_fetch",
  "search_accept",
  "cleanup_apply",
  "convert_apply",
  "organize_apply",
  "download_dats",
  "extdb_download",
  "firmware_download",
  "download_core",
  "download_manual",
  "download_manual_offer",
]);

// Run `cmd` as a job, after the ones queued. Resolves to its result, or
// null when it failed (the error is reported), was stopped or removed.
// `key`: an identical job already waiting is not queued twice (its
// promise is returned).
export function run(label, cmd, args = {}, { key = null } = {}) {
  const same = key && [jobs.active, ...jobs.queue].find((j) => j?.key === key);
  if (same) return same.promise;
  const job = queued(label, cmd, args, key);
  jobs.queue.push(job);
  pump();
  return job.promise;
}

function queued(label, cmd, args, key, edit = false) {
  const stoppable = STOPPABLE.has(cmd);
  const job = { id: nextId++, label, cmd, args, key, stoppable, edit, done: 0, total: 0, detail: "", stopping: false };
  job.promise = new Promise((resolve, reject) => {
    job.resolve = resolve;
    job.reject = reject;
  });
  return job;
}

// The backend's word for a quick edit that found the library held by a
// scan, refresh or DAT import (the rest of its message says so).
const BUSY = "busy:";

// A quick edit (forget a match, pick art, save settings...): runs at once,
// beside whatever job is running (it has its own connection). A scan,
// refresh or DAT import holds the library for its whole run, though; then
// the edit waits right behind that job, shown in the jobs bar, and
// `onWait(why)` is told. Resolves to the result, or throws the error, as
// `invoke` does (also when it was taken out of the queue).
export async function edit(label, cmd, args = {}, { onWait = () => {} } = {}) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    if (!String(e).startsWith(BUSY)) throw e;
    const holder = jobs.active;
    // The job holding the library ended in the meantime: try once more.
    if (!holder) return invoke(cmd, args);
    const plain = (l) => l.replace(/…$/, "");
    const job = queued(`${plain(label)}, after ${plain(holder.label).toLowerCase()}…`, cmd, args, null, true);
    jobs.queue.unshift(job);
    onWait(`waits for “${plain(holder.label)}” to end, which holds the library (see the jobs bar).`);
    pump();
    return job.promise;
  }
}

function pump() {
  if (jobs.active) return;
  const next = jobs.queue.shift();
  if (!next) {
    jobs.finished = 0;
    return;
  }
  jobs.active = next;
  invoke(next.cmd, { ...next.args, job: next.id })
    .then((value) => next.resolve(value))
    .catch((e) => {
      // An edit's error goes back to whoever made it.
      if (next.edit) return next.reject(e);
      const msg = String(e);
      if (msg.endsWith(STOPPED)) onNote(`${next.label.replace(/…$/, "")}: stopped.`);
      else onError(msg);
      next.resolve(null);
    })
    .finally(() => {
      jobs.active = null;
      jobs.finished += 1;
      pump();
    });
}

// Take a queued job out (it never starts).
export function remove(id) {
  const i = jobs.queue.findIndex((j) => j.id === id);
  if (i < 0) return;
  const [job] = jobs.queue.splice(i, 1);
  if (job.edit) job.reject(`${job.label}: taken out of the queue, not done.`);
  else job.resolve(null);
  if (!jobs.active && !jobs.queue.length) jobs.finished = 0;
}

// Ask the active job to stop at its next safe point.
export function stop() {
  const a = jobs.active;
  if (!a?.stoppable || a.stopping) return;
  a.stopping = true;
  invoke("job_cancel", { job: a.id }).catch(() => {});
}

// How far the whole queue is, 0..1.
export const overall = () => of(jobs);

const unProgress = listen("job-progress", (e) => {
  const p = e.payload;
  const a = jobs.active;
  if (a?.id !== p.job) return;
  a.done = p.done;
  a.total = p.total;
  a.detail = p.detail;
});
// A hot reload replaces this module: drop the old listener.
import.meta.hot?.dispose(() => unProgress.then((f) => f()));
