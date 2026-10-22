<script>
  import { untrack } from "svelte";
  import { artSrc } from "../lib.js";
  import { listLabel } from "../listLabel.js";

  // Attract mode (stage 2, step 6): games in turn, full screen (those with
  // fanart first, else box art), with their logo or name, platform and
  // year; a new one every 8 s. The screensaver dims it to black. `shown`
  // is told the game on screen, so waking up lands on it.
  let { all, saver = false, shown } = $props();

  const pics = (r) => r.pictures ?? {};
  let pool = $derived.by(() => {
    const art = all.filter((r) => pics(r).hero || pics(r).boxart);
    const hero = art.filter((r) => pics(r).hero);
    return hero.length >= 5 ? hero : art.length ? art : all;
  });
  let game = $state(null);
  let n = $state(0);
  function next() {
    if (!pool.length) return;
    let g = pool[Math.floor(Math.random() * pool.length)];
    if (pool.length > 1 && g === game) g = pool[(pool.indexOf(g) + 1) % pool.length];
    game = g;
    n++;
    shown(g);
  }
  // `next` reads and writes state: untracked, or the effect would re-run
  // itself without end.
  $effect(() => {
    untrack(next);
    const t = setInterval(next, 8000);
    return () => clearInterval(t);
  });
</script>

<section class="attract" class:saver data-fs-attract data-fs-screensaver={saver ? "" : undefined} aria-hidden="true">
  {#key n}
    {#if game}
      <div class="slide">
        {#if pics(game).hero}
          <img class="hero" src={artSrc(pics(game).hero)} alt="" />
        {:else if pics(game).boxart}
          <img class="box" src={artSrc(pics(game).boxart)} alt="" />
        {/if}
        <div class="caption">
          {#if pics(game).logo}<img class="logo" src={artSrc(pics(game).logo)} alt="" />{:else}<h1 data-fs-title>{listLabel(game)}</h1>{/if}
          <p>{[game.system_name, game.year].filter(Boolean).join(" · ")}</p>
        </div>
      </div>
    {/if}
  {/key}
  <p class="wake">Press any button</p>
</section>

<style>
  .attract { position: fixed; inset: 0; z-index: 70; background: #000; color: #fff; overflow: hidden; transition: filter 3s ease; }
  .saver { filter: brightness(0.06); }
  .slide { position: absolute; inset: 0; display: grid; place-items: center; animation: fade 1.2s ease; }
  .hero { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; animation: drift 8s linear; }
  .box { max-height: 70vh; max-width: 60vw; object-fit: contain; filter: drop-shadow(0 2vh 4vh #000c); }
  .caption { position: absolute; left: 5vw; bottom: 8vh; max-width: 50vw; padding: 2vh 2vw; border-radius: var(--fs-radius); background: linear-gradient(90deg, #000c, #0000); }
  .logo { max-height: 14vh; max-width: 40vw; object-fit: contain; }
  h1 { margin: 0; font-family: var(--fs-title-font); font-size: calc(5vh * var(--fs-scale)); }
  .caption p { margin: 1vh 0 0; font-size: calc(2.4vh * var(--fs-scale)); opacity: 0.85; }
  .wake { position: absolute; right: 4vw; bottom: 4vh; margin: 0; font-size: calc(1.8vh * var(--fs-scale)); opacity: 0.6; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes drift { from { transform: scale(1.08); } to { transform: scale(1); } }
  @media (prefers-reduced-motion: reduce) { .slide, .hero { animation: none; } }
</style>
