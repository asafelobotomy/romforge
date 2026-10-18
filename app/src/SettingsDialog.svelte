<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";

  // `call` is the app's invoke that reports errors on the status line.
  let { open = $bindable(false), note = $bindable(""), showEmpty = $bindable(false), call } = $props();

  // Settings panel: a working copy, saved on "Save".
  let draft = $state({ source_dir: null, library_dir: null, keep_originals: false, staging_dir: null, keys: {}, show_empty_media: false });
  let defaultStaging = $state("");
  let settingsError = $state("");

  export async function openSettings() {
    settingsError = "";
    const s = await call("get_settings");
    if (!s) return;
    draft = s;
    defaultStaging = (await call("settings_info"))?.default_staging ?? "";
    open = true;
  }

  async function browse(field) {
    const dir = await pickFolder({ directory: true, multiple: false, defaultPath: draft[field] ?? undefined });
    if (typeof dir === "string") draft[field] = dir;
  }

  async function saveSettings() {
    settingsError = "";
    try {
      draft = await invoke("save_settings", { settings: draft });
      showEmpty = draft.show_empty_media;
      open = false;
      note = "Settings saved. Refresh scans the Source and Library folders.";
    } catch (e) {
      settingsError = String(e).replace(/^settings: /, "");
    }
  }
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <h2 id="settings-title">Settings</h2>

      <section class="setting">
        <label for="source">Source</label>
        <p class="hint">Where your unprocessed ROMs are. Refresh scans it.</p>
        <div class="pick">
          <input id="source" bind:value={draft.source_dir} placeholder="Not set" />
          <button onclick={() => browse("source_dir")}>Browse…</button>
          {#if draft.source_dir}<button onclick={() => (draft.source_dir = null)}>Clear</button>{/if}
        </div>
      </section>

      <section class="setting">
        <label for="library">Library <span class="hint">(optional)</span></label>
        <p class="hint">Where Cleanup and rename puts processed games, sorted by platform. Refresh scans it too, and Play uses what's there.</p>
        <div class="pick">
          <input id="library" bind:value={draft.library_dir} placeholder="Not set" />
          <button onclick={() => browse("library_dir")}>Browse…</button>
          {#if draft.library_dir}<button onclick={() => { draft.library_dir = null; draft.keep_originals = false; }}>Clear</button>{/if}
        </div>
      </section>

      <section class="setting">
        <label class="toggle">
          <input type="checkbox" bind:checked={draft.keep_originals} disabled={!draft.library_dir} />
          Keep originals
        </label>
        <p class="hint">
          {#if draft.library_dir}
            {draft.keep_originals
              ? "Source files stay untouched. Cleanup writes each repacked game into the Library."
              : "Cleanup writes each repacked game into the Library; the originals go to .romforge-trash next to them once the new file is verified."}
          {:else}
            Needs a Library folder for the processed copies.
          {/if}
        </p>
      </section>

      <section class="setting">
        <label for="staging">Staging folder</label>
        <p class="hint">Scratch space for cleanup (tracks inside 7z archives are unpacked here). Must be outside the Source and Library.</p>
        <div class="pick">
          <input id="staging" bind:value={draft.staging_dir} placeholder={defaultStaging} />
          <button onclick={() => browse("staging_dir")}>Browse…</button>
          {#if draft.staging_dir}<button onclick={() => (draft.staging_dir = null)}>Use default</button>{/if}
        </div>
      </section>

      <section class="setting">
        <label class="toggle">
          <input type="checkbox" bind:checked={draft.show_empty_media} />
          Show empty media boxes
        </label>
        <p class="hint">In Details, keep a box for every kind of art (box, fanart, logo, 3D box, title screen, in-game, icon) even when there's none yet. Off: only the art there is.</p>
      </section>

      <section class="setting">
        <h3 class="sub">Online sources <span class="hint">(optional)</span></h3>
        <p class="hint">Free sources that need no account are always used first. These fill only what they leave empty.</p>
        <label for="sgdb">SteamGridDB API key</label>
        <input id="sgdb" type="password" autocomplete="off" bind:value={draft.keys.steamgriddb_key} placeholder="Not set" />
        <div class="two">
          <div><label for="igdb-id">IGDB client ID</label><input id="igdb-id" autocomplete="off" bind:value={draft.keys.igdb_client_id} placeholder="Twitch app" /></div>
          <div><label for="igdb-secret">IGDB client secret</label><input id="igdb-secret" type="password" autocomplete="off" bind:value={draft.keys.igdb_client_secret} /></div>
        </div>
        <div class="two">
          <div><label for="ss-dev">ScreenScraper developer ID</label><input id="ss-dev" autocomplete="off" bind:value={draft.keys.screenscraper_dev_id} /></div>
          <div><label for="ss-devpw">Developer password</label><input id="ss-devpw" type="password" autocomplete="off" bind:value={draft.keys.screenscraper_dev_password} /></div>
        </div>
        <div class="two">
          <div><label for="ss-user">ScreenScraper user</label><input id="ss-user" autocomplete="off" bind:value={draft.keys.screenscraper_user} placeholder="Optional" /></div>
          <div><label for="ss-pw">Password</label><input id="ss-pw" type="password" autocomplete="off" bind:value={draft.keys.screenscraper_password} /></div>
        </div>
        <p class="hint">Stored in romforge's database on this computer.</p>
      </section>

      {#if settingsError}<p class="err" role="alert">{settingsError}</p>{/if}
      <div class="actions">
        <button onclick={() => (open = false)}>Cancel</button>
        <button class="primary" onclick={saveSettings}>Save</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem; }
  .two input { width: 100%; box-sizing: border-box; }
</style>
