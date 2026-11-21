<script>
  import { UI_ICONS } from "./uiicons.js";

  // One of romforge's own icons (uiicons.js), in the text's colour.
  let { name, size = 20 } = $props();
</script>

<svg class="uiicon" viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={UI_ICONS[name]} />
</svg>

<style>
  .uiicon { display: block; flex: none; }
</style>
