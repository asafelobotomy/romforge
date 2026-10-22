<script>
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";

  // Settings → Display: the fullscreen mode's attract mode, screensaver
  // and background music (stage 2, step 6), part of the dialog's draft
  // (saved with Save).
  let { draft = $bindable() } = $props();

  async function browse() {
    const dir = await pickFolder({ directory: true, multiple: false, defaultPath: draft.music_dir ?? undefined, title: "Music for the fullscreen mode" });
    if (typeof dir === "string") draft.music_dir = dir;
  }
</script>

<section class="setting">
  <span class="flabel">Fullscreen mode when idle</span>
  <div class="grid">
    <label for="fs-attract">Attract mode after</label>
    <select id="fs-attract" bind:value={draft.attract_minutes}>
      {#each [0, 3, 5, 10] as n}<option value={n}>{n ? `${n} minutes` : "Never"}</option>{/each}
    </select>
    <label for="fs-saver">Screensaver after that</label>
    <select id="fs-saver" bind:value={draft.screensaver_minutes} disabled={!draft.attract_minutes}>
      {#each [0, 10, 30] as n}<option value={n}>{n ? `${n} minutes` : "Never"}</option>{/each}
    </select>
  </div>
  <p class="hint">Attract mode shows your games in turn, full screen; the screensaver then dims it. Any button wakes it, on the game it was showing.</p>
</section>

<section class="setting">
  <label for="fs-music">Music in the fullscreen mode</label>
  <div class="bar">
    <input id="fs-music" value={draft.music_dir ?? ""} placeholder="None" readonly />
    <button onclick={browse}>Browse…</button>
    {#if draft.music_dir}<button onclick={() => (draft.music_dir = null)}>Clear</button>{/if}
  </div>
  <div class="grid">
    <label for="fs-volume">Volume</label>
    <input id="fs-volume" type="range" min="0" max="100" step="5" bind:value={draft.music_volume} />
    <span></span>
    <label class="toggle"><input type="checkbox" bind:checked={draft.music_shuffle} /> Shuffle</label>
  </div>
  <p class="hint">A folder of your own music (MP3, Ogg, Opus, FLAC, M4A, WAV, and the folders in it), played while the fullscreen mode is open, paused while a game runs. Start → Music skips and pauses.</p>
</section>

<style>
  .grid { display: grid; grid-template-columns: auto minmax(10rem, 18rem); gap: 0.35rem 0.8rem; align-items: center; margin: 0.3rem 0; }
</style>
