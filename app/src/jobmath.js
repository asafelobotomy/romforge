// How far jobs are, for the footer and the Jobs dialog.

// How far a job is, 0..1, or null when it has no total (yet).
export function fraction(j) {
  return j?.total > 0 ? Math.min(1, j.done / j.total) : null;
}

// How far the whole queue is, 0..1: finished jobs count whole, the active
// one by its own progress, the queued ones not yet.
export function overall(s) {
  const all = s.finished + (s.active ? 1 : 0) + s.queue.length;
  if (!all) return 0;
  return (s.finished + (fraction(s.active) ?? 0)) / all;
}
