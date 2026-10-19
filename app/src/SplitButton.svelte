<script>
  // A toolbar button with a ▾ beside it: the button does its main job,
  // the arrow opens a menu of the others (a dry run, say).
  let { glyph, label, title = "", disabled = false, onclick, items = [] } = $props();
  let menu = $state(false);
  let root = $state(null);

  function pick(item) {
    menu = false;
    item.onclick();
  }
</script>

<svelte:window
  onclick={(e) => menu && root && !root.contains(e.target) && (menu = false)}
  onkeydown={(e) => e.key === "Escape" && menu && (menu = false)}
/>

<div class="split" bind:this={root}>
  <button class="tool main" {onclick} {disabled} {title}><span class="glyph">{glyph}</span>{label}</button>
  <button class="tool arrow" onclick={() => (menu = !menu)} {disabled} aria-haspopup="menu" aria-expanded={menu} aria-label="More: {label}" title="More">▾</button>
  {#if menu}
    <div class="menu" role="menu">
      {#each items as item}
        <button role="menuitem" onclick={() => pick(item)} title={item.title ?? ""}>{item.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .split { position: relative; display: flex; align-items: stretch; }
  .tool { display: flex; flex-direction: column; align-items: center; border: none; padding: 0.2rem 0.6rem; font-size: 0.8rem; gap: 0.1rem; background: none; }
  .tool:hover:not(:disabled) { background: var(--accent-bg); }
  .main { padding-right: 0.35rem; }
  .arrow { justify-content: center; padding: 0.2rem 0.3rem; font-size: 0.75rem; color: var(--muted); border-left: 1px solid transparent; }
  .split:hover .arrow { border-left-color: var(--line); }
  .glyph { font-size: 1.25rem; line-height: 1.2rem; }
  .menu { position: absolute; top: 100%; left: 0; z-index: 20; min-width: 100%; margin-top: 2px; padding: 0.25rem; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; box-shadow: 0 6px 18px #0005; display: flex; flex-direction: column; }
  .menu button { border: none; background: none; text-align: left; padding: 0.35rem 0.6rem; white-space: nowrap; border-radius: 4px; font-size: 0.85rem; }
  .menu button:hover { background: var(--accent-bg); }
</style>
