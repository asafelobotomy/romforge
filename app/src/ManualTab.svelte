<script>
  import { invoke } from "@tauri-apps/api/core";
  import { edit, run } from "./jobs.svelte.js";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ask, open as pick } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { artSrc, size } from "./lib.js";
  import { displayTitle } from "./labels.js";
  // pdf.js is big: the reader loads the first time a manual is opened.
  const reader = () => import("./ManualReader.svelte");

  // Details' Manual tab (milestone 26): the game's manual, or ways to get
  // one: the scans archive.org holds under the game's name
  // (retrokit-manuals, Kirkland's Manual Labor), Vimm's Lair's scan found
  // through Hasheous by the game's hash, or the user's own PDF (picked or
  // dropped here). Each is downloaded only when clicked. Manuals are
  // copyrighted scans: romforge never ships any.
  let { row, note = $bindable(""), error = $bindable("") } = $props();

  let info = $state(null);
  let offers = $state(null);
  let busy = $state("");
  let dpi = $state(100);
  let over = $state(false);
  let saveTimer = null;

  $effect(() => {
    load(row.id);
  });

  async function load(id) {
    info = null;
    offers = null;
    try {
      let i = await invoke("manual_info", { rowId: id });
      if (id !== row.id) return;
      info = i;
      if (!i.path)
        invoke("manual_offers", { rowId: id })
          .catch(() => [])
          .then((o) => id === row.id && (offers = o));
      if (!i.path && !i.checked) {
        busy = "lookup";
        await invoke("manual_lookup", { rowId: id }).catch(() => null);
        busy = "";
        i = await invoke("manual_info", { rowId: id });
        if (id === row.id) info = i;
      }
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
  }

  async function add(path) {
    busy = "add";
    try {
      const onWait = (why) => (note = `Adding the manual ${why}`);
      await edit("Adding a manual…", "add_manual", { rowId: row.id, path }, { onWait });
      note = "Manual added.";
      await load(row.id);
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
    busy = "";
  }

  async function choose() {
    const path = await pick({ multiple: false, title: "The game's manual", filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (typeof path === "string") add(path);
  }

  // Downloads are jobs in the queue (footer); the game may have changed
  // by the time one is done.
  async function download() {
    const [id, title] = [row.id, displayTitle(row)];
    busy = "download";
    const done = await run(`Downloading the manual of ${title}…`, "download_manual", { rowId: id, dpi }, { key: `manual:${id}` });
    busy = "";
    if (done) note = `${title}: manual downloaded from Vimm's Lair.`;
    if (done && row?.id === id) await load(id);
  }

  async function take(offer) {
    const [id, title] = [row.id, displayTitle(row)];
    busy = offer.url;
    const done = await run(`Downloading the manual of ${title}…`, "download_manual_offer", { rowId: id, offer }, { key: `manual:${id}` });
    busy = "";
    if (done) note = `${title}: manual downloaded (${offer.source}).`;
    if (done && row?.id === id) await load(id);
  }

  async function remove() {
    const ok = await ask("Forget this manual and delete romforge's copy?", { title: "Remove manual", kind: "warning" });
    if (!ok) return;
    try {
      const onWait = (why) => (note = `Removing the manual ${why}`);
      await edit("Removing a manual…", "remove_manual", { rowId: row.id }, { onWait });
      note = "Manual removed.";
      await load(row.id);
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
  }

  // Remember the page, a moment after leafing stops.
  function onpage(page) {
    clearTimeout(saveTimer);
    const id = row.id;
    saveTimer = setTimeout(() => invoke("set_manual_page", { rowId: id, page }).catch(() => {}), 800);
  }

  // A PDF dropped while the tab is showing becomes the manual.
  $effect(() => {
    let off = null;
    let gone = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const t = e.payload.type;
        if (t === "enter" || t === "over") over = true;
        else if (t === "leave") over = false;
        else if (t === "drop") {
          over = false;
          const pdf = e.payload.paths?.find((p) => p.toLowerCase().endsWith(".pdf"));
          if (pdf) add(pdf);
        }
      })
      .then((f) => (gone ? f() : (off = f)));
    return () => {
      gone = true;
      off?.();
    };
  });

  const vimmPage = (id) => `https://vimm.net/manual/${id}`;
  const mb = (n) => (n ? ` · ${size(n)}` : "");
  const name = $derived(displayTitle(row).split(" (")[0]);
  const elsewhere = $derived([
    ["archive.org", `https://archive.org/search?query=${encodeURIComponent(`"${name}" manual`)}`],
    ["VideoGameManual.com", "https://www.videogamemanual.com/"],
    ["GamesDatabase.org", "https://www.gamesdatabase.org/all_manuals"],
  ]);
</script>

<section class="manual" class:over>
  {#if !info}
    <p class="hint">Looking…</p>
  {:else if info.path}
    {#await reader() then { default: ManualReader }}
      <ManualReader src={artSrc(info.path)} start={info.last_page} rowId={row.id} {onpage} bind:error />
    {/await}
    <p class="hint foot">
      {info.source ?? "Your manual"}
      {#if info.vimm_id}· <button class="linkish" onclick={() => openUrl(vimmPage(info.vimm_id))}>Vimm's Lair</button>{/if}
      · <button class="linkish" onclick={choose}>Replace…</button>
      · <button class="linkish" onclick={remove}>Remove</button>
    </p>
  {:else}
    <div class="empty">
      <p><strong>No manual yet.</strong> Drop a PDF here, or add one.</p>
      <div class="pick">
        <button onclick={choose} disabled={!!busy}>Add a PDF…</button>
      </div>
      {#if offers === null}
        <p class="hint">Looking for this game's manual on archive.org…</p>
      {:else if offers.length}
        <p>Found on archive.org:</p>
        <ul class="offers">
          {#each offers as o (o.url)}
            <li>
              <button class:primary={o === offers[0]} onclick={() => take(o)} disabled={!!busy}>{busy === o.url ? "Downloading…" : "Download"}</button>
              <span>{o.name}</span>
              <span class="hint">{o.source}{mb(o.size)}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if busy === "lookup"}
        <p class="hint">Asking Hasheous whether Vimm's Lair has this game's manual…</p>
      {:else if info.vimm_id}
        <p>Vimm's Lair has a scan of this manual{offers?.length ? " too" : ""}.</p>
        <div class="pick">
          <button class:primary={!offers?.length} onclick={download} disabled={!!busy}>{busy === "download" ? "Downloading…" : "Download manual (Vimm's Lair)"}</button>
          <select bind:value={dpi} aria-label="Scan resolution" title="Scan resolution (clamped to each manual's own max on Vimm's Lair)">
            {#each [50, 100, 200, 300] as d}<option value={d}>{d} dpi</option>{/each}
          </select>
          <button class="linkish" onclick={() => openUrl(vimmPage(info.vimm_id))}>Visit Vimm's Lair</button>
        </div>
        <p class="hint">One manual per click, as Vimm's Lair asks. If their server won't send it, get it from their page and drop it here.</p>
      {:else if info.checked && offers?.length === 0}
        <p class="hint">Neither archive.org (by name) nor Vimm's Lair (by hash) has this game's manual.</p>
      {/if}
      {#if offers?.length === 0}
        <p class="hint">
          Search for it yourself, then drop the PDF here:
          {#each elsewhere as [label, url], i}{i ? " · " : ""}<button class="linkish" onclick={() => openUrl(url)}>{label}</button>{/each}
        </p>
      {/if}
    </div>
  {/if}
</section>

<style>
  .manual { display: flex; flex-direction: column; min-height: 26rem; height: calc(100vh - 16rem); border: 2px dashed transparent; border-radius: 8px; }
  .manual.over { border-color: var(--accent); background: var(--accent-bg); }
  .empty { padding: 1rem 0.5rem; display: grid; gap: 0.6rem; justify-items: start; }
  .empty p { margin: 0; }
  .foot { margin: 0.3rem 0 0; }
  .offers { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.35rem; }
  .offers li { display: flex; align-items: center; gap: 0.6rem; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
</style>
