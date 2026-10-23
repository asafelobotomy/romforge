<script>
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import { DETAILS, SHOWN, toggled } from "./fullscreen/fsdetails.js";
  import { SHELF_DIRECTIONS, SHELF_GROUPS, SHELF_SORTS } from "./fullscreen/fsshelf.js";

  // Settings → Display: the fullscreen mode's attract mode, screensaver
  // and background music (stage 2, step 6), part of the dialog's draft
  // (saved with Save).
  let { draft = $bindable() } = $props();
  // The core's choices (settings::ATTRACT_MINUTES, SCREENSAVER_MINUTES; it
  // takes no others): settings_info doesn't send them yet.
  const ATTRACT = [0, 3, 5, 10];
  const SAVER = [0, 10, 30];

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
      {#each ATTRACT as n}<option value={n}>{n ? `${n} minutes` : "Never"}</option>{/each}
    </select>
    <label for="fs-saver">Screensaver after that</label>
    <select id="fs-saver" bind:value={draft.screensaver_minutes} disabled={!draft.attract_minutes}>
      {#each SAVER as n}<option value={n}>{n ? `${n} minutes` : "Never"}</option>{/each}
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

<section class="setting">
  <span class="flabel">Details of the game in focus</span>
  <div class="details">
    {#each DETAILS as d}
      <label class="toggle"><input type="checkbox" checked={(draft.fs_details ?? SHOWN).includes(d.key)} onchange={() => (draft.fs_details = toggled(draft.fs_details ?? SHOWN, d.key))} /> {d.label}</label>
    {/each}
  </div>
  <p class="hint">What the fullscreen mode shows beside the games (under them in cover flow), in every layout. Start → Details there changes it too.</p>
</section>

<section class="setting">
  <span class="flabel">The bookshelf layout</span>
  {#if draft.fs_bookshelf}
    <div class="grid">
      <label for="fs-shelves">Shelves</label>
      <select id="fs-shelves" bind:value={draft.fs_bookshelf.direction}>
        {#each SHELF_DIRECTIONS as d}<option value={d.value}>{d.label}</option>{/each}
      </select>
      <label for="fs-shelf-group">All games</label>
      <select id="fs-shelf-group" bind:value={draft.fs_bookshelf.group}>
        {#each SHELF_GROUPS as g}<option value={g.value}>{g.label}</option>{/each}
      </select>
      <label for="fs-shelf-sort">Order</label>
      <select id="fs-shelf-sort" bind:value={draft.fs_bookshelf.sort}>
        {#each SHELF_SORTS as s}<option value={s.value}>{s.label}</option>{/each}
      </select>
      <span></span>
      <label class="toggle"><input type="checkbox" bind:checked={draft.fs_bookshelf.favourites} /> A favourites shelf at the top</label>
      <span></span>
      <label class="toggle"><input type="checkbox" bind:checked={draft.fs_bookshelf.recent} /> A recently played shelf under it</label>
    </div>
  {/if}
  <p class="hint">The platform picked along the top stands its games on shelves, spines out, at their real sizes (All games: every platform's, on a shelf each or mixed); its favourites and recently played shelves hold its own. Start → Bookshelf in the fullscreen mode changes these too.</p>
</section>

<style>
  .details { display: grid; grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr)); gap: 0.25rem 0.8rem; margin: 0.3rem 0; }
  .grid { display: grid; grid-template-columns: auto minmax(10rem, 18rem); gap: 0.35rem 0.8rem; align-items: center; margin: 0.3rem 0; }
</style>
