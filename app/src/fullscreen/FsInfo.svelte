<script>
  import { genres } from "../facts.js";
  import { released, played } from "../labels.js";

  // The focused game, beside the grid: its logo (else its title), the
  // platform and year, genre, players and play time. `detail` is the
  // game's details once they have loaded (null until then).
  let { game, detail } = $props();
  let f = $derived(detail?.row?.id === game?.id ? (detail.fields ?? {}) : {});
  let logo = $derived(detail?.row?.id === game?.id ? detail.art?.logo : null);
</script>

<aside class="info" data-fs-info aria-live="polite">
  {#if game}
    {#if logo}
      <img class="logo" src={logo} alt={game.name?.title || game.title} />
    {:else}
      <h2 data-fs-title>{game.name?.title || game.label || game.title}</h2>
    {/if}
    <p class="meta">{[game.system_name, released(f) || game.year].filter(Boolean).join(" · ")}</p>
    {#if f.genre}
      <p class="genres">
        {#each genres(f.genre) as g}<span class="genre">{@html g.svg} {g.name}</span>{/each}
      </p>
    {/if}
    <dl>
      {#if f.users}<div><dt>Players</dt><dd>{f.users}</dd></div>{/if}
      {#if f.developer}<div><dt>Developer</dt><dd>{f.developer}</dd></div>{/if}
      {#if detail?.play && detail.row?.id === game.id}<div><dt>Played</dt><dd>{played(detail.play)}</dd></div>{/if}
    </dl>
    {#if !game.playable}<p class="note">{game.packed ? "Still in its archive: Import unpacks it." : "Not playable yet."}</p>{/if}
  {/if}
</aside>

<style>
  .info { display: grid; align-content: start; gap: 1.4vh; min-width: 0; padding: 1vh 1vw; }
  .logo { max-width: 100%; max-height: 14vh; object-fit: contain; justify-self: start; }
  h2 { margin: 0; font-size: calc(3.6vh * var(--fs-scale)); line-height: 1.15; }
  .meta { margin: 0; color: var(--fs-muted); font-size: calc(2.1vh * var(--fs-scale)); }
  .genres { display: flex; flex-wrap: wrap; gap: 0.6vh 1.2vw; margin: 0; font-size: calc(1.9vh * var(--fs-scale)); }
  .genre { display: inline-flex; align-items: center; gap: 0.4vw; }
  dl { display: grid; gap: 0.8vh; margin: 0; font-size: calc(1.9vh * var(--fs-scale)); }
  dl div { display: grid; grid-template-columns: 9vw 1fr; gap: 0.6vw; }
  dt { color: var(--fs-muted); }
  dd { margin: 0; }
  .note { margin: 0; color: var(--fs-warn); font-size: calc(1.9vh * var(--fs-scale)); }
</style>
