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
  .scrim { position: absolute; inset: 0; z-index: 5; display: grid; place-items: center; background: #000a; }
  .menu { display: grid; gap: 1vh; min-width: 32vw; max-width: 60vw; padding: 3vh 2.4vw; border-radius: 1.6vh; background: var(--panel); border: 1px solid var(--line); box-shadow: 0 3vh 6vh #0008; }
  h2 { margin: 0 0 1vh; font-size: 2.8vh; }
  button { padding: 1.3vh 1.4vw; border: 0.3vh solid transparent; border-radius: 1vh; background: none; color: var(--text); font-size: 2.4vh; text-align: left; cursor: pointer; }
  button.focus { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 15%, transparent); }
</style>
