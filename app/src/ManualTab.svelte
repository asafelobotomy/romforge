<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as pick } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { artSrc } from "./lib.js";
  // pdf.js is big: the reader loads the first time a manual is opened.
  const reader = () => import("./ManualReader.svelte");

  // Details' Manual tab (milestone 26): the game's manual, or ways to get
  // one: the user's own PDF (picked or dropped here), or Vimm's Lair's
  // scan, found through Hasheous by the game's hash and downloaded only
  // when clicked (Vimm asks for no automated fetching). Manuals are
  // copyrighted scans: romforge never ships any.
  let { row, note = $bindable(""), error = $bindable("") } = $props();

  let info = $state(null);
  let busy = $state("");
  let dpi = $state(300);
  let over = $state(false);
  let saveTimer = null;

  $effect(() => {
    load(row.id);
  });

  async function load(id) {
    info = null;
    try {
      let i = await invoke("manual_info", { rowId: id });
      if (id !== row.id) return;
      info = i;
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
      await invoke("add_manual", { rowId: row.id, path });
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

  async function download() {
    busy = "download";
    try {
      await invoke("download_manual", { rowId: row.id, dpi });
      note = "Manual downloaded from Vimm's Lair.";
      await load(row.id);
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
    busy = "";
  }

  async function remove() {
    try {
      await invoke("remove_manual", { rowId: row.id });
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
</script>

<section class="manual" class:over>
  {#if !info}
    <p class="hint">Looking…</p>
  {:else if info.path}
    {#await reader() then { default: ManualReader }}
      <ManualReader src={artSrc(info.path)} start={info.last_page} {onpage} />
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
      {#if busy === "lookup"}
        <p class="hint">Asking Hasheous whether Vimm's Lair has this game's manual…</p>
      {:else if info.vimm_id}
        <p>Vimm's Lair has a scan of this manual.</p>
        <div class="pick">
          <button class="primary" onclick={download} disabled={!!busy}>{busy === "download" ? "Downloading…" : "Download manual (Vimm's Lair)"}</button>
          <select bind:value={dpi} aria-label="Scan resolution" title="The scan's resolution: higher is sharper and bigger">
            {#each [150, 300, 600] as d}<option value={d}>{d} dpi</option>{/each}
          </select>
          <button class="linkish" onclick={() => openUrl(vimmPage(info.vimm_id))}>Visit Vimm's Lair</button>
        </div>
        <p class="hint">One manual per click, as Vimm's Lair asks. If it won't download, get it from their page and drop it here.</p>
      {:else if info.checked}
        <p class="hint">Hasheous knows no Vimm's Lair manual for this game.</p>
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
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
</style>
