<script>
  import { artSrc } from "../lib.js";
  import { displayTitle, released } from "../labels.js";
  import { sizeOf } from "./fsshelf.js";

  // A box taken off the bookshelf: the game's box in 3D at its real
  // proportions, turned a quarter at a time (`turn`: 0 the front, 1 its
  // spine, 2 the back, 3 the other side). Its faces are its own art (box
  // art, spine, back), else drawn: the front its title, the spine its
  // title down it, the back its screenshots, overview and facts. `detail`
  // is the game's details (null while they load); `accent` its platform's
  // colour. By mouse: a click on the box's left or right half turns it,
  // one beside it puts it back (`close`), its button opens the game's page
  // (`open`); `turnBy(±1)` turns it. A modal dialog: it takes the focus
  // while out and gives it back when put away.
  let { game, detail, turn = 0, accent = "var(--fs-shelf-spine, #5c5c6a)", turnBy, close, open } = $props();
  let stage = $state(null);
  let boxEl = $state(null);
  $effect(() => {
    const before = document.activeElement;
    stage?.focus();
    return () => before?.focus?.();
  });
  function clicked(e) {
    const r = boxEl?.getBoundingClientRect();
    if (!r || e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) return close();
    turnBy(e.clientX < r.left + r.width / 2 ? -1 : 1);
  }
  let mine = $derived(detail?.row?.id === game.id ? detail : null);
  let art = $derived(mine?.art ?? {});
  let f = $derived(mine?.fields ?? {});
  let front = $derived(art.boxart ?? artSrc(game.pictures?.boxart));
  let spine = $derived(art.spine ?? artSrc(game.pictures?.spine));
  let shots = $derived([art.snap, art.title].filter(Boolean));
  // Its size on screen: the real one, as large as fits.
  let screenW = $state(1280);
  let screenH = $state(720);
  let mm = $derived(sizeOf(game));
  let k = $derived(Math.min((screenH * 0.66) / mm[1], (screenW * 0.42) / mm[0]));
  let facts = $derived(
    [released(f) || game.year, game.system_name, f.users && `${f.users} players`, f.developer].filter(Boolean).join(" · "),
  );
</script>

<svelte:window bind:innerWidth={screenW} bind:innerHeight={screenH} />

