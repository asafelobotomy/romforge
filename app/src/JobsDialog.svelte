<script>
  import { jobs, fraction, remove, stop } from "./jobs.svelte.js";

  // The job queue: the job running now (its own progress, Stop) and the
  // ones waiting their turn (Remove).
  let { open = $bindable(false) } = $props();
  let a = $derived(jobs.active);
  const pct = (j) => Math.round((fraction(j) ?? 0) * 100);
  // Nothing left to show: the dialog goes.
  $effect(() => {
    if (open && !jobs.active && !jobs.queue.length) open = false;
  });
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="jobs-title">
      <h2 id="jobs-title">Jobs</h2>
      <h3>Running</h3>
      {#if a}
        <div class="job">
          <div class="head">
            <span class="name">{a.label.replace(/…$/, "")}</span>
            <span class="num">{#if fraction(a) != null}{pct(a)}%{/if}</span>
          </div>
          <span class="bar" class:unknown={fraction(a) == null} role="progressbar" aria-label="{a.label} progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={fraction(a) == null ? undefined : pct(a)}>
            <span class="fill" style="width: {pct(a)}%"></span>
          </span>
          <div class="head">
            <span class="hint detail" title={a.detail}>{a.stopping ? "Stopping after the current step…" : a.detail || "Working…"}</span>
            <button onclick={stop} disabled={!a.stoppable || a.stopping} title={a.stoppable ? "Stop at the next safe point; what is done stays" : "This job can't be stopped part-way"}>Stop</button>
          </div>
        </div>
      {:else}
        <p class="hint">Nothing running.</p>
      {/if}
      <h3>Queued{jobs.queue.length ? ` (${jobs.queue.length})` : ""}</h3>
      {#if jobs.queue.length}
        <ol class="queue">
          {#each jobs.queue as j (j.id)}
            <li>
              <span class="name">{j.label.replace(/…$/, "")}</span>
              <button onclick={() => remove(j.id)} title="Take it out of the queue; it won't run">Remove</button>
            </li>
          {/each}
        </ol>
      {:else}
        <p class="hint">Nothing waiting. Jobs you start while one runs wait here for their turn.</p>
      {/if}
      <div class="actions">
        <span class="spacer"></span>
        <button onclick={() => (open = false)}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .job { display: grid; gap: 0.35rem; padding: 0.5rem 0.6rem; border: 1px solid var(--line); border-radius: 8px; }
  .head { display: flex; align-items: center; gap: 0.5rem; }
  .name { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .num { color: var(--muted); font-variant-numeric: tabular-nums; }
  .detail { flex: 1; min-width: 0; margin: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bar { position: relative; display: block; height: 0.6rem; border-radius: 999px; background: var(--line); overflow: hidden; }
  .fill { position: absolute; inset: 0 auto 0 0; background: var(--accent); border-radius: inherit; transition: width 0.25s ease; }
  .bar.unknown .fill { width: 30% !important; animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { from { left: -30%; } to { left: 100%; } }
  @media (prefers-reduced-motion: reduce) { .bar.unknown .fill { animation: none; left: 35%; } }
  .queue { list-style: none; padding: 0; margin: 0; max-height: 16rem; overflow-y: auto; }
  .queue li { display: flex; align-items: center; gap: 0.5rem; padding: 0.3rem 0; border-bottom: 1px solid var(--line); }
</style>
