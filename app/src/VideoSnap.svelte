<script>
  import { onDestroy } from "svelte";

  // A video snap (an MP4 Enrich saved), played from a Blob: WebKitGTK's
  // player can't stream from a custom scheme (as the music player found).
  // `delay`: wait this many ms before loading (the fullscreen page plays it
  // once the focus rests); `controls`: the user starts it (Details).
  let { src, delay = 0, controls = false, cls = "" } = $props();

  let url = $state(null);
  let failed = $state(false);
  let gone = false;

  async function load() {
    try {
      const r = await fetch(src);
      if (!r.ok) throw new Error(String(r.status));
      const blob = await r.blob();
      if (gone) return;
      url = URL.createObjectURL(blob);
    } catch {
      failed = true;
    }
  }

  $effect(() => {
    if (!src || controls) return;
    const t = setTimeout(load, delay);
    return () => clearTimeout(t);
  });

  onDestroy(() => {
    gone = true;
    if (url) URL.revokeObjectURL(url);
  });
</script>

{#if url}
  <!-- svelte-ignore a11y_media_has_caption -->
  <video class={cls} src={url} autoplay={!controls} muted={!controls} loop={!controls} {controls} playsinline></video>
{:else if controls && !failed}
  <button class="play {cls}" onclick={load} aria-label="Play the video snap">▶ Video snap</button>
{/if}

<style>
  video { display: block; max-width: 100%; }
  .play { padding: 1.2rem 1.6rem; font-size: 1rem; }
</style>
