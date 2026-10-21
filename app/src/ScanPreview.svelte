<script>
  import SortButton from "./SortButton.svelte";
  import { sorted } from "./sort.js";
  import { basename } from "./lib.js";
  import { run } from "./jobs.svelte.js";

  // Scan folder's dry run: what scanning a folder would find, worked out
  // without changing the library; Scan it to keep it, or leave it.
  // `scanned(report)` runs after an accepted scan.
  // The dry run and the scan are jobs in the queue (footer): the dialog
  // waits for the dry run's turn, and closes once the scan is queued.
  let { open = $bindable(false), statusLabel, scanned } = $props();
  let busy = $state("");

  let dir = $state("");
  let preview = $state(null);
  let failure = $state("");
  let sort = $state({ key: "status", dir: 1 });
  const order = ["verified", "incomplete", "by_you", "derivative", "detected", "candidate", "duplicate", "wrong_system", "unknown"];
  const getters = {
    status: (g) => order.indexOf(g.status),
    title: (g) => g.title,
    platform: (g) => g.system,
    new: (g) => (g.new ? 0 : 1),
  };

  export async function show(folder) {
    dir = folder;
    preview = null;
    failure = "";
    open = true;
    busy = `Looking at ${folder} (a dry run: nothing is kept)…`;
    preview = await run(`Looking at ${basename(folder)} (dry run)…`, "scan_preview", { dir: folder });
    busy = "";
    // Failed (the status line says why), stopped or taken out of the queue.
    if (!preview && dir === folder) open = false;
  }

  async function accept() {
    failure = "";
    const folder = dir;
    open = false;
    const r = await run(`Scanning ${folder}…`, "scan_folder", { dir: folder }, { key: `scan:${folder}` });
    if (r) await scanned(r);
  }

  const counts = (gs) => {
    const by = {};
    for (const g of gs) by[g.status] = (by[g.status] ?? 0) + 1;
    return order.filter((s) => by[s]).map((s) => `${by[s]} ${statusLabel[s]?.toLowerCase() ?? s}`);
  };
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="preview-title">
      <h2 id="preview-title">Scan folder: dry run</h2>
      <p class="meta"><bdi>{dir}</bdi></p>
      {#if !preview && !failure}
        <p class="hint">Reading the folder on a copy of your library; nothing is kept and no file is touched…</p>
      {:else if preview}
        <p class="hint">
          Nothing has been kept. {preview.report.hashed} files read{preview.report.skipped ? `, ${preview.report.skipped} skipped for their extension` : ""}:
          {counts(preview.games).join(", ") || "nothing romforge recognises"}. {preview.games.filter((g) => g.new).length} new to your library.
        </p>
        {#if preview.games.length}
          <p class="sortbar">
            Sort:
            <SortButton label="Status" key="status" bind:sort />
            <SortButton label="Title" key="title" bind:sort />
            <SortButton label="Platform" key="platform" bind:sort />
            <SortButton label="New first" key="new" bind:sort />
          </p>
          <ul class="found">
            {#each sorted(preview.games, sort, getters) as g}
              <li>
                <span class="badge {g.status}">{statusLabel[g.status] ?? g.status}</span>
                <span class="t">{g.title}</span>
                <span class="sys">{g.system ?? "—"}{g.new ? " · new" : ""}</span>
              </li>
            {/each}
          </ul>
        {/if}
        {#if preview.report.errors.length}
          <details><summary class="hint">{preview.report.errors.length} problems</summary>
            <ul class="hint">{#each preview.report.errors as [p, m, e]}<li>{m ? `${p}#${m}` : p}: {e}</li>{/each}</ul>
          </details>
        {/if}
      {/if}
      {#if failure}<p class="err" role="alert">{failure}</p>{/if}
      <div class="actions">
        <span class="spacer"></span>
        <button onclick={() => (open = false)} disabled={!!busy}>Don't scan</button>
        <button class="primary" onclick={accept} disabled={!preview || !!busy}>{busy || "Scan it"}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .found { list-style: none; padding: 0; margin: 0.4rem 0 0; max-height: 18rem; overflow-y: auto; }
  .found li { display: flex; gap: 0.6rem; align-items: center; padding: 0.25rem 0; border-bottom: 1px solid var(--line); }
  .found .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
