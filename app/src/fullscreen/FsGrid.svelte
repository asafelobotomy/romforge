<script>
  import { thumbSrc, artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { theme, cardRatio } from "./theme.svelte.js";
  import { gameKey } from "./fshome.js";

  // A platform's games as large cards, rows of `cols`, `rows` of them in
  // view. Only the rows in view are drawn, so thousands of games stay
  // quick; the focused card's row is always one of them. `cols` and
  // `rows` are worked out from the space there is and bound back, so the
  // focus model moves by them.
  let { games, index, focused, art = "boxart", icons = {}, fav = new Set(), dark = false, cols = $bindable(5), rows = $bindable(2), pick, open } = $props();

  let w = $state(1200);
  let h = $state(600);
  let el = $state(null);
  // The focused card grows (scale 1.05) and wears a ring outside it
  // (0.5vh offset + 0.5vh outline, more with a theme's shadow): ROOM keeps
  // that inside the grid, clear of the platforms above, and GAP keeps it
  // off the cards beside it. Both in vh, as the ring is.
  let screenH = $state(typeof innerHeight === "number" ? innerHeight : 800);
  let ROOM = $derived(Math.round(screenH * 0.03));
  let GAP = $derived(Math.max(18, Math.round(screenH * 0.036)));
  let rowsShown = $derived(h > 760 ? 3 : 2);
  let cardH = $derived(Math.max(120, (h - 2 * ROOM - GAP * (rowsShown - 1)) / rowsShown));
  // The card's shape is the theme's (--fs-card-ratio), read again as the
  // space changes.
  let ratio = $derived((void (w + h), cardRatio(el, dark)));
  let cardW = $derived(cardH * ratio);
  $effect(() => {
    cols = Math.max(1, Math.floor((w - 2 * ROOM + GAP) / (cardW + GAP)));
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

<svelte:window bind:innerHeight={screenH} />

<div class="grid" data-fs-grid data-fs-focus={focused ? "grid" : undefined} bind:clientWidth={w} bind:clientHeight={h} bind:this={el}>
  {#if !games.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    <div class="cards" style="--cols: {cols}; --card-h: {cardH}px; --card-w: {cardW}px; --gap: {GAP}px; --room: {ROOM}px">
      {#each shown as r, k (r.id)}
        {@const i = top * cols + k}
        {@const src = thumbSrc(picture(r), cardH)}
        <button class="card" class:focus={focused && i === index} class:sel={i === index} data-fs-card={r.id} data-fs-selected={i === index ? "" : undefined} data-fs-art={src ? undefined : "missing"} onclick={() => (i === index ? open() : pick(i))} aria-label={fav.has(gameKey(r)) ? `${listLabel(r)}, favourite` : listLabel(r)}>
          {#if fav.has(gameKey(r))}<span class="star" data-fs-fav aria-hidden="true">★</span>{/if}
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
  .cards { display: grid; grid-template-columns: repeat(var(--cols), var(--card-w)); grid-auto-rows: var(--card-h); gap: var(--gap); justify-content: center; padding: var(--room); }
  .card { position: relative; display: grid; place-items: center; padding: 0; border: 0; border-radius: var(--fs-radius); background: none; cursor: pointer; transition: transform 0.15s ease; }
  .card img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: calc(var(--fs-radius) * 0.6); filter: drop-shadow(0 0.8vh 1.2vh color-mix(in srgb, var(--fs-shadow) 75%, transparent)); }
  .card.sel { transform: scale(1.05); }
  /* An outline, not a border: WebKitGTK didn't draw a changed border on
     these scaled buttons. */
  .card.focus { outline: 0.5vh solid var(--fs-focus); outline-offset: 0.5vh; }
  .placeholder { box-sizing: border-box; display: grid; place-items: center; width: 100%; height: 100%; padding: 1vh; border-radius: calc(var(--fs-radius) * 0.8); background: var(--fs-panel); border: 1px solid var(--fs-line); color: var(--fs-text); font-size: calc(2vh * var(--fs-scale)); font-weight: 600; text-align: center; overflow: hidden; }
  .star { position: absolute; top: 0.6vh; right: 0.6vh; z-index: 1; color: var(--fs-accent); font-size: calc(2.6vh * var(--fs-scale)); text-shadow: 0 0 0.6vh var(--fs-shadow); }
  .placeholder.icon { grid-template-rows: 1fr auto; gap: 1vh; }
  .placeholder.icon img { max-width: 60%; max-height: 100%; filter: none; border-radius: 0; }
  .count { position: absolute; right: 0; bottom: -3.4vh; margin: 0; color: var(--fs-muted); font-size: calc(1.8vh * var(--fs-scale)); }
  .none { margin: 6vh 0; text-align: center; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
  @media (prefers-reduced-motion: reduce) {
    .card { transition: none; }
  }
</style>
