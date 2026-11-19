<script>
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";

  // Advanced: the earlier batches of Organize (`kind` "organize") or of
  // Rename & Cleanup, Convert and Import ("cleanup"), each with its own
  // Undo; the screens' plain button undoes the last one. `run` queues the
  // job (the footer shows it); `done(message)` says how it went. What the
  // CLI does with `undo --batch N`.
  let { kind, busy, run, done } = $props();

  let opened = $state(false);
  let batches = $state([]);

  async function load() {
    batches = ((await invoke(`${kind}_batches`).catch(() => [])) ?? []).slice().reverse();
  }
  $effect(() => {
    if (opened) load();
  });

  const undone = (b) => !!b.undone_at;
  const gone = (b) => !!b.emptied_at;
  const what = (b) =>
    kind === "organize"
      ? `${b.moves} files moved into ${b.dest}`
      : `${b.outputs} written, ${b.keep_originals ? "originals kept" : `${b.trashed} originals trashed`}`;

  async function undo(b) {
    const ok = await ask(
      kind === "organize"
        ? `Move the files of batch #${b.batch_id} back where they were?`
        : `Remove what batch #${b.batch_id} wrote and put its originals back?`,
      { title: `Undo batch #${b.batch_id}`, kind: "warning" },
    );
    if (!ok) return;
    const r = await run(`Undoing batch #${b.batch_id}…`, `${kind}_undo`, { batch: b.batch_id }, { key: `${kind}_undo` });
    if (r) {
      done(
        kind === "organize"
          ? `Batch #${r.batch_id}: restored ${r.restored}, failed ${r.failed.length}`
          : `Batch #${r.batch_id}: removed ${r.removed} new files, restored ${r.restored} originals` + (r.failed.length ? `; failed: ${r.failed.join("; ")}` : ""),
      );
    }
    await load();
  }
</script>

<details bind:open={opened} class="batches">
  <summary>Advanced: earlier batches</summary>
  {#if batches.length}
    <ul>
      {#each batches as b (b.batch_id)}
        <li>
          <span>#{b.batch_id} · {b.created_at} · {what(b)}</span>
          <span class="spacer"></span>
          {#if undone(b)}
            <span class="hint">undone {b.undone_at}</span>
          {:else if gone(b)}
            <span class="hint" title="The trash was emptied: its originals are gone">can't be undone</span>
          {:else}
            <button onclick={() => undo(b)} disabled={!!busy}>Undo</button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">No batches yet.</p>
  {/if}
</details>

<style>
  .batches { margin: 0.5rem 0; }
  ul { list-style: none; padding: 0; margin: 0.3rem 0 0; max-height: 12rem; overflow-y: auto; }
  li { display: flex; gap: 0.5rem; align-items: center; padding: 0.2rem 0; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  .spacer { flex: 1; }
</style>
