<script>
  import { thumbSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { gameKey } from "./fshome.js";

  // The wheel layout: a platform's games as their logos (else titles) on
  // a wheel down the right, the focused one large in the middle, the
  // others smaller and turned away along its curve. The game's fanart
  // fills the screen behind (the root's backdrop). Only SIDE a side are
  // drawn, keyed so they turn; `rows` is bound back as those in view, so
  // the shoulders page by them.
  let { games, index, focused, fav = new Set(), rows = $bindable(7), pick, open } = $props();

  const SIDE = 4;
  let h = $state(600);
  // Each step's height, as a share of the wheel's.
  let step = $derived(h / (2 * SIDE + 1));
  $effect(() => {
    rows = 2 * SIDE - 1;
  });
  let shown = $derived(
    games.slice(Math.max(0, index - SIDE), index + SIDE + 1).map((r, k) => ({ r, d: Math.max(0, index - SIDE) + k - index })),
  );
</script>

<div class="wheel" data-fs-wheel data-fs-focus={focused ? "grid" : undefined} bind:clientHeight={h}>
  {#if !games.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    {#each shown as { r, d } (r.id)}
      {@const logo = r.pictures?.logo ? thumbSrc(r.pictures.logo, step * (d === 0 ? 1.8 : 1)) : null}
      <button
        class="item"
        class:focus={focused && d === 0}
        style="--d: {d}; --s: {Math.sign(d)}; --a: {Math.abs(d)}; --step: {step}px"
        data-fs-card={r.id}
        data-fs-selected={d === 0 ? "" : undefined}
        data-fs-art={logo ? undefined : "missing"}
        onclick={() => (d === 0 ? open() : pick(index + d))}
        aria-label={fav.has(gameKey(r)) ? `${listLabel(r)}, favourite` : listLabel(r)}
        tabindex={Math.abs(d) === SIDE ? -1 : undefined}
      >
        {#if logo}
          <img src={logo} alt="" decoding="async" />
        {:else}
          <span class="name" data-fs-title>{listLabel(r)}</span>
        {/if}
        {#if fav.has(gameKey(r))}<span class="star" data-fs-fav aria-hidden="true">★</span>{/if}
      </button>
    {/each}
    <p class="count" aria-live="polite">{index + 1} / {games.length}</p>
  {/if}
</div>

<style>
  .wheel { position: relative; min-height: 0; height: 100%; overflow: hidden; }
  /* Placed by its distance from the middle (--d, its sign --s, its size
     --a): one step each, clear of the focused one (1.8 steps tall); the
     curve pushes the far ones out to the right, and they shrink and fade. */
  .item {
    position: absolute;
    top: calc(50% + var(--d) * var(--step) - (0.5 - 0.4 * var(--s)) * var(--step));
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.6vw;
    height: var(--step);
    padding: 0.6vh 1vw;
    border: 0;
    border-radius: var(--fs-radius);
    background: none;
    color: var(--fs-text);
    cursor: pointer;
    opacity: calc(1 - var(--a) * 0.2 - max(0, var(--a) - 3) * 0.2);
    transform: translateX(calc(var(--a) * var(--a) * var(--fs-wheel-curve, 1.2vw))) scale(calc(1 - var(--a) * 0.12));
    transition: transform 0.25s ease, top 0.25s ease, opacity 0.25s ease;
  }
  .item[data-fs-selected] { top: calc(50% - var(--step) * 0.9); height: calc(var(--step) * 1.8); }
  .item img { max-width: 100%; max-height: 100%; object-fit: contain; filter: drop-shadow(0 0.4vh 0.8vh var(--fs-shadow)); }
  .name { overflow: hidden; font-size: calc(2.6vh * var(--fs-scale)); font-weight: 600; text-align: center; text-overflow: ellipsis; white-space: nowrap; text-shadow: 0 0.2vh 0.8vh var(--fs-shadow); }
  .item[data-fs-selected] .name { font-size: calc(3.4vh * var(--fs-scale)); }
  .item.focus { outline: 0.4vh solid var(--fs-focus); outline-offset: 0.2vh; }
  .star { color: var(--fs-accent); font-size: calc(2.2vh * var(--fs-scale)); }
  .count { position: absolute; right: 0; bottom: 0; margin: 0; color: var(--fs-muted); font-size: calc(1.8vh * var(--fs-scale)); }
  .none { margin: 6vh 0; text-align: center; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
  @media (prefers-reduced-motion: reduce) {
    .item { transition: none; }
  }
</style>
