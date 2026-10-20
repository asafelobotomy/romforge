<script>
  import { TextLayer } from "pdfjs-dist";
  import { asTextContent, pageWords } from "./ocr.js";

  // One page of a manual (milestone 26): the page drawn by pdf.js, its
  // text layer over it (invisible, selectable: what highlights and search
  // work on), the saved marks, and search hits. In "select" mode a text
  // selection offers to save it as a highlight; in "area" mode a box
  // dragged over the page (a map, a picture) takes a note. Marks are kept
  // as fractions of the page, so they hold at any zoom. A scanned page
  // (no text in the PDF) gets its text layer from OCR when `ocrLang` is
  // set; `onscan` hears whether the page is a scan.
  let { doc, n, roomW, roomH, fit, zoom, marks = [], query = "", mode = "select", color = "yellow", rowId = null, ocrLang = null, onscan, onerror, onmark, onopen } = $props();

  const COLORS = ["yellow", "red", "green", "blue"];
  let frame = $state(null);
  let canvas = $state(null);
  let textBox = $state(null);
  let size = $state({ w: 0, h: 0 });
  let pending = $state(null);
  let note = $state("");
  let pick = $state(color);
  let drag = $state(null);
  let reading = $state(false);
  let task = null;
  let layer = null;
  let drawn = 0;

  $effect(() => {
    if (canvas && textBox) draw(doc, n, roomW, roomH, fit, zoom, ocrLang);
  });

  async function draw(d, page, w, h, how, z, lang) {
    const me = ++drawn;
    task?.cancel();
    layer?.cancel();
    pending = null;
    const p = await d.getPage(page);
    const base = p.getViewport({ scale: 1 });
    const scale = (how === "width" ? w / base.width : Math.min(w / base.width, h / base.height)) * z;
    const view = p.getViewport({ scale });
    const dpr = window.devicePixelRatio || 1;
    const sharp = p.getViewport({ scale: scale * dpr });
    canvas.width = Math.floor(sharp.width);
    canvas.height = Math.floor(sharp.height);
    size = { w: Math.floor(view.width), h: Math.floor(view.height) };
    task = p.render({ canvas, viewport: sharp });
    try {
      await task.promise;
    } catch {
      return; // A newer draw took over.
    }
    let content = await p.getTextContent();
    const scan = !content.items.some((i) => i.str?.trim());
    onscan?.(page, scan);
    if (scan && rowId && lang) {
      reading = true;
      try {
        content = asTextContent(p, await pageWords(d, page, rowId, lang, true));
      } catch (e) {
        onerror?.(`OCR failed: ${e?.message ?? e}`);
      }
      reading = false;
    }
    if (me !== drawn) return;
    textBox.replaceChildren();
    textBox.style.setProperty("--total-scale-factor", String(scale));
    layer = new TextLayer({ textContentSource: content, container: textBox, viewport: view });
    try {
      await layer.render();
    } catch {
      return;
    }
    // As pdf.js's viewer does: an element past the text that, while
    // selecting, covers the page, so a drag that starts in blank space
    // still selects text.
    const end = document.createElement("div");
    end.className = "endOfContent";
    textBox.append(end);
    markHits(query);
  }

  // Search hits: the text spans holding the query.
  function markHits(q) {
    if (!textBox) return;
    const want = q.trim().toLowerCase();
    for (const s of textBox.querySelectorAll("span")) s.classList.toggle("hit", !!want && s.textContent.toLowerCase().includes(want));
  }
  $effect(() => markHits(query));

  // A rectangle on the screen as fractions of the page.
  function frac(r, box) {
    return [(r.left - box.left) / box.width, (r.top - box.top) / box.height, r.width / box.width, r.height / box.height];
  }

  function selected() {
    textBox?.classList.remove("selecting");
    if (mode !== "select") return;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !sel.rangeCount) return;
    const range = sel.getRangeAt(0);
    if (!textBox.contains(range.commonAncestorContainer)) return;
    const box = frame.getBoundingClientRect();
    const rects = [...range.getClientRects()].filter((r) => r.width > 1 && r.height > 1).map((r) => frac(r, box));
    const end = range.getBoundingClientRect();
    if (!rects.length || !sel.toString().trim()) return;
    open({ kind: "highlight", text: sel.toString(), rects, x: end.right - box.left, y: end.bottom - box.top });
  }

  function open(p) {
    note = "";
    pick = color;
    pending = p;
  }

  function down(e) {
    if (mode === "select" && e.button === 0) textBox?.classList.add("selecting");
    if (mode !== "area" || e.button !== 0 || pending) return;
    const b = frame.getBoundingClientRect();
    drag = { x0: e.clientX - b.left, y0: e.clientY - b.top, x1: e.clientX - b.left, y1: e.clientY - b.top };
    frame.setPointerCapture(e.pointerId);
  }
  function move(e) {
    if (!drag) return;
    const b = frame.getBoundingClientRect();
    drag = { ...drag, x1: Math.min(Math.max(0, e.clientX - b.left), b.width), y1: Math.min(Math.max(0, e.clientY - b.top), b.height) };
  }
  function up() {
    if (!drag) return;
    const [x, y] = [Math.min(drag.x0, drag.x1), Math.min(drag.y0, drag.y1)];
    const [w, h] = [Math.abs(drag.x1 - drag.x0), Math.abs(drag.y1 - drag.y0)];
    drag = null;
    if (w < 8 || h < 8) return;
    open({ kind: "area", text: "", rects: [[x / size.w, y / size.h, w / size.w, h / size.h]], x: x + w, y: y + h });
  }

  function save() {
    onmark?.({ page: n, kind: pending.kind, text: pending.text, rects: pending.rects, color: pick, note });
    pending = null;
    window.getSelection()?.removeAllRanges();
  }
  function cancel() {
    pending = null;
    window.getSelection()?.removeAllRanges();
  }

  const pct = (v) => `${(v * 100).toFixed(3)}%`;
