<script>
  // The fullscreen mode's line for what happened: Play starting nothing,
  // an emulator closing straight away, how long a game was played. The
  // desktop shows these in its status bar, which the fullscreen mode
  // covers. Each shows for a while, an error longer.
  let { note = "", error = "" } = $props();
  let shown = $state(null);
  $effect(() => {
    const text = error || note;
    if (!text) return;
    shown = { text, bad: !!error };
    const t = setTimeout(() => (shown = null), error ? 8000 : 4000);
    return () => clearTimeout(t);
  });
</script>

{#if shown}
  <div class="toast" class:bad={shown.bad} role={shown.bad ? "alert" : "status"}>{shown.text}</div>
{/if}

<style>
  .toast { position: fixed; left: 50%; bottom: 12vh; transform: translateX(-50%); max-width: 70vw; padding: 1.2vh 2vw; border: 1px solid var(--fs-line); border-radius: var(--fs-radius); background: var(--fs-panel); color: var(--fs-text); font-size: calc(2vh * var(--fs-scale)); z-index: 30; text-align: center; }
  .bad { border-color: var(--fs-accent); }
</style>
