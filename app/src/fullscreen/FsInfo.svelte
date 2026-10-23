<script>
  import { genres, esrb, stars, regionFlags } from "../facts.js";
  import { displayTitle, released, played, edition } from "../labels.js";

  // The focused game, beside the grid (under the covers in cover flow):
  // its logo (else its title), platform and release date, region and
  // edition, genre, players, modes, studios, ratings, play time, what it
  // was made from and its overview, each only when `shown` (the keys of
  // fsdetails.js' DETAILS) has it. Each carries data-fs-detail for
  // themes. `detail` is the game's details once they have loaded (null
  // until then).
  let { game, detail, shown = [] } = $props();
  let mine = $derived(detail?.row?.id === game?.id ? detail : null);
  let f = $derived(mine?.fields ?? {});
  const on = (k) => shown.includes(k);
  let flags = $derived(on("edition") ? regionFlags(mine?.name) : []);
  let ed = $derived(on("edition") ? edition(mine?.name, true) : "");
  let rating = $derived(f.esrb ? esrb(f.esrb) : null);
  let score = $derived(f.community_rating ? stars(f.community_rating) : null);
  let meta = $derived(
    [on("platform") && ["platform", game?.system_name], on("released") && ["released", released(f) || game?.year]].filter((m) => m && m[1]),
  );
</script>

<aside class="info" data-fs-info aria-live="polite">
  {#if game}
    {#if on("logo")}
      {#if mine?.art?.logo}
        <img class="logo" src={mine.art.logo} alt={displayTitle(game)} data-fs-detail="logo" />
      {:else}
        <h2 data-fs-title data-fs-detail="logo">{displayTitle(game)}</h2>
      {/if}
    {/if}
    {#if meta.length}
      <p class="meta">{#each meta as [k, v], i}{#if i}<span aria-hidden="true">{" · "}</span>{/if}<span data-fs-detail={k}>{v}</span>{/each}</p>
    {/if}
    {#if flags.length || ed}
      <p class="meta" data-fs-detail="edition">
        {#each flags as r}{#if r.flag}<span class="flag" title={r.name} aria-label={r.name}>{r.flag}</span>{:else}<span>{r.name}</span>{/if}{/each}
        {#if ed}<span>{ed}</span>{/if}
      </p>
    {/if}
    {#if on("genre") && f.genre}
      <p class="genres" data-fs-detail="genre">
        {#each genres(f.genre) as g}<span class="genre">{@html g.svg} {g.name}</span>{/each}
      </p>
    {/if}
    <dl>
      {#if on("players") && f.users}<div data-fs-detail="players"><dt>Players</dt><dd>{f.users}</dd></div>{/if}
      {#if on("modes") && f.game_mode}<div data-fs-detail="modes"><dt>Modes</dt><dd>{f.game_mode}</dd></div>{/if}
      {#if on("developer") && f.developer}<div data-fs-detail="developer"><dt>Developer</dt><dd>{f.developer}</dd></div>{/if}
      {#if on("publisher") && f.publisher}<div data-fs-detail="publisher"><dt>Publisher</dt><dd>{f.publisher}</dd></div>{/if}
      {#if on("rating") && f.esrb}<div data-fs-detail="rating"><dt>Rating</dt><dd>{rating ? `ESRB ${rating.code}` : f.esrb}</dd></div>{/if}
      {#if on("community") && f.community_rating}
        <div data-fs-detail="community">
          <dt>Community</dt>
          <dd>
            {#if score}<span class="stars" aria-label="{score.value} out of {score.max} stars"><span class="on" style="width: {score.pct}%">★★★★★</span>★★★★★</span> {score.value}{:else}{f.community_rating}{/if}
          </dd>
        </div>
      {/if}
      {#if on("played") && mine?.play}<div data-fs-detail="played"><dt>Played</dt><dd>{played(mine.play)}</dd></div>{/if}
    </dl>
    {#if on("derived") && mine?.derived}
      <p class="meta" data-fs-detail="derived">{mine.derived.relation === "translation" ? "Translation of" : "Hack of"} {mine.derived.base}</p>
    {/if}
    {#if on("overview") && f.description}<p class="overview" data-fs-detail="overview">{f.description}</p>{/if}
    {#if !game.playable}<p class="note">{game.packed ? "Still in its archive: Import unpacks it." : "Not playable yet."}</p>{/if}
  {/if}
</aside>

<style>
  .info { display: grid; align-content: start; gap: 1.4vh; min-width: 0; padding: 1vh 1vw; }
  .logo { max-width: 100%; max-height: 14vh; object-fit: contain; justify-self: start; }
  h2 { margin: 0; font-size: calc(3.6vh * var(--fs-scale)); line-height: 1.15; }
  .meta { margin: 0; color: var(--fs-muted); font-size: calc(2.1vh * var(--fs-scale)); }
  .flag { margin-right: 0.5vw; }
  .genres { display: flex; flex-wrap: wrap; gap: 0.6vh 1.2vw; margin: 0; font-size: calc(1.9vh * var(--fs-scale)); }
  .genre { display: inline-flex; align-items: center; gap: 0.4vw; }
  dl { display: grid; gap: 0.8vh; margin: 0; font-size: calc(1.9vh * var(--fs-scale)); }
  dl div { display: grid; grid-template-columns: 9vw 1fr; gap: 0.6vw; }
  dt { color: var(--fs-muted); }
  dd { margin: 0; }
  .stars { position: relative; display: inline-block; color: color-mix(in srgb, var(--fs-muted) 45%, transparent); }
  .stars .on { position: absolute; inset: 0 auto 0 0; overflow: hidden; white-space: nowrap; color: var(--fs-accent); }
  .overview { margin: 0; color: var(--fs-text); font-size: calc(1.9vh * var(--fs-scale)); line-height: 1.4; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: var(--fs-overview-lines, 6); overflow: hidden; }
  .note { margin: 0; color: var(--fs-warn); font-size: calc(1.9vh * var(--fs-scale)); }
</style>
