<script>
  import { genres } from "../facts.js";
  import { displayTitle, released, played } from "../labels.js";

  // A game's own page in the fullscreen mode: its fanart dimmed behind,
  // the box, the logo (else the title), facts, the overview, and its
  // buttons, the focused one marked. `detail` is the game's details (null
  // while they load).
  let { game, detail, buttons, focus, press } = $props();
  let mine = $derived(detail?.row?.id === game.id ? detail : null);
  let f = $derived(mine?.fields ?? {});
  let art = $derived(mine?.art ?? {});
</script>

<section class="page" data-fs-screen="game">
  {#if art.hero}<img class="hero" src={art.hero} alt="" />{/if}
  <div class="body">
    <div class="box">
      {#if art.box3d || art.boxart}
        <img src={art.box3d || art.boxart} alt="" />
      {:else}
        <span class="nobox" data-fs-title>{displayTitle(game)}</span>
      {/if}
    </div>
    <div class="text">
      {#if art.logo}
        <img class="logo" src={art.logo} alt={displayTitle(game)} />
      {:else}
        <h1 data-fs-title>{displayTitle(game)}</h1>
      {/if}
      <p class="meta">{[game.system_name, released(f) || game.year, f.users && `${f.users} players`].filter(Boolean).join(" · ")}</p>
      {#if f.genre}
        <p class="genres">{#each genres(f.genre) as g}<span>{@html g.svg} {g.name}</span>{/each}</p>
      {/if}
      {#if f.developer || f.publisher}
        <p class="meta">{[f.developer, f.publisher].filter((v, i, a) => v && a.indexOf(v) === i).join(" · ")}</p>
      {/if}
      {#if mine?.play}<p class="meta">Played {played(mine.play)}</p>{/if}
      {#if f.description}<p class="overview">{f.description}</p>{/if}
      <div class="buttons" role="toolbar" aria-label="Game">
        {#each buttons as b, i}
          <button class:focus={i === focus} class:primary={b.primary} data-fs-button={b.id} onclick={() => press(i)}>{b.label}</button>
        {/each}
      </div>
    </div>
  </div>
</section>

<style>
  .page { position: relative; height: 100%; overflow: hidden; }
  .hero { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: 0.22; filter: blur(0.4vh); }
  .body { position: relative; display: grid; grid-template-columns: minmax(0, 34vw) minmax(0, 1fr); gap: 4vw; height: 100%; padding: 4vh 5vw; align-items: center; }
  .box { display: grid; place-items: center; height: 100%; max-height: 66vh; }
  .box img { max-width: 100%; max-height: 66vh; object-fit: contain; filter: drop-shadow(0 2vh 2.4vh var(--fs-shadow)); }
  .nobox { display: grid; place-items: center; width: 70%; aspect-ratio: calc(var(--fs-card-ratio) * 36 / 37); padding: 2vh; border-radius: var(--fs-radius); background: var(--fs-panel); border: 1px solid var(--fs-line); font-size: calc(3vh * var(--fs-scale)); font-weight: 600; text-align: center; }
  .text { display: grid; align-content: center; gap: 1.6vh; min-width: 0; }
  .logo { max-width: 70%; max-height: 16vh; object-fit: contain; justify-self: start; }
  h1 { margin: 0; font-size: calc(5vh * var(--fs-scale)); line-height: 1.1; }
  .meta { margin: 0; color: var(--fs-muted); font-size: calc(2.2vh * var(--fs-scale)); }
  .genres { display: flex; flex-wrap: wrap; gap: 1vw; margin: 0; font-size: calc(2vh * var(--fs-scale)); }
  .genres span { display: inline-flex; align-items: center; gap: 0.4vw; }
  .overview { margin: 0; max-height: 24vh; overflow: hidden; font-size: calc(2.1vh * var(--fs-scale)); line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 7; -webkit-box-orient: vertical; }
  .buttons { display: flex; gap: 1.2vw; margin-top: 1.6vh; }
  .buttons button { padding: 1.2vh 2.4vw; border: 0.3vh solid var(--fs-line); border-radius: var(--fs-radius); background: var(--fs-panel); color: var(--fs-text); font-size: calc(2.4vh * var(--fs-scale)); cursor: pointer; }
  .buttons button.primary { background: var(--fs-accent); border-color: var(--fs-accent); color: var(--fs-accent-text); }
  .buttons button.focus { outline: 0.45vh solid var(--fs-focus); outline-offset: 0.4vh; }
</style>
