<script>
  import { thumbSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { gameKey } from "./fshome.js";
  import { sizeOf, shelfScale, shelfFirst, shelfFits } from "./fsshelf.js";

  // The bookshelf layout: a bookcase of wooden shelves (fsshelf.js'
  // bookcase: endless ones or shelves of thirty, by platform or mixed,
  // favourites and lately played first if asked for), each a name plate
  // on its board and boxes stood side by side, spines out, each at its
  // real size (box_size) and all to one scale; its spine art, else one
  // drawn in its platform's colour with the title down it. The focused box
  // (on shelf `row`, at `col`) slides out a little; Accept takes it off
  // the shelf (FsBox). Two shelves are in view, the case moving only when
  // the focus leaves them; each shelf draws the boxes that fit, moving
  // only when its focus would leave it. `rows` is bound back as the boxes
  // a shelf shows, so the shoulders page by them. `accents`: platform
  // key → its colour.
  let { shelves, row, col, focused, accents = {}, fav = new Set(), rows = $bindable(10), pick, open } = $props();

  let w = $state(1200);
  let h = $state(400);
  const GAP = 2;
  // The shelves in view and each one's height.
  let inView = $derived(Math.min(2, Math.max(1, shelves.length)));
  let shelfH = $derived(h / inView);
  // One scale for the whole case: its tallest box fills a shelf, less
  // the board and headroom.
  let scale = $derived(shelfScale(shelves.flatMap((s) => s.games), shelfH * 0.78));
  const width = (r) => Math.max(8, sizeOf(r)[2] * scale) + GAP;
  let top = $state(0);
  $effect(() => {
    if (row < top) top = row;
    else if (row > top + inView - 1) top = row - inView + 1;
  });
  // Each shelf's first box drawn, by its key, kept while the focus moves.
  let firsts = $state({});
  $effect(() => {
    const s = shelves[row];
    if (!s) return;
    const ws = s.games.map(width);
    firsts[s.key] = shelfFirst(ws, col, Math.min(firsts[s.key] ?? 0, col), w - 40);
    rows = shelfFits(ws, firsts[s.key], w - 40);
  });
  let shown = $derived(
    shelves.slice(top, top + inView).map((s, k) => {
      const f = Math.min(firsts[s.key] ?? 0, Math.max(0, s.games.length - 1));
      return { s, r: top + k, f, games: s.games.slice(f, f + shelfFits(s.games.map(width), f, w - 40) + 1) };
    }),
  );
</script>

<div class="case" data-fs-bookshelf data-fs-focus={focused ? "grid" : undefined} bind:clientWidth={w} bind:clientHeight={h} style="--gap: {GAP}px; --shelf-h: {shelfH}px">
  {#if !shelves.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    {#each shown as { s, r, f, games } (s.key)}
      <section class="shelf" data-fs-shelf={s.key}>
        <div class="boxes">
          {#each games as g, k (g.id)}
            {@const c = f + k}
            {@const sel = r === row && c === col}
            {@const [, bh, bd] = sizeOf(g)}
            {@const spine = g.pictures?.spine ? thumbSrc(g.pictures.spine, bh * scale) : null}
            <button
              class="box"
              class:sel
              class:focus={focused && sel}
              style="--w: {Math.max(8, bd * scale)}px; --h: {bh * scale}px; --c: {accents[g.system] ?? '#5c5c6a'}"
              data-fs-card={g.id}
              data-fs-selected={sel ? "" : undefined}
              data-fs-art={spine ? undefined : "missing"}
              onclick={() => (sel ? open() : pick(r, c))}
              aria-label={listLabel(g)}
            >
              {#if spine}
                <img src={spine} alt="" decoding="async" />
              {:else}
                <span class="drawn" data-fs-title>{listLabel(g)}</span>
              {/if}
              {#if fav.has(gameKey(g))}<span class="star" data-fs-fav aria-label="Favourite">★</span>{/if}
            </button>
          {/each}
        </div>
        <div class="board" aria-hidden="true"><span class="plate">{s.name}{r === row ? ` · ${col + 1} / ${s.games.length}` : ""}</span></div>
      </section>
    {/each}
  {/if}
</div>

<style>
  /* The case: a dark back panel with grain, the board in front. */
  .case {
    position: relative;
    min-height: 0;
    height: 100%;
    padding: 0 20px;
    overflow: hidden;
    border-radius: calc(var(--fs-radius) * 0.6);
    background:
      repeating-linear-gradient(90deg, #0000 0 37px, #0000000d 37px 39px, #0000 39px 83px, #ffffff08 83px 84px),
      linear-gradient(180deg, var(--fs-shelf-back, #3b2416), color-mix(in srgb, var(--fs-shelf-back, #3b2416) 70%, #000));
    box-shadow: inset 0 2vh 4vh #0006;
  }
  .shelf { position: relative; height: var(--shelf-h); }
  .boxes { position: absolute; left: 0; right: 0; bottom: 12%; display: flex; align-items: flex-end; gap: var(--gap); }
  .board {
    position: absolute;
    inset: auto -20px 0;
    display: grid;
    place-items: center;
    height: 12%;
    background:
      repeating-linear-gradient(90deg, #0000 0 120px, #00000014 120px 123px, #0000 123px 260px),
      linear-gradient(180deg, color-mix(in srgb, var(--fs-shelf-wood, #9a6a3c) 70%, #fff), var(--fs-shelf-wood, #9a6a3c) 30%, color-mix(in srgb, var(--fs-shelf-wood, #9a6a3c) 60%, #000));
    box-shadow: 0 -0.6vh 1.2vh #0007;
  }
  /* A box, spine out: its depth across, its height up. */
  .box {
    position: relative;
    display: grid;
    flex: none;
    width: var(--w);
    height: var(--h);
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 2px;
    background: linear-gradient(90deg, color-mix(in srgb, var(--c) 70%, #000), var(--c) 20%, var(--c) 80%, color-mix(in srgb, var(--c) 60%, #000));
    box-shadow: inset 0 0 0 1px #0004;
    cursor: pointer;
    transition: transform 0.2s ease, filter 0.2s ease;
  }
  .box img { width: 100%; height: 100%; object-fit: fill; }
  .drawn {
    place-self: center;
    max-height: 92%;
    overflow: hidden;
    color: #fff;
    font-size: clamp(9px, calc(var(--w) * 0.5), 2.4vh);
    font-weight: 700;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-shadow: 0 1px 2px #0008;
    writing-mode: vertical-rl;
  }
  /* The focused box slides out towards you. */
  .box.sel { z-index: 1; transform: translateY(-3%) scale(1.06); filter: brightness(1.12); box-shadow: inset 0 0 0 1px #0004, 0 1.6vh 2.4vh #000a; }
  .box.focus { outline: 0.4vh solid var(--fs-focus); outline-offset: 0.3vh; }
  .star { position: absolute; top: 0.4vh; left: 50%; translate: -50% 0; color: var(--fs-accent); font-size: calc(1.8vh * var(--fs-scale)); text-shadow: 0 0 0.6vh #000; }
  /* A brass name plate on the board. */
  .plate { padding: 0.2vh 1.2vw; border-radius: 2px; background: linear-gradient(180deg, #e8cf8a, #b8913f); color: #3a2a10; font-size: calc(1.6vh * var(--fs-scale)); font-weight: 600; white-space: nowrap; box-shadow: 0 1px 2px #0008, inset 0 1px 0 #fff8; }
  .none { margin: 6vh 0; text-align: center; color: #f3e6d2; font-size: calc(2.4vh * var(--fs-scale)); }
  @media (prefers-reduced-motion: reduce) {
    .box { transition: none; }
  }
</style>