<!-- The pad's and keyboard's keys reach the mode (Fullscreen), which turns it. -->
<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
<div class="stage" data-fs-box role="dialog" aria-modal="true" aria-label="{displayTitle(game)}: box" tabindex="-1" bind:this={stage} onclick={clicked}>
  <div class="box" bind:this={boxEl} style="--bw: {mm[0] * k}px; --bh: {mm[1] * k}px; --bd: {mm[2] * k}px; --turn: {turn}; --c: {accent}">
    <div class="face front" data-fs-face="front">
      {#if front}<img src={front} alt="Front of the box" />{:else}<span class="drawn" data-fs-title>{displayTitle(game)}</span>{/if}
    </div>
    <div class="face back" data-fs-face="back">
      {#if art.boxback}
        <img src={art.boxback} alt="Back of the box" />
      {:else}
        <div class="drawn-back">
          <h2 data-fs-title>{displayTitle(game)}</h2>
          {#if shots.length}<div class="shots">{#each shots as s}<img src={s} alt="" />{/each}</div>{/if}
          {#if f.description}<p class="blurb">{f.description}</p>{/if}
          {#if facts}<p class="facts">{facts}</p>{/if}
        </div>
      {/if}
    </div>
    <div class="face side right" data-fs-face="spine">
      {#if spine}<img src={spine} alt="Spine of the box" />{:else}<span class="drawn spine-text">{displayTitle(game)}</span>{/if}
    </div>
    <div class="face side left" aria-hidden="true"></div>
    <div class="face cap top" aria-hidden="true"></div>
    <div class="face cap bottom" aria-hidden="true"></div>
  </div>
  <div class="actions">
    <button onclick={(e) => (e.stopPropagation(), turnBy(-1))} aria-label="Turn it left">◀</button>
    <button class="go" onclick={(e) => (e.stopPropagation(), open())}>Open the game</button>
    <button onclick={(e) => (e.stopPropagation(), turnBy(1))} aria-label="Turn it right">▶</button>
  </div>
</div>

<style>
  .stage { position: absolute; inset: 0; z-index: 4; display: grid; place-items: center; align-content: center; gap: 4vh; background: var(--fs-scrim); perspective: 160vh; outline: 0; }
  .actions { display: flex; gap: 1.2vw; }
  .actions button { padding: 0.8vh 1.4vw; border: 1px solid var(--fs-line); border-radius: var(--fs-radius); background: var(--fs-panel); color: inherit; font: inherit; font-size: calc(2vh * var(--fs-scale)); cursor: pointer; }
  .actions .go { border-color: var(--fs-accent); }
  /* The box turns about its middle; a little off square so its depth shows. */
  .box {
    position: relative;
    width: var(--bw);
    height: var(--bh);
    transform-style: preserve-3d;
    transform: rotateX(-4deg) rotateY(calc(var(--turn) * -90deg - 16deg));
    transition: transform 0.45s ease;
  }
  .face { position: absolute; display: grid; overflow: hidden; backface-visibility: hidden; background: linear-gradient(135deg, color-mix(in srgb, var(--c) 80%, #fff), var(--c) 40%, color-mix(in srgb, var(--c) 70%, #000)); box-shadow: inset 0 0 0 1px #0005; }
  /* A scan keeps its shape on the box's real one (a jewel case's insert
     sits inside its hinge); the spine fills its side. */
  .face > img { width: 100%; height: 100%; object-fit: contain; }
  .side > img { object-fit: fill; }
  .front, .back { inset: 0; }
  .front { transform: translateZ(calc(var(--bd) / 2)); }
  .back { transform: rotateY(180deg) translateZ(calc(var(--bd) / 2)); }
  .side { top: 0; left: calc(50% - var(--bd) / 2); width: var(--bd); height: var(--bh); }
  .right { transform: rotateY(90deg) translateZ(calc(var(--bw) / 2)); }
  .left { transform: rotateY(-90deg) translateZ(calc(var(--bw) / 2)); }
  .cap { left: 0; top: calc(50% - var(--bd) / 2); width: var(--bw); height: var(--bd); filter: brightness(0.8); }
  .top { transform: rotateX(90deg) translateZ(calc(var(--bh) / 2)); }
  .bottom { transform: rotateX(-90deg) translateZ(calc(var(--bh) / 2)); }
  .drawn { place-self: center; padding: 6%; color: #fff; font-size: calc(var(--bh) * 0.07); font-weight: 700; text-align: center; text-shadow: 0 2px 6px #0009; }
  .spine-text { padding: 0; font-size: calc(var(--bd) * 0.5); white-space: nowrap; writing-mode: vertical-rl; }
  .drawn-back { display: grid; align-content: start; gap: calc(var(--bh) * 0.025); padding: 7%; background: color-mix(in srgb, var(--c) 35%, #111); color: #f4f0e8; font-size: calc(var(--bh) * 0.026); line-height: 1.4; }
  .drawn-back h2 { margin: 0; font-size: 1.8em; line-height: 1.1; }
  .shots { display: grid; grid-auto-flow: column; gap: 4%; }
  .shots img { width: 100%; aspect-ratio: 4 / 3; object-fit: cover; border-radius: 2px; box-shadow: 0 2px 6px #0008; }
  .blurb { margin: 0; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 9; overflow: hidden; }
  .facts { margin: 0; opacity: 0.8; font-size: 0.9em; }
  @media (prefers-reduced-motion: reduce) {
    .box { transition: none; }
  }
</style>
