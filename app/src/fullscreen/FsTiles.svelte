<script>
  import { thumbSrc, artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { theme } from "./theme.svelte.js";
  import { gameKey } from "./fshome.js";

  // The hero row layouts' strip (along the top, or the bottom in
  // hero-bottom): a platform's games as small tiles of box art, the
  // focused one larger; the game's fanart fills the screen behind (the
  // root's backdrop) and its details sit beside the strip. As many tiles
  // as fit are drawn, the strip moving only when the focus would leave it;
  // `rows` is bound back as that many, so the shoulders page by them.
  let { games, index, focused, art = "boxart", icons = {}, fav = new Set(), rows = $bindable(6), pick, open } = $props();

  let w = $state(1200);
  let screenH = $state(typeof innerHeight === "number" ? innerHeight : 800);
  let small = $derived(Math.round(screenH * 0.13));
  let big = $derived(Math.round(screenH * 0.2));
  let gap = $derived(Math.round(screenH * 0.015));
  let fits = $derived(Math.max(2, Math.floor((w - big + small) / (small + gap))));
  $effect(() => {
    rows = fits;
  });
  // The first tile drawn: the focus keeps one tile after it where it can.
  let first = $state(0);
  $effect(() => {
    if (index < first) first = index;
    else if (index > first + fits - 2) first = Math.max(0, Math.min(index - fits + 2, games.length - fits));
  });
  let shown = $derived(games.slice(first, first + fits));
  const picture = (r) => r.pictures?.[art] ?? r.pictures?.boxart ?? null;
</script>

<svelte:window bind:innerHeight={screenH} />

<div class="tiles" data-fs-tiles data-fs-focus={focused ? "grid" : undefined} bind:clientWidth={w} style="--small: {small}px; --big: {big}px; --gap: {gap}px">
  {#if !games.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    {#each shown as r, k (r.id)}
      {@const i = first + k}
      {@const src = thumbSrc(picture(r), i === index ? big : small)}
      <button class="card" class:sel={i === index} class:focus={focused && i === index} data-fs-card={r.id} data-fs-selected={i === index ? "" : undefined} data-fs-art={src ? undefined : "missing"} onclick={() => (i === index ? open() : pick(i))} aria-label={fav.has(gameKey(r)) ? `${listLabel(r)}, favourite` : listLabel(r)}>
        {#if fav.has(gameKey(r))}<span class="star" data-fs-fav aria-hidden="true">★</span>{/if}
        {#if src}
          <img {src} alt="" loading="lazy" decoding="async" />
        {:else if theme.applied?.fallback === "platform" && icons[r.system]}
          <span class="placeholder"><img class="icon" src={artSrc(icons[r.system])} alt="" /></span>
        {:else}
          <span class="placeholder" data-fs-title>{listLabel(r)}</span>
        {/if}
      </button>
    {/each}
    <p class="count" aria-live="polite">{index + 1} / {games.length}</p>
  {/if}
</div>

<style>
  /* Room above and below for the focused tile's ring (0.5vh outline at
     0.4vh), clear of the platforms' edge and the help bar. */
  .tiles { position: relative; display: flex; align-items: flex-end; gap: var(--gap); min-width: 0; height: calc(var(--big) + 4vh); padding: 2vh 0.6vh; }
  .card { position: relative; display: grid; grid-template: minmax(0, 1fr) / minmax(0, 1fr); place-items: end center; flex: none; width: var(--small); height: var(--small); padding: 0; border: 0; border-radius: var(--fs-radius); background: none; cursor: pointer; transition: width 0.2s ease, height 0.2s ease; }
  .card.sel { width: var(--big); height: var(--big); }
  .card img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: calc(var(--fs-radius) * 0.6); filter: drop-shadow(0 0.6vh 1vh color-mix(in srgb, var(--fs-shadow) 75%, transparent)); }
  .card.focus { outline: 0.5vh solid var(--fs-focus); outline-offset: 0.4vh; }
  .placeholder { display: grid; place-items: center; width: 100%; height: 100%; padding: 0.6vh; border-radius: calc(var(--fs-radius) * 0.8); background: var(--fs-panel); border: 1px solid var(--fs-line); color: var(--fs-text); font-size: calc(1.6vh * var(--fs-scale)); font-weight: 600; text-align: center; overflow: hidden; }
  .placeholder .icon { max-width: 70%; filter: none; }
  .star { position: absolute; top: 0.4vh; right: 0.4vh; z-index: 1; color: var(--fs-accent); font-size: calc(2.2vh * var(--fs-scale)); text-shadow: 0 0 0.6vh var(--fs-shadow); }
  .count { position: absolute; right: 0; bottom: -1vh; margin: 0; color: var(--fs-muted); font-size: calc(1.8vh * var(--fs-scale)); }
  .none { margin: 2vh 0; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
  @media (prefers-reduced-motion: reduce) {
    .card { transition: none; }
  }
</style>
