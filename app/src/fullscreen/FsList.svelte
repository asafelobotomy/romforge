<script>
  import { thumbSrc, artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";
  import { theme } from "./theme.svelte.js";

  // The list layout: a platform's games as one column of titles, the
  // focused one's box art large beside it. Only the rows in view are
  // drawn; `rows` (how many) is bound back so the shoulders page by it.
  let { games, index, focused, art = "boxart", icons = {}, fav = new Set(), rows = $bindable(8), pick, open } = $props();

  let h = $state(600);
  const ROW = 0.062; // of the list's height: about 16 rows on a TV
  let rowH = $derived(Math.max(28, h * ROW * (theme.applied?.scale ?? 1)));
  let shownRows = $derived(Math.max(3, Math.floor(h / rowH)));
  $effect(() => {
    rows = shownRows;
  });
  let top = $state(0);
  $effect(() => {
    if (index < top) top = index;
    else if (index > top + shownRows - 1) top = index - shownRows + 1;
  });
  let shown = $derived(games.slice(top, top + shownRows));
  let game = $derived(games[index] ?? null);
  let picH = $derived(Math.round(h));
  let src = $derived(game ? thumbSrc(game.pictures?.[art] ?? game.pictures?.boxart ?? null, picH) : null);
  let icon = $derived(game?.system && icons[game.system] ? artSrc(icons[game.system]) : null);
</script>

<div class="list" data-fs-list data-fs-focus={focused ? "grid" : undefined} bind:clientHeight={h}>
  {#if !games.length}
    <p class="none">No games on this platform yet.</p>
  {:else}
    <ol class="rows" style="--row-h: {rowH}px">
      {#each shown as r, k (r.id)}
        {@const i = top + k}
        <li>
          <button class="row" class:sel={i === index} class:focus={focused && i === index} data-fs-row={r.id} data-fs-selected={i === index ? "" : undefined} onclick={() => (i === index ? open() : pick(i))}>
            <span data-fs-title>{listLabel(r)}</span>
            {#if fav.has(`${r.system}\t${r.title}`)}<span class="star" data-fs-fav aria-label="Favourite">★</span>{/if}
            {#if r.year}<span class="year">{r.year}</span>{/if}
          </button>
        </li>
      {/each}
    </ol>
    <figure class="art" data-fs-art={src ? undefined : "missing"}>
      {#if src}
        <img {src} alt="" decoding="async" />
      {:else if theme.applied?.fallback === "platform" && icon}
        <img class="icon" src={icon} alt="" />
      {:else if game}
        <span class="placeholder" data-fs-title>{listLabel(game)}</span>
      {/if}
    </figure>
    <p class="count" aria-live="polite">{index + 1} / {games.length}</p>
  {/if}
</div>

<style>
  .list { position: relative; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 0.9fr); gap: 2vw; min-height: 0; height: 100%; }
  .rows { list-style: none; margin: 0; padding: 0; display: grid; grid-auto-rows: var(--row-h); align-content: start; min-width: 0; }
  .row { display: flex; align-items: center; gap: 1vw; width: 100%; height: 100%; padding: 0 1.2vw; border: 0; border-radius: calc(var(--fs-radius) * 0.6); background: none; color: var(--fs-text); font-family: var(--fs-title-font); font-size: calc(2.3vh * var(--fs-scale)); text-align: left; cursor: pointer; overflow: hidden; }
  .row span[data-fs-title] { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row.sel { background: var(--fs-accent); color: var(--fs-accent-text); }
  .row.focus { outline: 0.4vh solid var(--fs-focus); outline-offset: 0.2vh; }
  .star { color: inherit; }
  .year { color: inherit; opacity: 0.7; font-size: 0.85em; }
  .art { margin: 0; display: grid; place-items: center; min-height: 0; }
  .art img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: calc(var(--fs-radius) * 0.6); filter: drop-shadow(0 0.8vh 1.2vh color-mix(in srgb, var(--fs-shadow) 75%, transparent)); }
  .art img.icon { max-width: 40%; filter: none; }
  .placeholder { display: grid; place-items: center; width: 70%; aspect-ratio: var(--fs-card-ratio); padding: 2vh; border-radius: var(--fs-radius); background: var(--fs-panel); border: 1px solid var(--fs-line); font-size: calc(2.6vh * var(--fs-scale)); font-weight: 600; text-align: center; }
  .count { position: absolute; right: 0; bottom: -3.4vh; margin: 0; color: var(--fs-muted); font-size: calc(1.8vh * var(--fs-scale)); }
  .none { margin: 6vh 0; text-align: center; color: var(--fs-muted); font-size: calc(2.4vh * var(--fs-scale)); }
</style>
