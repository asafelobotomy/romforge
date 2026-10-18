<script>
  import { invoke } from "@tauri-apps/api/core";

  // `call` is the app's invoke that reports errors on the status line;
  // `load` reloads the library.
  // With `homeRoot` (the RomForge folder) this is Import: games move into
  // its Games/, from `from` (a folder just added) or everywhere outside it.
  let { open = $bindable(false), busy = $bindable(""), note = $bindable(""), selected, homeRoot = null, call, load } = $props();
  let from = $state(null);

  // Cleanup and rename dialog: a plan first, applied on confirmation.
  let cleanOnly = $state(false); // just the selected game
  let cleanPlan = $state(null);
  let cleanBatches = $state([]);
  let cleanError = $state("");
  let confirmCleanUndo = $state(false);
  let trash = $state(null); // what Empty trash would delete, once asked

  async function askEmptyTrash() {
    trash = await call("trash_contents");
  }

  async function emptyTrash() {
    cleanError = "";
    busy = "Emptying the trash…";
    try {
      const r = await invoke("empty_trash");
      note = `Deleted ${r.files} originals (${mib(r.bytes)}) from ${r.batches.length} batches; those can no longer be undone` + (r.failed.length ? `; failed: ${r.failed.join("; ")}` : "");
    } catch (e) {
      cleanError = String(e);
    }
    trash = null;
    busy = "";
    await planCleanup();
  }

  const mib = (n) => (n >= 1 << 30 ? `${(n / (1 << 30)).toFixed(1)} GiB` : `${(n / (1 << 20)).toFixed(1)} MiB`);

  export async function openCleanup(folder = null) {
    from = folder;
    cleanOnly = false;
    confirmCleanUndo = false;
    open = true;
    await planCleanup();
  }

  async function planCleanup() {
    cleanError = "";
    try {
      cleanPlan = await invoke("cleanup_plan", { only: cleanOnly && selected ? [selected] : null, from });
    } catch (e) {
      cleanPlan = null;
      cleanError = String(e).replace(/^settings: /, "");
    }
    cleanBatches = ((await call("cleanup_batches")) ?? []).filter((b) => !b.undone_at && !b.emptied_at);
  }

  async function applyCleanup() {
    cleanError = "";
    busy = homeRoot ? `Importing ${cleanPlan.items.length} games…` : `Repacking ${cleanPlan.items.length} games…`;
    try {
      const r = await invoke("cleanup_apply", { only: cleanOnly && selected ? [selected] : null, from });
      const trashed = homeRoot ? "to the RomForge trash" : "to .romforge-trash";
      note = r.batch_id
        ? `Batch #${r.batch_id}: ${homeRoot ? "imported" : "wrote"} ${r.done}, ${cleanPlan.keep_originals ? "originals kept" : `${r.trashed} originals ${trashed}`}` +
          (r.failed.length ? `; ${r.failed.length} failed: ${r.failed.map(([t, e]) => `${t}: ${e}`).join("; ")}` : "")
        : "Nothing was written" + (r.failed.length ? `: ${r.failed.map(([t, e]) => `${t}: ${e}`).join("; ")}` : "");
      open = false;
    } catch (e) {
      cleanError = String(e);
    }
    busy = "";
    await load();
  }

  async function undoCleanup() {
    confirmCleanUndo = false;
    cleanError = "";
    busy = "Undoing the last cleanup…";
    try {
      const r = await invoke("cleanup_undo");
      note = `Batch #${r.batch_id}: removed ${r.removed} new files, restored ${r.restored} originals` + (r.failed.length ? `; failed: ${r.failed.join("; ")}` : "");
      open = false;
    } catch (e) {
      cleanError = String(e);
    }
    busy = "";
    await load();
  }

  const base = (p) => p.split("/").pop();
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="clean-title">
      {#if homeRoot}
        <h2 id="clean-title">Import into RomForge</h2>
        <p class="hint">
          Each identified game {from ? "in this folder" : "outside RomForge"} moves into RomForge as one file named by its DAT: a
          zip for cartridges, a CHD for discs (art and details in a folder beside it). Every new file is read back and checked
          against the original hashes before anything else happens. Files that aren't identified stay where they are.
        </p>
        {#if from}<p class="meta">From <bdi>{from}</bdi></p>{/if}
      {:else}
        <h2 id="clean-title">Cleanup and rename</h2>
        <p class="hint">
          Each identified game in the Source becomes one file named by its DAT: a zip for cartridges, a CHD for discs (art and
          details in a folder beside it). Every new file is read back and checked against the original hashes before anything
          else happens. Nothing is ever overwritten.
        </p>
      {/if}
      {#if !from}
        <label class="toggle">
          <input type="checkbox" bind:checked={cleanOnly} disabled={!selected} onchange={planCleanup} />
          Only the selected game
        </label>
      {/if}
      {#if cleanPlan}
        <p class="meta">
          Into <bdi>{cleanPlan.dest}</bdi> ·
          {cleanPlan.keep_originals
            ? "originals stay untouched"
            : homeRoot
              ? "originals go to the RomForge trash until you undo or empty it"
              : "originals go to .romforge-trash next to them, until you undo or empty it"}
        </p>
        {#if cleanPlan.items.length}
          <ul class="choices">
            {#each cleanPlan.items as it}
              <li title={it.parts.map((p) => p.name).join("\n")}>
                <span class="ctitle">{base(it.target)}</span>
                {#if it.existing}
                  <span class="sys">{it.system} · already in RomForge · {it.trash.length} cop{it.trash.length === 1 ? "y" : "ies"} to the trash</span>
                {:else}
                  <span class="sys">{it.system} · {it.parts.length} file{it.parts.length === 1 ? "" : "s"}{it.kept.length ? " · original stays (holds other files)" : ""}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="hint">Nothing to do: no identified game {homeRoot ? (from ? "in this folder" : "outside RomForge") : "in the Source"} that isn't processed yet.</p>
        {/if}
        {#if cleanPlan.skipped.length}
          <details>
            <summary class="hint">{cleanPlan.skipped.length} skipped</summary>
            <ul class="files">
              {#each cleanPlan.skipped as [t, why]}<li>{t}: {why}</li>{/each}
            </ul>
          </details>
        {/if}
      {/if}
      {#if cleanError}<p class="err" role="alert">{cleanError}</p>{/if}
      <div class="actions">
        {#if cleanBatches.length}
          {#if confirmCleanUndo}
            <button onclick={undoCleanup} disabled={!!busy}>Yes, undo batch #{cleanBatches[cleanBatches.length - 1].batch_id}</button>
          {:else}
            <button onclick={() => (confirmCleanUndo = true)} disabled={!!busy} title="Remove the last batch's new files and put its originals back">Undo last…</button>
          {/if}
        {/if}
        {#if trash}
          {#if trash.files}
            <span class="hint" role="status">Delete {trash.files} originals ({mib(trash.bytes)}) from {trash.batches.length} batches for good? They can't be undone after.</span>
            <button onclick={emptyTrash} disabled={!!busy}>Yes, empty the trash</button>
            <button onclick={() => (trash = null)} disabled={!!busy}>Cancel</button>
          {:else}
            <span class="hint" role="status">The trash is empty.</span>
          {/if}
        {:else}
          <button onclick={askEmptyTrash} disabled={!!busy} title="Originals are kept in the trash so a batch can be undone; this deletes them for good">Empty trash…</button>
        {/if}
        <span class="spacer"></span>
        <button onclick={() => (open = false)} disabled={!!busy}>Close</button>
        <button class="primary" onclick={applyCleanup} disabled={!cleanPlan?.items.length || !!busy}>
          {busy || `${homeRoot ? "Import" : "Repack"} ${cleanPlan?.items.length ?? 0}`}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .choices { list-style: none; padding: 0; margin: 0.4rem 0 0; max-height: 16rem; overflow-y: auto; }
  .choices li { display: flex; gap: 0.6rem; align-items: center; padding: 0.25rem 0; border-bottom: 1px solid var(--line); }
</style>
