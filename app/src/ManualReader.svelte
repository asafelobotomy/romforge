<script>
  import { invoke } from "@tauri-apps/api/core";
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import PageStrip from "./PageStrip.svelte";
  import PageView from "./PageView.svelte";
  import ReaderSide from "./ReaderSide.svelte";
  import { find } from "./manualSearch.js";
  import { edit } from "./jobs.svelte.js";
  import { languages, ocrTexts, pageWords } from "./ocr.js";

  // A manual's pages (milestone 26), drawn with pdf.js (Apache-2.0), the
  // same everywhere (not WebKitGTK's own PDF view). One page or a spread
  // (the cover alone, then pairs), fit to the width or the whole page,
  // zoom, a page box, keys, thumbnails; search; highlights and notes on
  // text or on an area of a page, kept in a collection for the game
  // (`rowId`). `onpage` hears the page so it can be remembered. A scanned
  // manual can be read by OCR (Tesseract, in the app), a page as it's
  // shown or all at once for search; what it reads is kept.
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  let { src, start = 1, rowId = null, onpage, error = $bindable("") } = $props();

  let doc = $state.raw(null);
  let pages = $state(0);
  let page = $state(1);
  let typed = $state(1);
  let spread = $state(false);
  let fit = $state("width");
  let zoom = $state(1);
  let strip = $state(true);
  let side = $state(false);
  let sideTab = $state("collection");
  let mode = $state("select");
  let color = $state("yellow");
  let problem = $state("");
  let width = $state(0);
  let height = $state(0);
  let marks = $state([]);
  let editing = $state(null);
  let query = $state("");
  let search = $state(null);
  let scans = $state({});
  let langs = $state([]);
  let ocrLang = $state(null);
  let ocrPick = $state(saved("romforge.ocrLang") ?? "eng");
  let ocrAll = $state(null);

  function saved(k) {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  }
  const scanShown = $derived(shown.some((n) => scans[n]));
  $effect(() => {
    if (rowId && (scanShown || ocrLang) && !langs.length) languages().then((l) => (langs = l.langs)).catch(() => {});
  });
  function toggleOcr() {
    ocrLang = ocrLang ? null : ocrPick;
    try {
      localStorage.setItem("romforge.ocrLang", ocrPick);
    } catch {
      // Only a convenience.
    }
  }

  // Read every scanned page not read yet (for search), one at a time.
  async function readAll(list) {
    ocrAll = { done: 0, total: list.length, stop: false };
    for (const n of list) {
      if (ocrAll.stop) break;
      try {
        await pageWords(doc, n, rowId, ocrPick, true);
      } catch (e) {
        fail(`OCR failed: ${e?.message ?? e}`);
        break;
      }
      ocrAll.done += 1;
    }
    ocrAll = null;
    if (query.trim()) runSearch();
  }

  $effect(() => {
    open(src);
  });
  $effect(() => {
    loadMarks(rowId);
  });
  $effect(() => {
    typed = page;
  });

  async function open(url) {
    doc = null;
    problem = "";
    search = null;
    try {
      const res = await fetch(url);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.arrayBuffer();
      if (data.byteLength < 5) throw new Error("empty file");
      const d = await pdfjs.getDocument({ data }).promise;
      pages = d.numPages;
      page = Math.min(Math.max(1, start), pages);
      doc = d;
    } catch (e) {
      problem = `This PDF couldn't be opened (${e?.message ?? e}).`;
    }
  }

  async function loadMarks(id) {
    marks = id ? ((await invoke("manual_marks", { rowId: id }).catch(() => [])) ?? []) : [];
  }
  const fail = (e) => (error = String(e).replace(/^(launch|settings): /, ""));
  async function addMark(m) {
    if (!rowId) return;
    try {
      await edit("Saving a bookmark…", "add_manual_mark", { rowId, mark: m });
      await loadMarks(rowId);
    } catch (e) {
      fail(e);
    }
  }
  async function editMark(id, note, c) {
    try {
      await edit("Saving a bookmark…", "edit_manual_mark", { id, note, color: c });
      await loadMarks(rowId);
    } catch (e) {
      fail(e);
    }
  }
  async function removeMark(id) {
    try {
      await edit("Saving a bookmark…", "remove_manual_mark", { id });
      await loadMarks(rowId);
    } catch (e) {
      fail(e);
    }
  }
  function openMark(m) {
    side = true;
    sideTab = "collection";
    editing = m.id;
  }

  async function runSearch() {
    if (!doc || !query.trim()) return;
    side = true;
    sideTab = "search";
    search = { busy: true, query };
    const r = await find(doc, query, rowId ? await ocrTexts(rowId, ocrPick) : undefined);
    search = { ...r, query, busy: false };
    if (r.hits.length && !r.hits.some((h) => h.page === page)) go(r.hits[0].page);
  }
  function nextHit(step) {
    const hs = search?.hits ?? [];
    if (!hs.length) return;
    const after = step > 0 ? hs.find((h) => h.page > page) : [...hs].reverse().find((h) => h.page < page);
    go((after ?? (step > 0 ? hs[0] : hs[hs.length - 1])).page);
  }

  // The pages on screen: one, or the spread `page` is in.
  const shown = $derived.by(() => {
    if (!spread || page === 1) return [page];
    const left = page % 2 === 0 ? page : page - 1;
    return left + 1 <= pages ? [left, left + 1] : [left];
  });

  function go(n) {
    page = Math.min(Math.max(1, Math.round(Number(n)) || 1), pages);
    onpage?.(page);
  }
  const next = () => go(spread ? Math.max(...shown) + 1 : page + 1);
  const prev = () => go(spread ? (shown[0] === 2 ? 1 : shown[0] - 2) : page - 1);

  function keys(e) {
    if (["INPUT", "TEXTAREA"].includes(e.target.tagName)) return;
    if (e.key === "ArrowRight" || e.key === "PageDown") next();
    else if (e.key === "ArrowLeft" || e.key === "PageUp") prev();
    else if (e.key === "Home") go(1);
    else if (e.key === "End") go(pages);
    else return;
    e.preventDefault();
  }

  // Each page's share of the room, a few pixels short so fitting the width
  // never adds a sideways scrollbar.
  const roomW = $derived((width - 6 - 12 * (shown.length - 1)) / shown.length);