</script>

<div
  class="frame"
  class:area={mode === "area"}
  bind:this={frame}
  style:width="{size.w}px"
  style:height="{size.h}px"
  role="presentation"
  onmouseup={selected}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
>
  <canvas bind:this={canvas} aria-label="Page {n}"></canvas>
  <div class="marks" aria-hidden="true">
    {#each marks as m (m.id)}
      {#each m.rects as [x, y, w, h]}
        <div class="mark {m.kind} {m.color}" style:left={pct(x)} style:top={pct(y)} style:width={pct(w)} style:height={pct(h)}></div>
      {/each}
    {/each}
  </div>
  <div class="textLayer" bind:this={textBox}></div>
  {#if reading}<span class="reading">Reading the scan…</span>{/if}
  {#each marks as m (m.id)}
    {@const [x, y] = m.rects[0]}
    <button class="pin {m.color}" style:left={pct(x)} style:top={pct(y)} title={m.note || (m.kind === "area" ? "Area note" : m.text)} onclick={() => onopen?.(m)}>{m.note ? "✎" : "•"}</button>
  {/each}
  {#if drag}
    <div class="drag" style:left="{Math.min(drag.x0, drag.x1)}px" style:top="{Math.min(drag.y0, drag.y1)}px" style:width="{Math.abs(drag.x1 - drag.x0)}px" style:height="{Math.abs(drag.y1 - drag.y0)}px"></div>
  {/if}
  {#if pending}
    <div class="pop" style:left="{Math.min(pending.x, size.w - 250)}px" style:top="{Math.min(pending.y + 6, size.h - 150)}px" role="dialog" aria-label="Save a mark" tabindex="-1" onpointerdown={(e) => e.stopPropagation()} onmouseup={(e) => e.stopPropagation()}>
      <div class="dots">
        {#each COLORS as c}<button class="dot {c}" class:on={pick === c} aria-label={c} onclick={() => (pick = c)}></button>{/each}
      </div>
      <textarea bind:value={note} rows="2" placeholder={pending.kind === "area" ? "Your note on this part of the page" : "A note (optional)"}></textarea>
      <div class="acts">
        <button onclick={cancel}>Cancel</button>
        <button class="primary" onclick={save} disabled={pending.kind === "area" && !note.trim()}>{pending.kind === "area" ? "Save note" : "Highlight"}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .frame { position: relative; flex: none; background: #fff; box-shadow: 0 2px 10px #0005; }
  .frame.area { cursor: crosshair; }
  .frame.area .textLayer { pointer-events: none; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; }
  .marks { position: absolute; inset: 0; pointer-events: none; }
  .mark { position: absolute; }
  .mark.highlight { mix-blend-mode: multiply; border-radius: 2px; }
  .mark.area { border: 2px dashed; border-radius: 3px; }
  .mark.highlight.yellow { background: #f0b81d80; }
  .mark.highlight.red { background: #d8312f55; }
  .mark.highlight.green { background: #1f9b4f55; }
  .mark.highlight.blue { background: #3b5ab055; }
  .mark.area.yellow { border-color: #c48f0f; background: #f0b81d22; }
  .mark.area.red { border-color: #d8312f; background: #d8312f1a; }
  .mark.area.green { border-color: #1f9b4f; background: #1f9b4f1a; }
  .mark.area.blue { border-color: #3b5ab0; background: #3b5ab01a; }
  .pin { position: absolute; z-index: 3; transform: translate(-60%, -60%); width: 1.3rem; height: 1.3rem; padding: 0; border-radius: 50%; border: 2px solid #fff; color: #fff; font-size: 0.7rem; line-height: 1; box-shadow: 0 1px 4px #0006; }
  .yellow { --c: #c48f0f; }
  .red { --c: #d8312f; }
  .green { --c: #1f9b4f; }
  .blue { --c: #3b5ab0; }
  .pin, .dot { background: var(--c); }
  .drag { position: absolute; z-index: 3; border: 2px dashed var(--accent); background: #3b5ab022; pointer-events: none; }
  .pop { position: absolute; z-index: 4; width: 240px; display: grid; gap: 0.4rem; padding: 0.5rem; border-radius: 8px; background: var(--panel); color: var(--text); border: 1px solid var(--line); box-shadow: 0 6px 20px #0006; }
  .pop textarea { width: 100%; box-sizing: border-box; resize: vertical; font: inherit; }
  .dots { display: flex; gap: 0.35rem; }
  .dot { width: 1.3rem; height: 1.3rem; padding: 0; border-radius: 50%; border: 2px solid transparent; }
  .dot.on { border-color: var(--text); }
  .acts { display: flex; justify-content: flex-end; gap: 0.4rem; }
  .reading { position: absolute; z-index: 3; top: 0.5rem; right: 0.5rem; padding: 0.15rem 0.5rem; border-radius: 4px; background: #000a; color: #fff; font-size: 0.8rem; }

  /* pdf.js's text layer: invisible text placed over the drawn page (its
     own rules, the part romforge needs). */
  .textLayer { position: absolute; inset: 0; overflow: clip; line-height: 1; z-index: 2; transform-origin: 0 0; --min-font-size: 1; --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size)); --min-font-size-inv: calc(1 / var(--min-font-size)); }
  .textLayer :global(span), .textLayer :global(br) { color: transparent; position: absolute; white-space: pre; cursor: text; transform-origin: 0% 0%; user-select: text; }
  .textLayer > :global(:not(.markedContent)), .textLayer :global(.markedContent span:not(.markedContent)) { --font-height: 0; font-size: calc(var(--text-scale-factor) * var(--font-height)); --scale-x: 1; --rotate: 0deg; transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv)); }
  .textLayer :global(.markedContent) { display: contents; }
  .textLayer :global(span.hit) { background: #3b5ab055; border-radius: 2px; }
  .textLayer :global(::selection) { background: #3b5ab055; color: transparent; }
  .textLayer :global(.endOfContent) { display: block; position: absolute; inset: 100% 0 0; user-select: none; cursor: default; }
  .textLayer:global(.selecting) :global(.endOfContent) { top: 0; }
</style>
