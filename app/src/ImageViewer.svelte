<script>
  import { artLabel, caption } from "./labels.js";
  import { ui } from "./ui.svelte.js";

  // The image open at full size (`ui.viewer`), over everything; a click
  // closes it (Escape too, in App).
  let { detail, row } = $props();

  let winW = $state(1200);
  let winH = $state(800);

  // Full size: its own pixels, smaller only to fit the window. Title screens
  // and screenshots of old systems are tiny, so they are scaled up by whole
  // steps with sharp pixels instead of a blur.
  function viewerScale(kind) {
    const d = ui.dims[kind];
    if (!d) return 1;
    const fit = Math.min((winW * 0.92) / d[0], (winH * 0.82) / d[1]);
    if (fit < 1) return fit;
    return ["title", "snap"].includes(kind) ? Math.max(1, Math.floor(fit)) : 1;
  }
</script>

<svelte:window bind:innerWidth={winW} bind:innerHeight={winH} />

{#if ui.viewer}
  {@const k = ui.viewer.kind}
  {@const scale = viewerScale(k)}
  {@const dims = ui.dims}
  <div class="scrim viewer" role="presentation" onclick={() => (ui.viewer = null)}>
    <figure>
      <img
        src={ui.viewer.src}
        alt="{artLabel[k]} of {row?.title}"
        class:pixels={scale > 1}
        style:width={dims[k] ? `${Math.round(dims[k][0] * scale)}px` : null}
        style:height={dims[k] ? `${Math.round(dims[k][1] * scale)}px` : null}
      />
      <figcaption>{caption(k, dims, detail)}{scale > 1 ? ` · shown ×${scale}` : scale < 1 ? ` · shown at ${Math.round(scale * 100)}%` : " · actual size"}</figcaption>
    </figure>
  </div>
{/if}

<style>
  figure { margin: 0; }
  figcaption { font-size: 0.72rem; color: var(--muted); padding: 0.2rem 0.1rem 0; line-height: 1.3; }
  .viewer { cursor: zoom-out; }
  .viewer figure { display: flex; flex-direction: column; align-items: center; }
  .viewer img { max-width: 92vw; max-height: 82vh; object-fit: contain; box-shadow: 0 8px 30px #0008; }
  .viewer img.pixels { image-rendering: pixelated; max-width: none; max-height: none; }
  .viewer figcaption { color: #ddd; font-size: 0.85rem; padding-top: 0.5rem; }
</style>
