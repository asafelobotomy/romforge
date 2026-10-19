<script>
  import { artSrc } from "./lib.js";
  import { iconSvg } from "./facts.js";

  // The platforms down the left (all games, each platform, the
  // unidentified files), and the DATs and organize screens at the foot.
  // `icons`: system key -> file of its picture; one that fails to load
  // keeps its badge, also after the next load (no retry, no flicker).
  let { lib, games, view, platform = $bindable(), icons, iconStyle, show } = $props();
  let failed = $state(new Set());
</script>

<nav class="platforms" aria-label="Platforms">
  <button class="plat" class:active={view === "library" && platform === "*"} onclick={() => { show("library"); platform = "*"; }}>
    <span class="icon all">{@html iconSvg("library")}</span><span class="pname">All games</span><span class="count">{games}</span>
  </button>
  {#each lib.platforms as p}
    <button
      class="plat"
      class:active={view === "library" && platform === (p.system ?? "")}
      onclick={() => { show("library"); platform = p.system ?? ""; }}
      title={p.name}
    >
      {#if !p.system}
        <span class="icon unknown">{@html iconSvg("unknown")}</span>
      {:else if icons[p.system] && !failed.has(icons[p.system])}
        <img class="picon" class:logo={iconStyle === "logo"} src={artSrc(icons[p.system])} alt="" onerror={() => (failed = new Set(failed).add(icons[p.system]))} />
      {:else}
        <span class="icon" data-maker={p.manufacturer ?? "none"}>{p.short}</span>
      {/if}
      <span class="pname">{p.name}</span>
      <span class="count">{p.games}</span>
    </button>
  {/each}
  <span class="spacer"></span>
  <button class="plat small" class:active={view === "dats"} onclick={() => show("dats")}>DATs</button>
  <button class="plat small" class:active={view === "bios"} onclick={() => show("bios")}>BIOS</button>
  <button class="plat small" class:active={view === "organize"} onclick={() => show("organize")}>Organize</button>
</nav>

<style>
  .platforms { display: flex; flex-direction: column; gap: 0.15rem; padding: 0.4rem; border-right: 1px solid var(--line); overflow-y: auto; }
  .plat { display: flex; flex-direction: column; align-items: center; border: none; padding: 0.4rem 0.2rem; border-radius: 6px; gap: 0.15rem; }
  .plat:hover { background: var(--accent-bg); }
  .plat.active { background: var(--accent); color: white; }
  .plat.small { font-size: 0.8rem; padding: 0.3rem; }
  .icon { width: 3rem; height: 2.2rem; border-radius: 6px; display: grid; place-items: center; font-size: 0.7rem; font-weight: 700; letter-spacing: 0.02em; color: white; background: #6b7078; }
  .picon { width: 3rem; height: 2.4rem; object-fit: contain; }
  /* Logos are often dark lettering on nothing: on a light tile. */
  .picon.logo { box-sizing: border-box; padding: 0.2rem 0.25rem; background: #f4f4f4; border-radius: 6px; }
  .icon[data-maker="Nintendo"] { background: #c62828; }
  .icon[data-maker="Sega"] { background: #1e5bb8; }
  .icon[data-maker="Sony"] { background: #3d3d48; }
  .icon[data-maker="Atari"] { background: #b25d00; }
  .icon[data-maker="NEC"] { background: #6a3fb5; }
  .icon[data-maker="SNK"] { background: #2b7a5a; }
  .icon[data-maker="Microsoft"] { background: #3b8a2e; }
  .icon[data-maker="Commodore"] { background: #4a6a8a; }
  .icon.all { background: var(--accent); }
  /* Files no platform could be found for. */
  .icon.unknown { background: #6b7078; }
  /* A platform's whole name, wrapped (never cut short), in bold. */
  .pname { font-size: 0.72rem; font-weight: 700; text-align: center; line-height: 1.15; white-space: normal; overflow-wrap: anywhere; hyphens: auto; max-width: 100%; }
  .count { font-size: 0.7rem; color: var(--muted); }
  .plat.active .count { color: #fffc; }
</style>
