<script>
  // A menu over the fullscreen mode (Start's, Select's, a game's options,
  // a question): a title and a list, the focused item marked.
  let { menu, press } = $props();
</script>

<div class="scrim" data-fs-menu role="presentation">
  <div class="menu" role="menu" aria-label={menu.title}>
    <h2>{menu.title}</h2>
    {#each menu.items as item, i}
      <button role="menuitem" class:focus={i === menu.index} onclick={() => press(i)}>{item.label}</button>
    {/each}
  </div>
</div>

<style>
  .scrim { position: absolute; inset: 0; z-index: 5; display: grid; place-items: center; background: var(--fs-scrim); }
  .menu { display: grid; gap: 1vh; min-width: 32vw; max-width: 60vw; padding: 3vh 2.4vw; border-radius: calc(var(--fs-radius) * 1.6); background: var(--fs-panel); border: 1px solid var(--fs-line); box-shadow: 0 3vh 6vh var(--fs-shadow); }
  h2 { margin: 0 0 1vh; font-size: calc(2.8vh * var(--fs-scale)); }
  button { padding: 1.3vh 1.4vw; border: 0.3vh solid transparent; border-radius: var(--fs-radius); background: none; color: var(--fs-text); font-size: calc(2.4vh * var(--fs-scale)); text-align: left; cursor: pointer; }
  button.focus { border-color: var(--fs-focus); background: color-mix(in srgb, var(--fs-focus) 15%, transparent); }
</style>