</script>

<div class="reader">
  <div class="rbar">
    <button onclick={prev} disabled={!doc || page <= 1} title="Previous page (←, Page Up)">◀</button>
    <input class="pageno" type="number" min="1" max={pages || 1} bind:value={typed} onchange={() => go(typed)} aria-label="Page" title="Go to a page" />
    <span class="of">/ {pages || "…"}</span>
    <button onclick={next} disabled={!doc || Math.max(...shown) >= pages} title="Next page (→, Page Down)">▶</button>
    <span class="sep"></span>
    <button class:on={!spread} onclick={() => (spread = false)} title="One page at a time">Page</button>
    <button class:on={spread} onclick={() => (spread = true)} title="Two pages side by side, as the printed manual">Spread</button>
    <button class:on={fit === "width"} onclick={() => ((fit = "width"), (zoom = 1))} title="Fit to the width">Width</button>
    <button class:on={fit === "page"} onclick={() => ((fit = "page"), (zoom = 1))} title="Fit the whole page">Whole</button>
    <button onclick={() => (zoom = Math.max(0.5, zoom - 0.25))} title="Zoom out">−</button>
    <span class="of">{Math.round(zoom * 100)}%</span>
    <button onclick={() => (zoom = Math.min(4, zoom + 0.25))} title="Zoom in">+</button>
    <span class="sep"></span>
    <input class="find" type="search" bind:value={query} placeholder="Search the manual" onkeydown={(e) => e.key === "Enter" && runSearch()} aria-label="Search the manual" />
    {#if search?.hits?.length}
      <button onclick={() => nextHit(-1)} title="Previous page with a match">↑</button>
      <button onclick={() => nextHit(1)} title="Next page with a match">↓</button>
    {/if}
    <span class="sep"></span>
    {#if rowId}
      <button class:on={mode === "select"} onclick={() => (mode = "select")} title="Select text to highlight it">Text</button>
      <button class:on={mode === "area"} onclick={() => (mode = "area")} title="Drag a box over a picture or part of a page to add a note">Area</button>
      <button class:on={side} onclick={() => (side = !side)} title="Your highlights and notes, and search results">Collection</button>
      {#if scanShown || ocrLang}
        <span class="sep"></span>
        <select bind:value={ocrPick} disabled={!!ocrLang} aria-label="OCR language" title="The language the scan is in">
          {#each langs.length ? langs : [{ code: ocrPick, name: ocrPick }] as l (l.code)}<option value={l.code}>{l.name}</option>{/each}
        </select>
        <button class:on={!!ocrLang} onclick={toggleOcr} title="This page is a scan: read its text (OCR) so it can be selected, highlighted and searched">OCR</button>
      {/if}
    {/if}
    <button class:on={strip} onclick={() => (strip = !strip)} title="Show or hide the page thumbnails">Pages</button>
  </div>
  {#if problem}
    <p class="hint">{problem}</p>
  {:else}
    <div class="body">
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="view" bind:clientWidth={width} bind:clientHeight={height} tabindex="0" role="document" aria-label="Manual" onkeydown={keys}>
        <div class="sheets" class:fitpage={fit === "page"}>
          {#if doc && width}
            {#each shown as n (n)}
              <PageView {doc} {n} {roomW} roomH={height} {fit} {zoom} {mode} {color} {query} {rowId} {ocrLang} onscan={(k, s) => (scans[k] = s)} onerror={fail} marks={marks.filter((m) => m.page === n)} onmark={addMark} onopen={openMark} />
            {/each}
          {/if}
        </div>
      </div>
      {#if side && rowId}
        <ReaderSide bind:tab={sideTab} bind:editing {marks} {search} {go} {ocrAll} {readAll} stopAll={() => ocrAll && (ocrAll.stop = true)} edit={editMark} remove={removeMark} />
      {/if}
    </div>
    {#if strip && doc}<PageStrip {doc} {pages} {page} {go} />{/if}
  {/if}
</div>

<style>
  .reader { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .rbar { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; padding: 0.3rem 0; }
  .rbar button { padding: 0.15rem 0.55rem; }
  .rbar button.on { background: var(--accent-bg); border-color: var(--accent); }
  .pageno { width: 3.6rem; text-align: center; }
  .find { width: 11rem; }
  .of { font-size: 0.85rem; color: var(--muted); }
  .sep { width: 0.5rem; }
  .body { flex: 1; display: flex; gap: 0.6rem; min-height: 16rem; }
  /* The vertical scrollbar's room kept, so the measured width doesn't change
     once a page is taller than the view (no sideways scrollbar then). */
  .view { flex: 1; min-width: 0; overflow: auto; scrollbar-gutter: stable; background: #0002; border-radius: 6px; outline: none; }
  .view:focus-visible { box-shadow: 0 0 0 2px var(--accent); }
  .sheets { display: flex; justify-content: center; align-items: flex-start; gap: 12px; }
  .sheets.fitpage { align-items: center; min-height: 100%; }
</style>
