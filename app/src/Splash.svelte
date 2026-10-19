<script>
  import Wordmark from "./Wordmark.svelte";

  // The splash while a game starts (milestone 18, 1.6): the ROMFORGE
  // wordmark at the top, the game's box in the middle, turning slowly, and
  // its platform's logo at the bottom. A 3D box render (a picture of a box
  // at an angle) sways from side to side, since a full turn would show its
  // picture from behind; flat box art is wrapped onto a box with a spine
  // that turns all the way round. No motion when the system asks for less.
  let { title, emulator, platform = "", box3d = null, boxart = null, logo = null } = $props();

  // Flat box art's proportions, once it has loaded (a DVD case until then).
  let ratio = $state(0.7);
</script>

<div class="splash" role="status" aria-live="polite" aria-label="Starting {title}">
  <div class="head"><Wordmark height="2.6rem" /></div>

  <div class="stage">
    {#if box3d}
      <img class="render" src={box3d} alt="" />
    {:else if boxart}
      <div class="box" style:--w="calc(var(--h) * {ratio})">
        <img class="face front" src={boxart} alt="" onload={(e) => (ratio = e.currentTarget.naturalWidth / e.currentTarget.naturalHeight || 0.7)} />
        <img class="face back" src={boxart} alt="" />
        <span class="face side right"></span>
        <span class="face side left"></span>
        <span class="face cap lid"></span>
        <span class="face cap base"></span>
      </div>
    {:else}
      <div class="nobox">{title}</div>
    {/if}
    <span class="shadow" aria-hidden="true"></span>
  </div>

  <div class="foot">
    {#if logo}
      <span class="logo"><img src={logo} alt={platform} /></span>
    {:else if platform}
      <span class="platform">{platform}</span>
    {/if}
    <p><strong>{title}</strong> · starting {emulator}…</p>
  </div>
</div>

<style>
  .splash { position: fixed; inset: 0; z-index: 50; display: grid; grid-template-rows: auto 1fr auto; justify-items: center; padding: 4vh 2rem; background: radial-gradient(ellipse at 50% 55%, var(--panel), var(--bg) 70%); text-align: center; }
  .head { padding-top: 1vh; }
  .stage { --h: min(48vh, 28rem); --d: calc(var(--h) * 0.1); position: relative; display: grid; place-items: center; perspective: 1400px; width: 100%; }
  .shadow { position: absolute; bottom: calc(50% - var(--h) / 2 - 2.2rem); width: calc(var(--h) * 0.6); height: 1.4rem; border-radius: 50%; background: radial-gradient(#0007, #0000 70%); }

  /* A 3D box render: swaying, as if turning on a stand. */
  .render { height: var(--h); max-width: 70vw; object-fit: contain; filter: drop-shadow(0 18px 24px #0006); animation: sway 7s ease-in-out infinite alternate; }
  @keyframes sway { from { transform: rotateY(-28deg); } to { transform: rotateY(28deg); } }

  /* Flat box art on a box: front and back the art, a dark spine. */
  .box { position: relative; width: var(--w); height: var(--h); transform-style: preserve-3d; animation: spin 14s linear infinite; }
  .face { position: absolute; display: block; backface-visibility: hidden; }
  .front, .back { inset: 0; width: 100%; height: 100%; object-fit: cover; border-radius: 3px; }
  .front { transform: translateZ(calc(var(--d) / 2)); }
  .back { transform: rotateY(180deg) translateZ(calc(var(--d) / 2)); filter: brightness(0.55) saturate(0.7); }
  .side { top: 0; width: var(--d); height: 100%; left: calc(50% - var(--d) / 2); background: linear-gradient(90deg, #2b2b33, #44444f, #2b2b33); }
  .right { transform: rotateY(90deg) translateZ(calc(var(--w) / 2)); }
  .left { transform: rotateY(-90deg) translateZ(calc(var(--w) / 2)); }
  .cap { left: 0; width: 100%; height: var(--d); top: calc(50% - var(--d) / 2); background: #34343c; }
  .lid { transform: rotateX(90deg) translateZ(calc(var(--h) / 2)); }
  .base { transform: rotateX(-90deg) translateZ(calc(var(--h) / 2)); }
  @keyframes spin { from { transform: rotateX(-6deg) rotateY(0deg); } to { transform: rotateX(-6deg) rotateY(360deg); } }

  .nobox { display: grid; place-items: center; width: calc(var(--h) * 0.7); height: var(--h); padding: 1rem; border-radius: 6px; background: var(--panel); border: 1px solid var(--line); font-size: 1.4rem; font-weight: 600; }

  .foot { display: grid; justify-items: center; gap: 0.6rem; }
  /* The logo on nothing (its own transparency), large: the splash has the
     room. A faint light edge in dark mode keeps a dark logo readable. */
  .logo { display: grid; place-items: center; }
  .logo img { height: min(9rem, 16vh); width: auto; max-width: min(28rem, 70vw); object-fit: contain; }
  @media (prefers-color-scheme: dark) {
    .logo img { filter: drop-shadow(0 0 1px #fffa) drop-shadow(0 0 6px #fff3); }
  }
  .platform { font-size: 1.2rem; font-weight: 600; color: var(--muted); }
  .foot p { margin: 0; color: var(--muted); }

  @media (prefers-reduced-motion: reduce) {
    .render, .box { animation: none; }
    .box { transform: rotateX(-6deg) rotateY(-24deg); }
  }
</style>
