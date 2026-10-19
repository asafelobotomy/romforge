<script>
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import PageStrip from "./PageStrip.svelte";

  // A manual's pages (milestone 26), drawn with pdf.js (Apache-2.0) on
  // canvases, the same everywhere (not WebKitGTK's own PDF view). One page
  // or a two-page spread (the cover alone, then pairs), fit to the width
  // or the whole page, zoom, arrow keys and Page Up/Down, a strip of
  // thumbnails. `onpage` hears the page so it can be remembered.
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  let { src, start = 1, onpage } = $props();

  let doc = $state.raw(null);
  let pages = $state(0);
  let page = $state(1);
  let spread = $state(false);
  let fit = $state("width");
  let zoom = $state(1);
  let strip = $state(true);
  let problem = $state("");
  let box = $state(null);
  let width = $state(0);
  let height = $state(0);
  let canvases = $state([]);
  let tasks = [];

  $effect(() => {
    open(src);
  });

  async function open(url) {
    doc = null;
    problem = "";
    try {
      const data = await (await fetch(url)).arrayBuffer();
      const d = await pdfjs.getDocument({ data }).promise;
      pages = d.numPages;
      page = Math.min(Math.max(1, start), pages);
      doc = d;
    } catch (e) {
      problem = `This PDF couldn't be opened (${e?.message ?? e}).`;
    }
  }

  // The pages on screen: one, or the spread `page` is in.
  const shown = $derived.by(() => {
    if (!spread || page === 1) return [page];
    const left = page % 2 === 0 ? page : page - 1;
    return left + 1 <= pages ? [left, left + 1] : [left];
  });

  function go(n) {
    page = Math.min(Math.max(1, n), pages);
    onpage?.(page);
  }
  const next = () => go(spread ? Math.max(...shown) + 1 : page + 1);
  const prev = () => go(spread ? (shown[0] === 2 ? 1 : shown[0] - 2) : page - 1);

  function keys(e) {
    if (e.key === "ArrowRight" || e.key === "PageDown") next();
    else if (e.key === "ArrowLeft" || e.key === "PageUp") prev();
    else if (e.key === "Home") go(1);
    else if (e.key === "End") go(pages);
    else return;
    e.preventDefault();
  }

  // Draw the pages on screen whenever what's shown or the room changes.
  $effect(() => {
    if (!doc || !width || !height) return;
    draw(shown, fit, zoom, width, height);
  });

  async function draw(ns, how, z, w, h) {
    for (const t of tasks) t.cancel();
    tasks = [];
    const gap = 12;
    // A few pixels short of the room, so fitting the width never adds a
    // sideways scrollbar (the vertical one takes its share once pages draw).
    const roomW = (w - 6 - gap * (ns.length - 1)) / ns.length;
    const dpr = window.devicePixelRatio || 1;
    await Promise.all(
      ns.map(async (n, i) => {
        const p = await doc.getPage(n);
        const base = p.getViewport({ scale: 1 });
        const scale = (how === "width" ? roomW / base.width : Math.min(roomW / base.width, h / base.height)) * z;
        const viewport = p.getViewport({ scale: scale * dpr });
        const canvas = canvases[i];
        if (!canvas) return;
        canvas.width = Math.floor(viewport.width);
        canvas.height = Math.floor(viewport.height);
        canvas.style.width = `${Math.floor(viewport.width / dpr)}px`;
        canvas.style.height = `${Math.floor(viewport.height / dpr)}px`;
        const task = p.render({ canvas, viewport });
        tasks.push(task);
        try {
          await task.promise;
        } catch {
          // Cancelled by a newer draw.
        }
      }),
    );
  }
</script>

<div class="reader">
  <div class="bar">
    <button onclick={prev} disabled={!doc || page <= 1} title="Previous page (←, Page Up)">◀</button>
    <span class="where">{#if doc}{shown.join("–")} / {pages}{:else}…{/if}</span>
    <button onclick={next} disabled={!doc || Math.max(...shown) >= pages} title="Next page (→, Page Down)">▶</button>
    <span class="sep"></span>
    <button class:on={!spread} onclick={() => (spread = false)} title="One page at a time">Page</button>
    <button class:on={spread} onclick={() => (spread = true)} title="Two pages side by side, as the printed manual">Spread</button>
    <span class="sep"></span>
    <button class:on={fit === "width"} onclick={() => ((fit = "width"), (zoom = 1))} title="Fit to the width">Width</button>
    <button class:on={fit === "page"} onclick={() => ((fit = "page"), (zoom = 1))} title="Fit the whole page">Whole</button>
    <button onclick={() => (zoom = Math.max(0.5, zoom - 0.25))} title="Zoom out">−</button>
    <span class="where">{Math.round(zoom * 100)}%</span>
    <button onclick={() => (zoom = Math.min(4, zoom + 0.25))} title="Zoom in">+</button>
    <span class="sep"></span>
    <button class:on={strip} onclick={() => (strip = !strip)} title="Show or hide the page thumbnails">Pages</button>
  </div>
  {#if problem}
    <p class="hint">{problem}</p>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="view" bind:this={box} bind:clientWidth={width} bind:clientHeight={height} tabindex="0" role="document" aria-label="Manual" onkeydown={keys}>
      <div class="sheets" class:fitpage={fit === "page"}>
        {#each shown as n, i (n)}<canvas bind:this={canvases[i]} aria-label="Page {n}"></canvas>{/each}
      </div>
    </div>
    {#if strip && doc}<PageStrip {doc} {pages} {page} {go} />{/if}
  {/if}
</div>

<style>
  .reader { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .bar { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; padding: 0.3rem 0; }
  .bar button { padding: 0.15rem 0.55rem; }
  .bar button.on { background: var(--accent-bg); border-color: var(--accent); }
  .where { min-width: 3.5rem; text-align: center; font-size: 0.85rem; color: var(--muted); }
  .sep { width: 0.6rem; }
  .view { flex: 1; min-height: 16rem; overflow: auto; background: #0002; border-radius: 6px; outline: none; }
  .view:focus-visible { box-shadow: 0 0 0 2px var(--accent); }
  .sheets { display: flex; justify-content: center; align-items: flex-start; gap: 12px; padding: 0; }
  .sheets.fitpage { align-items: center; min-height: 100%; }
  .sheets canvas { background: #fff; box-shadow: 0 2px 10px #0005; }
</style>
