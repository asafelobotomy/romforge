<script>
  // The help bar along the bottom of the fullscreen mode, always on: what
  // each button does on this screen, as a pad's buttons once one was seen,
  // else as keys.
  import { KEY_LABELS } from "./fsnav.js";

  let { items, pad = false } = $props();

  const PAD = { accept: "Ⓐ", back: "Ⓑ", north: "Ⓨ", west: "Ⓧ", menu: "☰ Start", context: "⧉ Select", page: "LB / RB", move: "✛", side: "◀ ▶" };
</script>

<footer class="help" data-fs-help>
  {#each items as [button, label]}
    <span class="item"><kbd>{(pad ? PAD : KEY_LABELS)[button] ?? button}</kbd> {label}</span>
  {/each}
</footer>

<style>
  .help { display: flex; flex-wrap: wrap; justify-content: center; gap: 0.8vh 2.4vw; padding: 1.4vh 2vw; border-top: 1px solid var(--fs-line); background: color-mix(in srgb, var(--fs-panel) 80%, transparent); font-size: calc(1.9vh * var(--fs-scale)); color: var(--fs-muted); }
  .item { display: inline-flex; align-items: center; gap: 0.5vw; }
  kbd { padding: 0.2vh 0.6vw; border: 1px solid var(--fs-line); border-radius: calc(var(--fs-radius) * 0.6); background: var(--fs-bg); color: var(--fs-text); font: inherit; font-weight: 600; }
</style>
