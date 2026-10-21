<script>
  import { jobs, overall, fraction } from "./jobs.svelte.js";

  // The window's footer: the job queue's overall progress (click for the
  // queue), then the last note or error.
  let { note = "", error = "", openJobs } = $props();
  let a = $derived(jobs.active);
  let waiting = $derived(jobs.queue.length);
  let pct = $derived(Math.round(overall() * 100));
  let label = $derived.by(() => {
    if (!a) return "";
    const f = fraction(a);
    const own = f == null ? "" : ` ${Math.round(f * 100)}%`;
    return `${a.label.replace(/…$/, "")}${own}${waiting ? ` · ${waiting} queued` : ""}`;
  });
</script>

<footer class="foot">
  {#if a}
    <div class="jobs" role="button" tabindex="0" onclick={openJobs} onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), openJobs())} title="Show the job queue{a.detail ? `: ${a.detail}` : ''}" aria-label="{label}; show the job queue">
      <span class="bar" class:unknown={fraction(a) == null && pct === 0} role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct}>
        <span class="fill" style="width: {pct}%"></span>
      </span>
      <span class="label">{label}</span>
    </div>
  {/if}
  <p class="msg" class:error={!!error} title={error || note}>{error || note}</p>
</footer>

<style>
  .foot { display: flex; align-items: center; gap: 0.75rem; min-height: 1.9rem; padding: 0.2rem 0.75rem; border-top: 1px solid var(--line); background: var(--panel); font-size: 0.85rem; }
  .jobs { display: flex; align-items: center; gap: 0.5rem; flex: 0 1 auto; min-width: 0; max-width: 65%; padding: 0.15rem 0.5rem; border: 1px solid transparent; border-radius: 6px; cursor: pointer; }
  .jobs:hover, .jobs:focus-visible { border-color: var(--line); }
  /* An explicit width: WebKitGTK sizes the box around it from widths,
     not flex-basis, and the bar has no content of its own. */
  .bar { position: relative; flex: none; width: 9rem; height: 0.55rem; border-radius: 999px; background: var(--line); overflow: hidden; }
  .fill { position: absolute; inset: 0 auto 0 0; background: var(--accent); border-radius: inherit; transition: width 0.25s ease; }
  /* No total yet: a sliver moving across instead of a width. */
  .bar.unknown .fill { width: 30% !important; animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { from { left: -30%; } to { left: 100%; } }
  @media (prefers-reduced-motion: reduce) { .bar.unknown .fill { animation: none; left: 35%; } }
  .label { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--accent); }
  .msg { flex: 1 1 0; min-width: 0; margin: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--accent); }
  .msg.error { color: var(--bad); }
</style>
