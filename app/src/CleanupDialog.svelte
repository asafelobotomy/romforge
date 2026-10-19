<script>
  import { invoke } from "@tauri-apps/api/core";
  import SortButton from "./SortButton.svelte";
  import { sorted, remembered, remember } from "./sort.js";
  import { basename } from "./lib.js";

  // `call` is the app's invoke that reports errors on the status line;
  // `load` reloads the library.
  // With `homeRoot` (the RomForge folder) this is Import: games move into
  // its Games/, from `from` (a folder just added) or everywhere outside it.
  let { open = $bindable(false), busy = $bindable(""), note = $bindable(""), selected, homeRoot = null, call, load } = $props();
  let from = $state(null);
  // "cleanup": Rename & Cleanup's dry run (or Import); "convert": disc
  // images into CHD where they are.
  let mode = $state("cleanup");
  const converting = $derived(mode === "convert");

  // Cleanup and rename dialog: a plan first, applied on confirmation.
  let cleanOnly = $state(false); // just the selected game
  // The game "Only the selected game" planned for, which is what applies
  // even if the selection changes behind the dialog.
  let planned = null;
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

  export async function openCleanup(folder = null, how = "cleanup") {
    from = folder;
    mode = how;
    cleanOnly = false;
    confirmCleanUndo = false;
    open = true;
    await planCleanup();
  }

  async function planCleanup() {
    cleanError = "";
    try {
      planned = cleanOnly && selected ? selected : null;
      const only = planned ? [planned] : null;
      cleanPlan = converting ? await invoke("convert_plan", { only }) : await invoke("cleanup_plan", { only, from });
    } catch (e) {
      cleanPlan = null;
      cleanError = String(e).replace(/^settings: /, "");
    }
    cleanBatches = ((await call("cleanup_batches")) ?? []).filter((b) => !b.undone_at && !b.emptied_at);
  }

  async function applyCleanup() {
    cleanError = "";
    busy = converting ? `Converting ${cleanPlan.items.length} discs…` : `Renaming and cleaning up ${cleanPlan.items.length} games…`;
    try {
      const only = planned ? [planned] : null;
      const r = converting ? await invoke("convert_apply", { only }) : await invoke("cleanup_apply", { only, from });
      const trashed = homeRoot ? "to the RomForge trash" : "to .romforge-trash";
      note = r.batch_id
        ? `Batch #${r.batch_id}: ${converting ? "converted" : homeRoot ? "imported" : "wrote"} ${r.done}, ${cleanPlan.keep_originals ? "originals kept" : `${r.trashed} originals ${trashed}`}` +
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

  let itemSort = $state(remembered("import", { key: "name", dir: 1 }));
  $effect(() => remember("import", itemSort));
  const itemGetters = { name: (it) => basename(it.target), platform: (it) => it.system, files: (it) => it.parts.length };
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="clean-title">
      {#if converting}
        <h2 id="clean-title">Convert</h2>
        <p class="hint">
          Each identified disc image that isn't a CHD yet (CD tracks with their cue sheet, or one DVD image) becomes a CHD beside
          it, under its own name, for platforms whose emulators read CHD. Every new file is read back and checked against the
          disc's hashes before the original is touched. GameCube, Wii and Xbox images stay as they are.
        </p>
      {:else if cleanPlan?.in_place}
        <h2 id="clean-title">Rename &amp; Cleanup: dry run</h2>
        <p class="hint">
          Nothing has been touched yet. Each identified game becomes one file named by its DAT, in its own folder (Replace
          originals): a zip for cartridges, a CHD for discs. Every new file is read back and checked against the original hashes
          before the original goes to the trash.
        </p>
      {:else if homeRoot}
        <h2 id="clean-title">Rename &amp; Cleanup: dry run</h2>
        <p class="hint">
          Nothing has been touched yet. Each identified game {from ? "in this folder" : "outside RomForge"} moves into RomForge as one file named by its DAT: a
          zip for cartridges, a CHD for discs (art and details in a folder beside it). Every new file is read back and checked
          against the original hashes before anything else happens. Files that aren't identified stay where they are.
        </p>
        {#if from}<p class="meta">From <bdi>{from}</bdi></p>{/if}
      {:else}
        <h2 id="clean-title">Rename &amp; Cleanup: dry run</h2>
        <p class="hint">
          Nothing has been touched yet. Each identified game in the Source becomes one file named by its DAT: a zip for cartridges, a CHD for discs (art and
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
          {#if cleanPlan.in_place}Each where it is{:else}Into <bdi>{cleanPlan.dest}</bdi>{/if} ·
          {cleanPlan.keep_originals
            ? "originals stay untouched"
            : homeRoot
              ? "originals go to the RomForge trash until you undo or empty it"
              : "originals go to .romforge-trash next to them, until you undo or empty it"}
        </p>
        {#if cleanPlan.items.length}
          <p class="sortbar">
            Sort:
            <SortButton label="Name" key="name" bind:sort={itemSort} />
            <SortButton label="Platform" key="platform" bind:sort={itemSort} />
            <SortButton label="Files" key="files" bind:sort={itemSort} />
          </p>
          <ul class="choices">
            {#each sorted(cleanPlan.items, itemSort, itemGetters) as it}
              <li title={it.parts.map((p) => p.name).join("\n")}>
                <span class="ctitle">{basename(it.target)}</span>
                {#if it.existing}
                  <span class="sys">{it.system} · already in RomForge · {it.trash.length} cop{it.trash.length === 1 ? "y" : "ies"} to the trash</span>
                {:else}
                  <span class="sys">{it.system} · {it.parts.length} file{it.parts.length === 1 ? "" : "s"}{it.kept.length ? " · original stays (holds other files)" : ""}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="hint">
            {converting
              ? "Nothing to convert: every identified disc is a CHD already, or its platform can't use one."
              : `Nothing to do: no identified game ${homeRoot ? (from ? "in this folder" : "outside RomForge") : "in the Source"} that isn't processed yet.`}
          </p>
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
          {busy || `${converting ? "Convert" : "Rename & clean up"} ${cleanPlan?.items.length ?? 0}`}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .choices { list-style: none; padding: 0; margin: 0.4rem 0 0; max-height: 16rem; overflow-y: auto; }
  .choices li { display: flex; gap: 0.6rem; align-items: center; padding: 0.25rem 0; border-bottom: 1px solid var(--line); }
</style>
