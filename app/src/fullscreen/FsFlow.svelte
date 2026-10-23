<script>
  import { thumbSrc, artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { theme, themeVars } from "./theme.svelte.js";
  import { gameKey } from "./fshome.js";

  // The cover flow layout: a platform's games as one row of box art, the
  // focused one in the middle facing out, the others turned away on
  // either side. Only SIDE covers each side are drawn (the outermost
  // faded out), keyed by game so they slide when the focus moves. `rows`
  // is bound back as the covers in view, so the shoulders page by them.
  let { games, index, focused, art = "boxart", icons = {}, fav = new Set(), rows = $bindable(7), pick, open } = $props();

  const SIDE = 4;
  let w = $state(1200);
  let h = $state(600);
  let el = $state(null);
  // The card's shape is the theme's (--fs-card-ratio), as in the grid.
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  let ratio = $derived.by(() => {
    const set = theme.applied && parseFloat(themeVars(theme.applied, dark).vars["--fs-card-ratio"]);
    if (set) return set;
    void (w + h + theme.cssLoads);
    return (el && parseFloat(getComputedStyle(el).getPropertyValue("--fs-card-ratio"))) || 0.74;
  });
  // Tall enough to fill the row, narrow enough for three a side.
  let coverH = $derived(Math.max(120, Math.min(h * 0.82, w / 4.4 / ratio)));
  let coverW = $derived(coverH * ratio);
  $effect(() => {
    rows = 2 * SIDE - 1;
  });
  let shown = $derived(
    games.slice(Math.max(0, index - SIDE), index + SIDE + 1).map((r, k) => ({ r, d: Math.max(0, index - SIDE) + k - index })),
  );

  const picture = (r) => r.pictures?.[art] ?? r.pictures?.boxart ?? null;
</script>

<div class="flow" data-fs-flow data-fs-focus={focused ? "grid" : undefined} bind:clientWidth={w} bind:clientHeight={h} bind:this={el}>
  {#if !games.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    <div class="stage" style="--cover-w: {coverW}px; --cover-h: {coverH}px">
      {#each shown as { r, d } (r.id)}
        {@const i = index + d}
        {@const src = thumbSrc(picture(r), coverH)}
        <button
          class="card"
          class:focus={focused && d === 0}
          class:side={d !== 0}
          style="--d: {d}; --s: {Math.sign(d)}; --a: {Math.abs(d)}"
          data-fs-card={r.id}
          data-fs-selected={d === 0 ? "" : undefined}
          data-fs-art={src ? undefined : "missing"}
          onclick={() => (d === 0 ? open() : pick(i))}
          aria-label={listLabel(r)}
          tabindex={Math.abs(d) === SIDE ? -1 : undefined}
        >
          {#if fav.has(gameKey(r))}<span class="star" data-fs-fav aria-label="Favourite">★</span>{/if}
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
  .flow { position: relative; min-height: 0; height: 100%; perspective: var(--fs-flow-perspective, 110vh); }
  .stage { position: absolute; inset: 0; transform-style: preserve-3d; }
  /* Placed by its distance from the middle (--d, its sign --s and size
     --a): the first step clears the middle cover, later ones overlap. */
  .card {
    position: absolute;
    left: calc(50% - var(--cover-w) / 2);
    top: calc(50% - var(--cover-h) / 2);
    display: grid;
    align-items: end;
    justify-items: center;
    width: var(--cover-w);
    height: var(--cover-h);
    padding: 0;
    border: 0;
    border-radius: var(--fs-radius);
    background: none;
    cursor: pointer;
    z-index: calc(10 - var(--a));
    opacity: calc(1 - max(0, var(--a) - 3));
    transform:
      translateX(calc(var(--cover-w) * (var(--s) * 0.5 + var(--d) * var(--fs-flow-spread, 0.42))))
      translateZ(calc(var(--s) * var(--s) * -1 * var(--fs-flow-depth, 14vh)))
      rotateY(calc(var(--s) * -1 * var(--fs-flow-angle, 55deg)));
    transition: transform 0.25s ease, opacity 0.25s ease;
  }
  .card.side { filter: brightness(var(--fs-flow-dim, 0.7)); }
  .card img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: calc(var(--fs-radius) * 0.6); }
  /* An outline, not a border, as in the grid (WebKitGTK and transforms). */
  .card.focus { outline: 0.5vh solid var(--fs-focus); outline-offset: 0.5vh; }
  .placeholder { box-sizing: border-box; display: grid; place-items: center; width: 100%; height: 100%; padding: 1vh; border-radius: calc(var(--fs-radius) * 0.8); background: var(--fs-panel); border: 1px solid var(--fs-line); color: var(--fs-text); font-size: calc(2.2vh * var(--fs-scale)); font-weight: 600; text-align: center; overflow: hidden; }
  .placeholder.icon { grid-template-rows: 1fr auto; gap: 1vh; }
  .placeholder.icon img { max-width: 60%; max-height: 100%; border-radius: 0; }
  .star { position: absolute; top: 0.6vh; right: 0.6vh; z-index: 1; color: var(--fs-accent); font-size: calc(2.6vh * var(--fs-scale)); text-shadow: 0 0 0.6vh var(--fs-shadow); }
  .count { position: absolute; right: 0; bottom: -3.4vh; margin: 0; color: var(--fs-muted); font-size: calc(1.8vh * var(--fs-scale)); }
  .none { margin: 6vh 0; text-align: center; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
  @media (prefers-reduced-motion: reduce) {
    .card { transition: none; }
  }
</style>
