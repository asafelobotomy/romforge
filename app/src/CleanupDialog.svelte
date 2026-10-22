<script>
  import { invoke } from "@tauri-apps/api/core";
  import SortButton from "./SortButton.svelte";
  import { sorted, remembered, remember } from "./sort.js";
  import { basename, size } from "./lib.js";
  import { run } from "./jobs.svelte.js";

  // `call` is the app's invoke that reports errors on the status line;
  // `load` reloads the library.
  // With `homeRoot` (the RomForge folder) this is Import: games move into
  // its Games/, from `from` (a folder just added) or everywhere outside it.
  // Applying, undoing and emptying the trash are long jobs: the dialog
  // closes and they wait their turn in the queue (footer); `busy` is only
  // what the dialog itself waits on.
  let { open = $bindable(false), note = $bindable(""), homeRoot = null, call, load } = $props();
  let busy = $state("");
  let from = $state(null);
  // "cleanup": Rename & Cleanup's dry run (or Import); "convert": disc
  // images into CHD where they are.
  let mode = $state("cleanup");
  const converting = $derived(mode === "convert");

  // Cleanup and rename dialog: a plan first, applied on confirmation.
  // `only`: the games selected when it opened (row ids), which is what
  // applies even if the selection changes behind the dialog; null for
  // everything (Import from the organize screen).
  let only = $state(null);
  let cleanPlan = $state(null);
  let cleanBatches = $state([]);
  let cleanError = $state("");
  let confirmCleanUndo = $state(false);
  let trash = $state(null); // what Empty trash would delete, once asked

  async function askEmptyTrash() {
    trash = await call("trash_contents");
  }

  async function emptyTrash() {
    trash = null;
    open = false;
    const r = await run("Emptying the trash…", "empty_trash", {}, { key: "empty_trash" });
    if (r) note = `Deleted ${r.files} originals (${size(r.bytes)}) from ${r.batches.length} batches; those can no longer be undone` + (r.failed.length ? `; failed: ${r.failed.join("; ")}` : "");
  }

  export async function openCleanup(folder = null, how = "cleanup", ids = null) {
    from = folder;
    mode = how;
    only = ids ? [...ids] : null;
    confirmCleanUndo = false;
    open = true;
    await planCleanup();
  }

  async function planCleanup() {
    cleanError = "";
    try {
      cleanPlan = converting ? await invoke("convert_plan", { only }) : await invoke("cleanup_plan", { only, from });
    } catch (e) {
      cleanPlan = null;
      cleanError = String(e).replace(/^settings: /, "");
    }
    cleanBatches = ((await call("cleanup_batches")) ?? []).filter((b) => !b.undone_at && !b.emptied_at);
  }

  async function applyCleanup() {
    // What the plan was made from: the job plans again with exactly these.
    const [conv, ids, dir, n, kept] = [converting, only, from, cleanPlan.items.length, cleanPlan.keep_originals];
    open = false;
    const r = conv
      ? await run(`Converting ${n} discs…`, "convert_apply", { only: ids })
      : await run(`${homeRoot ? "Importing" : "Renaming and cleaning up"} ${n} games…`, "cleanup_apply", { only: ids, from: dir });
    if (r) {
      const trashed = homeRoot ? "to the RomForge trash" : "to .romforge-trash";
      const failed = r.failed.map(([t, e]) => `${t}: ${e}`).join("; ");
      note = r.batch_id
        ? `Batch #${r.batch_id}: ${conv ? "converted" : homeRoot ? "imported" : "wrote"} ${r.done}, ${kept ? "originals kept" : `${r.trashed} originals ${trashed}`}` +
          (r.stopped ? "; stopped before the rest" : "") +
          (r.failed.length ? `; ${r.failed.length} failed: ${failed}` : "")
        : "Nothing was written" + (r.failed.length ? `: ${failed}` : "");
    }
    await load();
  }

  async function undoCleanup() {
    confirmCleanUndo = false;
    open = false;
    const r = await run("Undoing the last cleanup…", "cleanup_undo", {}, { key: "cleanup_undo" });
    if (r) note = `Batch #${r.batch_id}: removed ${r.removed} new files, restored ${r.restored} originals` + (r.failed.length ? `; failed: ${r.failed.join("; ")}` : "");
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
      {#if only}
        <p class="meta">For {only.length === 1 ? "the selected game" : `the ${only.length} selected games`}</p>
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
            <span class="hint" role="status">Delete {trash.files} originals ({size(trash.bytes)}) from {trash.batches.length} batches for good? They can't be undone after.</span>
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
