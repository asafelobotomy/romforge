<script>
  import { thumbSrc, artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { theme } from "./theme.svelte.js";

  // A platform's games as large cards, rows of `cols`, `rows` of them in
  // view. Only the rows in view are drawn, so thousands of games stay
  // quick; the focused card's row is always one of them. `cols` and
  // `rows` are worked out from the space there is and bound back, so the
  // focus model moves by them.
  let { games, index, focused, art = "boxart", icons = {}, cols = $bindable(5), rows = $bindable(2), pick, open } = $props();

  let w = $state(1200);
  let h = $state(600);
  let el = $state(null);
  const GAP = 18;
  let rowsShown = $derived(h > 760 ? 3 : 2);
  let cardH = $derived(Math.max(120, (h - GAP * (rowsShown - 1)) / rowsShown));
  // The card's shape is the theme's (--fs-card-ratio), read again as the
  // space or the theme changes (after its CSS has had a frame to apply).
  let frame = $state(0);
  $effect(() => {
    void theme.applied;
    const t = setTimeout(() => frame++, 120);
    return () => clearTimeout(t);
  });
  let ratio = $derived.by(() => {
    void (w + h + frame);
    return (el && parseFloat(getComputedStyle(el).getPropertyValue("--fs-card-ratio"))) || 0.74;
  });
  let cardW = $derived(cardH * ratio);
  $effect(() => {
    cols = Math.max(1, Math.floor((w + GAP) / (cardW + GAP)));
    rows = rowsShown;
  });

  // The first row drawn: moves only when the focus would leave the view.
  let top = $state(0);
  $effect(() => {
    const r = Math.floor(index / Math.max(1, cols));
    if (r < top) top = r;
    else if (r > top + rowsShown - 1) top = r - rowsShown + 1;
  });
  let shown = $derived(games.slice(top * cols, (top + rowsShown) * cols));

  const picture = (r) => r.pictures?.[art] ?? r.pictures?.boxart ?? null;
</script>

<div class="grid" data-fs-grid data-fs-focus={focused ? "grid" : undefined} bind:clientWidth={w} bind:clientHeight={h} bind:this={el}>
  {#if !games.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    <div class="cards" style="--cols: {cols}; --card-h: {cardH}px; --card-w: {cardW}px; --gap: {GAP}px">
      {#each shown as r, k (r.id)}
        {@const i = top * cols + k}
        {@const src = thumbSrc(picture(r), cardH)}
        <button class="card" class:focus={focused && i === index} class:sel={i === index} data-fs-card={r.id} data-fs-selected={i === index ? "" : undefined} data-fs-art={src ? undefined : "missing"} onclick={() => (i === index ? open() : pick(i))} aria-label={listLabel(r)}>
          {#if src}
            <img {src} alt="" loading="lazy" decoding="async" />
          {:else if theme.applied?.fallback === "platform" && icons[r.system]}
            <span class="placeholder icon"><img src={artSrc(icons[r.system])} alt="" /><span data-fs-title>{listLabel(r)}</span></span>
          {:else}
            <span class="placeholder" data-fs-title>{listLabel(r)}</span>
          {/if}
        </button>
      {/each}
    </div>
    <p class="count" aria-live="polite">{index + 1} / {games.length}</p>
  {/if}
</div>

<style>
  .grid { position: relative; min-height: 0; height: 100%; }
  .cards { display: grid; grid-template-columns: repeat(var(--cols), var(--card-w)); grid-auto-rows: var(--card-h); gap: var(--gap); justify-content: center; }
  .card { position: relative; display: grid; place-items: center; padding: 0; border: 0; border-radius: var(--fs-radius); background: none; cursor: pointer; transition: transform 0.15s ease; }
  .card img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: calc(var(--fs-radius) * 0.6); filter: drop-shadow(0 0.8vh 1.2vh color-mix(in srgb, var(--fs-shadow) 75%, transparent)); }
  .card.sel { transform: scale(1.05); }
  /* An outline, not a border: WebKitGTK didn't draw a changed border on
     these scaled buttons. */
  .card.focus { outline: 0.5vh solid var(--fs-focus); outline-offset: 0.5vh; }
  .placeholder { box-sizing: border-box; display: grid; place-items: center; width: 100%; height: 100%; padding: 1vh; border-radius: calc(var(--fs-radius) * 0.8); background: var(--fs-panel); border: 1px solid var(--fs-line); color: var(--fs-text); font-size: calc(2vh * var(--fs-scale)); font-weight: 600; text-align: center; overflow: hidden; }
  .placeholder.icon { grid-template-rows: 1fr auto; gap: 1vh; }
  .placeholder.icon img { max-width: 60%; max-height: 100%; filter: none; border-radius: 0; }
  .count { position: absolute; right: 0; bottom: -3.4vh; margin: 0; color: var(--fs-muted); font-size: calc(1.8vh * var(--fs-scale)); }
  .none { margin: 6vh 0; text-align: center; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
  @media (prefers-reduced-motion: reduce) {
    .card { transition: none; }
  }
</style>
