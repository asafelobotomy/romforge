<script>
  import { onDestroy } from "svelte";

  // A video snap (an MP4 Enrich saved), played from a Blob: WebKitGTK's
  // player can't stream from a custom scheme (as the music player found).
  // `delay`: wait this many ms before loading (the fullscreen page plays it
  // once the focus rests); `controls`: the user starts it (Details).
  let { src, delay = 0, controls = false, cls = "" } = $props();

  let url = $state(null);
  let failed = $state(false);
  let loading = $state(false);
  // The load under way, aborted when the snap changes or goes (leaving
  // mid-download doesn't fetch the rest).
  let ctl = null;

  function show(next) {
    if (url) URL.revokeObjectURL(url);
    url = next;
  }

  function stop() {
    ctl?.abort();
    ctl = null;
    loading = false;
  }

  async function load() {
    if (loading || url) return;
    const mine = (ctl = new AbortController());
    loading = true;
    try {
      const r = await fetch(src, { signal: mine.signal });
      if (!r.ok) throw new Error(String(r.status));
      const blob = await r.blob();
      if (mine.signal.aborted) return;
      show(URL.createObjectURL(blob));
    } catch {
      if (!mine.signal.aborted) failed = true;
    } finally {
      if (ctl === mine) ((ctl = null), (loading = false));
    }
  }

  // A new snap: the last one's load stopped and its video let go.
  $effect(() => {
    void src;
    return () => (stop(), show(null), (failed = false));
  });

  $effect(() => {
    if (!src || controls) return;
    const t = setTimeout(load, delay);
    return () => clearTimeout(t);
  });

  onDestroy(() => (stop(), show(null)));
</script>

{#if url}
  <!-- svelte-ignore a11y_media_has_caption -->
  <video class={cls} src={url} autoplay={!controls} muted={!controls} loop={!controls} {controls} playsinline></video>
{:else if controls && !failed}
  <button class="play {cls}" onclick={load} disabled={loading} aria-busy={loading} aria-label="Play the video snap">{loading ? "Loading…" : "▶ Video snap"}</button>
{/if}

<style>
  video { display: block; max-width: 100%; }
  .play { padding: 1.2rem 1.6rem; font-size: 1rem; }
</style>
