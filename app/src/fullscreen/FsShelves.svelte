<script>
  import { thumbSrc, artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { theme, cardRatio } from "./theme.svelte.js";
  import { gameKey } from "./fshome.js";

  // The shelves layout: a platform's games as rows (fshome.js'
  // shelvesFor: played lately, favourites, all), each a strip of box art
  // under its name; the focused row (`row`, at `col`) larger. The rows in
  // view start one above the focused one; each strip moves only when its
  // focus would leave it. `rows` is bound back as the tiles a strip shows,
  // so the shoulders page along it.
  let { shelves, row, col, focused, art = "boxart", icons = {}, fav = new Set(), dark = false, rows = $bindable(6), pick, open } = $props();

  let w = $state(1200);
  let h = $state(600);
  let screenH = $state(typeof innerHeight === "number" ? innerHeight : 800);
  // The focused row's tiles, and the others' (both as heights, the others
  // 0.6 of it): what the rows in view leave once their names (2vh text and
  // a 0.8vh gap each), the 1.6vh between rows and the focus ring's room
  // (1vh) are taken away.
  let shown = $derived(Math.max(1, Math.min(3, shelves.length)));
  let vh = $derived(screenH / 100);
  let spare = $derived(h - shown * (2.6 * vh * (theme.applied?.scale ?? 1) + 0.8 * vh) - (shown - 1) * 1.6 * vh - vh);
  let big = $derived(Math.max(60, Math.round(spare / (1 + 0.6 * (shown - 1)))));
  let small = $derived(Math.round(big * 0.6));
  // The card's shape, the theme's (--fs-card-ratio), to count what fits.
  let el = $state(null);
  let ratio = $derived((void (w + h), cardRatio(el, dark)));
  let gap = $derived(Math.round(h * 0.03));
  const fit = (tileH) => Math.max(2, Math.floor((w + gap) / (tileH * ratio + gap)));
  $effect(() => {
    rows = fit(big);
  });
  let top = $derived(Math.max(0, Math.min(row - 1, shelves.length - 3)));
  let inView = $derived(shelves.slice(top, top + 3).map((s, k) => ({ s, r: top + k })));
  // Each row's first tile drawn, by its key, kept while the focus moves.
  let firsts = $state({});
  $effect(() => {
    const s = shelves[row];
    if (!s) return;
    const n = fit(big);
    const f = firsts[s.key] ?? 0;
    if (col < f) firsts[s.key] = col;
    else if (col > f + n - 1) firsts[s.key] = col - n + 1;
  });
  const picture = (r) => r.pictures?.[art] ?? r.pictures?.boxart ?? null;
</script>

<svelte:window bind:innerHeight={screenH} />

<div class="shelves" data-fs-shelves data-fs-focus={focused ? "grid" : undefined} bind:clientWidth={w} bind:clientHeight={h} bind:this={el} style="--gap: {gap}px">
  {#if !shelves.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    {#each inView as { s, r } (s.key)}
      {@const on = r === row}
      {@const tileH = on ? big : small}
      {@const first = Math.min(firsts[s.key] ?? 0, Math.max(0, s.games.length - 1))}
      <section class="shelf" class:on data-fs-shelf={s.key}>
        <h3 class="name">{s.name} <span class="n" aria-live={on ? "polite" : undefined}>{on ? `${col + 1} / ${s.games.length}` : s.games.length}</span></h3>
        <div class="strip" style="--tile-h: {tileH}px">
          {#each s.games.slice(first, first + fit(tileH)) as g, k (g.id)}
            {@const c = first + k}
            {@const sel = on && c === col}
            {@const src = thumbSrc(picture(g), tileH)}
            <button class="card" class:focus={focused && sel} data-fs-card={g.id} data-fs-selected={sel ? "" : undefined} data-fs-art={src ? undefined : "missing"} onclick={() => (sel ? open() : pick(r, c))} aria-label={fav.has(gameKey(g)) ? `${listLabel(g)}, favourite` : listLabel(g)}>
              {#if fav.has(gameKey(g))}<span class="star" data-fs-fav aria-hidden="true">★</span>{/if}
              {#if src}
                <img {src} alt="" loading="lazy" decoding="async" />
              {:else if theme.applied?.fallback === "platform" && icons[g.system]}
                <span class="placeholder"><img class="icon" src={artSrc(icons[g.system])} alt="" /></span>
              {:else}
                <span class="placeholder" data-fs-title>{listLabel(g)}</span>
              {/if}
            </button>
          {/each}
        </div>
      </section>
    {/each}
  {/if}
</div>

<style>
  .shelves { position: relative; display: grid; align-content: start; gap: 1.6vh; min-height: 0; height: 100%; overflow: hidden; }
  .shelf { display: grid; gap: 0.8vh; min-width: 0; opacity: 0.75; }
  .shelf.on { opacity: 1; }
  .name { margin: 0; color: var(--fs-muted); font-size: calc(2vh * var(--fs-scale)); font-weight: 600; }
  .shelf.on .name { color: var(--fs-text); }
  .n { color: var(--fs-muted); font-weight: 400; }
  .strip { display: flex; gap: var(--gap); height: var(--tile-h); padding: 0 0.6vh; }
  .card { position: relative; display: grid; grid-template: minmax(0, 1fr) / minmax(0, 1fr); place-items: end center; flex: none; width: calc(var(--tile-h) * var(--fs-card-ratio)); height: 100%; padding: 0; border: 0; border-radius: var(--fs-radius); background: none; cursor: pointer; }
  .card img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: calc(var(--fs-radius) * 0.6); filter: drop-shadow(0 0.6vh 1vh color-mix(in srgb, var(--fs-shadow) 75%, transparent)); }
  .card.focus { outline: 0.5vh solid var(--fs-focus); outline-offset: 0.4vh; }
  .placeholder { display: grid; place-items: center; width: 100%; height: 100%; padding: 0.8vh; border-radius: calc(var(--fs-radius) * 0.8); background: var(--fs-panel); border: 1px solid var(--fs-line); color: var(--fs-text); font-size: calc(1.8vh * var(--fs-scale)); font-weight: 600; text-align: center; overflow: hidden; }
  .placeholder .icon { max-width: 60%; filter: none; }
  .star { position: absolute; top: 0.4vh; right: 0.4vh; z-index: 1; color: var(--fs-accent); font-size: calc(2.2vh * var(--fs-scale)); text-shadow: 0 0 0.6vh var(--fs-shadow); }
  .none { margin: 6vh 0; text-align: center; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
</style>
