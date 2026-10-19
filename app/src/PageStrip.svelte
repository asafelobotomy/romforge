<script>
  // The manual's pages as a strip of small thumbnails, drawn only when
  // they scroll into view (a 100-page scan stays quick). Clicking one
  // goes there.
  let { doc, pages, page, go } = $props();

  const HEIGHT = 110;

  // Draw a page's thumbnail into `canvas` once it is visible.
  function thumb(canvas, n) {
    let done = false;
    const seen = new IntersectionObserver(async (entries) => {
      if (done || !entries.some((e) => e.isIntersecting)) return;
      done = true;
      seen.disconnect();
      const p = await doc.getPage(n);
      const base = p.getViewport({ scale: 1 });
      const scale = (HEIGHT * (window.devicePixelRatio || 1)) / base.height;
      const viewport = p.getViewport({ scale });
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      await p.render({ canvas, viewport }).promise;
    });
    seen.observe(canvas);
    return { destroy: () => seen.disconnect() };
  }

  // Keep the current page's thumbnail in view.
  function current(el, active) {
    const show = (a) => a && el.scrollIntoView({ block: "nearest", inline: "nearest" });
    show(active);
    return { update: show };
  }
</script>

<div class="strip" role="listbox" aria-label="Pages">
  {#each Array.from({ length: pages }, (_, i) => i + 1) as n (n)}
    <button role="option" aria-selected={n === page} class:on={n === page} onclick={() => go(n)} title="Page {n}" use:current={n === page}>
      <canvas use:thumb={n} style:height="{HEIGHT}px"></canvas>
      <span>{n}</span>
    </button>
  {/each}
</div>

<style>
  .strip { display: flex; gap: 0.4rem; overflow-x: auto; padding: 0.4rem 0.2rem; border-top: 1px solid var(--line); }
  .strip button { display: grid; justify-items: center; gap: 0.15rem; padding: 0.2rem; border: 2px solid transparent; border-radius: 4px; background: none; flex: none; }
  .strip button.on { border-color: var(--accent); }
  .strip canvas { width: auto; background: #fff; box-shadow: 0 1px 4px #0004; }
  .strip span { font-size: 0.75rem; color: var(--muted); }
</style>
