<script>
  import { invoke } from "@tauri-apps/api/core";
  import { artSrc } from "../lib.js";

  // The platforms along the top of the fullscreen mode, the focused one in
  // the middle: its large logo (Wikidata's, fetched once for those in view
  // and cached), else its icon, else its short name.
  let { platforms, index, focused, icons = {}, iconStyle = "", pick } = $props();

  let logos = $state({});
  const asked = new Set();
  $effect(() => {
    for (let d = -3; d <= 3; d++) {
      const p = platforms[(index + d + platforms.length) % platforms.length];
      const sys = p?.system;
      if (!sys || sys === "*" || asked.has(sys)) continue;
      asked.add(sys);
      invoke("platform_logo", { system: sys })
        .then((file) => file && (logos[sys] = artSrc(file)))
        .catch(() => {});
    }
  });
  const icon = (p) => (p.system && icons[p.system] ? artSrc(icons[p.system]) : null);
</script>

<nav class="carousel" data-fs-carousel data-fs-focus={focused ? "carousel" : undefined} aria-label="Platforms">
  <ol class="strip" style="--at: {index}">
    {#each platforms as p, i (p.system ?? i)}
      <li>
        <button class="plat" class:sel={i === index} class:focus={focused && i === index} data-fs-platform={p.system} onclick={() => pick(i)} aria-current={i === index ? "true" : undefined}>
          <span class="mark">
            {#if p.system === "*"}
              <span class="all">All games</span>
            {:else if logos[p.system]}
              <img src={logos[p.system]} alt={p.name} />
            {:else if icon(p)}
              <img class="icon" class:tile={iconStyle === "logo"} src={icon(p)} alt={p.name} />
            {:else}
              <span class="short">{p.short || p.name}</span>
            {/if}
          </span>
          <span class="name">{p.name} · {p.games}</span>
        </button>
      </li>
    {/each}
  </ol>
</nav>

<style>
  .carousel { overflow: hidden; padding: 2.4vh 0 1.2vh; }
  /* The focused platform in the middle: the strip slides under it. */
  .strip { --w: 17vw; display: flex; gap: 1.5vw; margin: 0; padding: 0; list-style: none; transform: translateX(calc(50vw - var(--w) / 2 - var(--at) * (var(--w) + 1.5vw))); transition: transform 0.25s ease; }
  .plat { display: grid; justify-items: center; gap: 0.6vh; width: var(--w); padding: 1vh 0.5vw; border: 0.3vh solid transparent; border-radius: calc(var(--fs-radius) * 1.2); background: none; color: var(--fs-muted); cursor: pointer; opacity: 0.55; transition: opacity 0.2s, transform 0.2s; }
  .plat.sel { opacity: 1; color: var(--fs-text); transform: scale(1.08); }
  .plat.focus { outline: 0.4vh solid var(--fs-focus); outline-offset: -0.4vh; background: color-mix(in srgb, var(--fs-focus) 12%, transparent); }
  .mark { display: grid; place-items: center; height: 10vh; width: 100%; }
  .mark img { max-height: 10vh; max-width: 100%; object-fit: contain; }
  .mark img.icon { max-height: 8vh; }
  .mark img.tile { padding: 0.5vh; background: #f4f4f4; border-radius: calc(var(--fs-radius) * 0.6); }
  .all, .short { font-size: calc(3.4vh * var(--fs-scale)); font-weight: 700; letter-spacing: 0.05em; }
  .name { font-size: calc(1.9vh * var(--fs-scale)); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  @media (prefers-color-scheme: dark) {
    .mark img:not(.icon) { filter: drop-shadow(0 0 1px #fffa) drop-shadow(0 0 6px #fff3); }
  }
  @media (prefers-reduced-motion: reduce) {
    .strip, .plat { transition: none; }
  }
</style>
