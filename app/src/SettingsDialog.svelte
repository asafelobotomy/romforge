<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { iconCredits } from "./facts.js";
  import { edit } from "./jobs.svelte.js";
  import ThemeSettings from "./ThemeSettings.svelte";
  import FullscreenSettings from "./FullscreenSettings.svelte";
  import { knowFsInfo } from "./fullscreen/fsinfo.js";
  import SteamSettings from "./SteamSettings.svelte";

  // `call` is the app's invoke that reports errors on the status line;
  // `load` reloads the library (its Media/ paths change with a move).
  let { open = $bindable(false), note = $bindable(""), showEmpty = $bindable(false), regionFlags = $bindable(true), homeRoot = $bindable(null), call, load } = $props();
  let moving = $state(false);
  let regionsText = $state("");
  let languages = $state([]);
  let systemLanguage = $state("en");
  let scummvmNote = $state("");
  let readingList = $state(false);

  // ScummVM names its games itself: read its list from the installed
  // ScummVM, or from a file saved with --dump-all-detection-entries.
  async function readScummvm(fromFile) {
    let file = null;
    if (fromFile) {
      file = await pickFolder({ multiple: false, title: "ScummVM's detection list", filters: [{ name: "DAT", extensions: ["dat", "txt"] }] });
      if (typeof file !== "string") return;
    }
    scummvmNote = "";
    readingList = true;
    try {
      const r = await invoke("scummvm_list", { file });
      scummvmNote = `${r.entries} entries; ScummVM itself named ${r.asked} folder${r.asked === 1 ? "" : "s"}; ${r.games} ScummVM game${r.games === 1 ? "" : "s"} in your folders.`;
    } catch (e) {
      scummvmNote = String(e).replace(/^launch: /, "");
    }
    readingList = false;
  }

  // Pick a new parent folder; RomForge moves into it as a whole.
  async function moveHome() {
    const dir = await pickFolder({ directory: true, multiple: false, title: "Move RomForge into…" });
    if (typeof dir !== "string") return;
    settingsError = "";
    moving = true;
    try {
      homeRoot = await invoke("home_move", { parent: dir });
      note = `RomForge moved to ${homeRoot}`;
      await load();
    } catch (e) {
      settingsError = String(e).replace(/^settings: /, "");
    }
    moving = false;
  }

  // Settings panel: a working copy (get_settings, when it opens), saved
  // on "Save".
  let draft = $state(null);
  // Rename & Cleanup's behaviour: the two settings it is made of.
  const modeOf = (d) => (d.in_place ? "replace" : d.keep_originals ? "keep" : "trash");
  function setMode(m) {
    draft.in_place = m === "replace";
    draft.keep_originals = m === "keep";
  }
  let defaultStaging = $state("");
  let settingsError = $state("");
  // Settings in tabs, by what they are about.
  const TABS = [
    ["folders", "Folders"],
    ["cleanup", "Rename & Cleanup"],
    ["display", "Display"],
    ["sources", "Sources"],
    ["steam", "Steam"],
    ["credits", "Credits"],
  ];
  let stab = $state("folders");

  export async function openSettings() {
    settingsError = "";
    const s = await call("get_settings");
    if (!s) return;
    draft = s;
    regionsText = (s.regions ?? []).join(", ");
    const info = await call("settings_info");
    defaultStaging = info?.default_staging ?? "";
    languages = info?.languages ?? [];
    systemLanguage = info?.system_language ?? "en";
    knowFsInfo(info);
    open = true;
  }

  async function browse(field) {
    const dir = await pickFolder({ directory: true, multiple: false, defaultPath: draft[field] ?? undefined });
    if (typeof dir === "string") draft[field] = dir;
  }

  async function saveSettings() {
    settingsError = "";
    try {
      draft.regions = regionsText.split(",").map((r) => r.trim()).filter(Boolean);
      // Behind a scan or the like it waits its turn: the dialog closes.
      const onWait = (why) => {
        open = false;
        note = `Saving settings ${why}`;
      };
      draft = await edit("Saving settings…", "save_settings", { settings: draft }, { onWait });
      showEmpty = draft.show_empty_media;
      regionFlags = draft.region_flags !== false;
      open = false;
      note = "Settings saved.";
      await load();
    } catch (e) {
      settingsError = String(e).replace(/^settings: /, "");
    }
  }
</script>

{#if open && draft}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog tall" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <h2 id="settings-title">Settings</h2>

      <div class="stabs" role="tablist">
        {#each TABS as [k, label]}
          <button role="tab" aria-selected={stab === k} class:active={stab === k} onclick={() => (stab = k)}>{label}</button>
        {/each}
      </div>

      <div class="body">
      {#if stab === "folders"}
      {#if homeRoot}
        <section class="setting">
          <span class="flabel">RomForge folder</span>
          <p class="hint">Everything lives here: Games, Media (art), Databases and the library. Scan folder scans any folder; Rename &amp; Cleanup imports its games.</p>
          <div class="pick">
            <input value={homeRoot} readonly aria-label="RomForge folder" />
            <button onclick={moveHome} disabled={moving} title="Move the whole folder somewhere else; everything in it comes along">{moving ? "Moving…" : "Move…"}</button>
          </div>
        </section>
      {/if}
      <section class="setting">
        <label for="source">Source</label>
        <p class="hint">Where your unprocessed ROMs are. Refresh scans it; the first folder you scan with Scan folder becomes it if none is set.</p>
        <div class="pick">
          <input id="source" bind:value={draft.source_dir} placeholder="Not set" />
          <button onclick={() => browse("source_dir")}>Browse…</button>
          {#if draft.source_dir}<button onclick={() => (draft.source_dir = null)}>Clear</button>{/if}
        </div>
      </section>
      {#if !homeRoot}
      <section class="setting">
        <label for="library">Library <span class="hint">(optional)</span></label>
        <p class="hint">Where Rename &amp; Cleanup puts processed games, sorted by platform. Refresh scans it too, and Play uses what's there.</p>
        <div class="pick">
          <input id="library" bind:value={draft.library_dir} placeholder="Not set" />
          <button onclick={() => browse("library_dir")}>Browse…</button>
          {#if draft.library_dir}<button onclick={() => { draft.library_dir = null; if (!homeRoot) draft.keep_originals = false; }}>Clear</button>{/if}
        </div>
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
      {/if}
      {:else if stab === "cleanup"}
      <section class="setting">
        <span class="flabel">Rename &amp; Cleanup</span>
        {#each [
          ["trash", "Move, then trash originals", "Each game is renamed, repacked and moved into " + (homeRoot ? "RomForge" : "the Library") + "; the original goes to the trash once the new file is verified (Undo puts it back)."],
          ["keep", "Move and keep originals", homeRoot ? "Each game is copied into RomForge; your files stay where they are." : "Each game is copied into the Library; Source files stay untouched."],
          ["replace", "Replace originals", "Each game is renamed and repacked where it is, not moved; the original goes to the trash once the new file is verified (Undo puts it back)."],
        ] as [m, label, hint]}
          {@const needsLibrary = m === "keep" && !homeRoot && !draft.library_dir}
          <label class="toggle" title={needsLibrary ? "Needs a Library folder for the copies" : hint}>
            <input type="radio" name="cleanup-mode" checked={modeOf(draft) === m} disabled={needsLibrary} onchange={() => setMode(m)} />
            {label}
          </label>
        {/each}
        <p class="hint">
          {#if modeOf(draft) === "replace"}
            Renamed and repacked where each game is, not moved; originals to the trash once the new file is verified (Undo puts
            them back). Convert follows this too.
          {:else if modeOf(draft) === "keep"}
            {homeRoot ? "Each game is copied into RomForge; your files stay where they are." : "Each game is copied into the Library; Source files stay untouched."}
            Convert keeps the originals too.
          {:else}
            Each game moves into {homeRoot ? "RomForge" : "the Library"}; originals go to the trash once the new file is verified
            (Undo puts them back). Convert trashes them too.
          {/if}
        </p>
      </section>
      {:else if stab === "display"}
      <ThemeSettings />
      <FullscreenSettings bind:draft />

      <section class="setting">
        <label for="icons">Platform icons</label>
        <p class="hint">The picture beside each platform in the sidebar, fetched once: controllers and systems from RetroArch's own icon sets, logos from Wikidata (Wikimedia Commons).</p>
        {#each [["", "Controller icons"], ["systematic", "System icons"], ["logo", "Platform logos"]] as [v, label]}
          <label class="toggle">
            <input type="radio" name="platform-icons" checked={(draft.platform_icons || "") === v || (v === "" && draft.platform_icons === "flatui")} onchange={() => (draft.platform_icons = v)} />
            {label}
          </label>
        {/each}
      </section>

      <section class="setting">
        <label for="regions">Preferred regions</label>
        <p class="hint">Every version of a game is kept and listed under one row; that row shows the version from the first region here that you have.</p>
        <input id="regions" bind:value={regionsText} placeholder="USA, World, Europe, Japan" />
        <p class="hint">Enrich also takes pictures from these regions first.</p>
      </section>

      <section class="setting">
        <label for="language">Descriptions in</label>
        <p class="hint">Enrich reads a game's Wikipedia article (and ScreenScraper's synopsis) in this language, else in English.</p>
        <select id="language" bind:value={draft.language}>
          <option value="">The system's ({languages.find(([c]) => c === systemLanguage)?.[1] ?? systemLanguage})</option>
          {#each languages as [code, name]}<option value={code}>{name}</option>{/each}
        </select>
      </section>

      <section class="setting">
        <span class="flabel">Region and language</span>
        <label class="toggle">
          <input type="radio" name="region-display" checked={draft.region_flags !== false} onchange={() => (draft.region_flags = true)} />
          Flag emojis
        </label>
        <label class="toggle">
          <input type="radio" name="region-display" checked={draft.region_flags === false} onchange={() => (draft.region_flags = false)} />
          Text: (USA)/(En)
        </label>
        <p class="hint">On a game's other versions in the list (not the main title, which stays clean).</p>
      </section>

      <section class="setting">
        <label class="toggle">
          <input type="checkbox" bind:checked={draft.show_missing_versions} />
          Display missing versions
        </label>
        <p class="hint">Under each game, list every region and edition in your DATs that romforge can link to it, even ones you don't have (shown italic). Off: only what you own, plus a missing original when you only have a translation, hack or patch.</p>
      </section>

      <section class="setting">
        <label class="toggle">
          <input type="checkbox" bind:checked={draft.start_fullscreen} />
          Start in the fullscreen mode
        </label>
        <p class="hint">For a game pad and a TV: platforms along the top, games below. F11 or the toolbar's Fullscreen switches anytime; its Start menu has Back to desktop.</p>
      </section>

      <section class="setting">
        <label class="flabel" for="while-playing">While a game plays</label>
        <select id="while-playing" bind:value={draft.while_playing}>
          <option value="tray">Hide romforge to the tray</option>
          <option value="minimise">Minimise romforge</option>
          <option value="stay">Keep romforge open</option>
        </select>
        <p class="hint">Once the game is up. romforge comes back by itself when the game ends; from the tray, Show romforge brings it back sooner and Stop asks the game to quit. Without a tray (some desktops have none), romforge minimises instead.</p>
      </section>

      <section class="setting">
        <label class="flabel" for="list-age">Online lists on Refresh</label>
        <select id="list-age" bind:value={draft.list_age_days}>
          {#each [[7, "Every 7 days"], [14, "Every 14 days"], [30, "Every 30 days"], [0, "Never"]] as [d, label]}<option value={d}>{label}</option>{/each}
          <!-- A number set with the CLI stays as it is. -->
          {#if ![7, 14, 30, 0].includes(draft.list_age_days)}<option value={draft.list_age_days}>Every {draft.list_age_days} days</option>{/if}
        </select>
        <p class="hint">Refresh first reads ScummVM's and MAME's lists again when the installed program changed, and downloads the DATs and lists of the systems you have files for once they are this old. Never: Refresh doesn't go online for them (Download DATs still does).</p>
      </section>

      <section class="setting">
        <label class="flabel" for="threads">Threads for long jobs</label>
        <select id="threads" bind:value={draft.threads}>
          <option value={0}>Automatic (one per CPU, at most 8)</option>
          <option value={1}>1 (a slow spinning disk)</option>
          {#each [2, 4, 8] as n}<option value={n}>{n}</option>{/each}
        </select>
        <p class="hint">How many files Scan and Refresh hash, and how many games Rename & Cleanup and Convert write, at once; downloads use up to four. Each website still gets no more than its own pace.</p>
      </section>

      <section class="setting">
        <label class="toggle">
          <input type="checkbox" bind:checked={draft.show_empty_media} />
          Show empty media boxes
        </label>
        <p class="hint">In Details, keep a box for every kind of art (box, fanart, logo, 3D box, title screen, in-game, icon) even when there's none yet. Off: only the art there is.</p>
      </section>

      <section class="setting">
        <label class="toggle">
          <input type="checkbox" bind:checked={draft.video_snaps} />
          Download video snaps
        </label>
        <p class="hint">Enrich also fetches a short gameplay clip per game (2–30 MB each; the Arcade Database for arcade games, ScreenScraper with its keys for the rest). It plays in Details and behind a game in the fullscreen mode.</p>
      </section>
      {:else if stab === "sources"}
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

      <section class="setting">
        <span class="flabel">ScummVM games</span>
        <p class="hint">ScummVM games are named by ScummVM's own game list, read from the ScummVM installed here (kept on this computer, never shared); ScummVM itself is asked about folders the list can't name (LucasArts, Sierra). Read it again after updating ScummVM or adding games.</p>
        <div class="pick">
          <button onclick={() => readScummvm(false)} disabled={readingList}>{readingList ? "Reading…" : "Read ScummVM's list"}</button>
          <button onclick={() => readScummvm(true)} disabled={readingList} title="A list saved with: scummvm --dump-all-detection-entries">From a file…</button>
        </div>
        {#if scummvmNote}<p class="hint" role="status">{scummvmNote}</p>{/if}
      </section>
      {:else if stab === "steam"}
      <SteamSettings />
      {:else}
      <section class="setting credits">
        {#snippet link(url, text)}<button class="linkish" onclick={() => openUrl(url)}>{text}</button>{/snippet}
        <p class="hint">
          Genre icons, and the sidebar's All games and Unidentified icons, from
          {@render link("https://game-icons.net", "game-icons.net")}, licensed
          {@render link("https://creativecommons.org/licenses/by/3.0/", "CC BY 3.0")} (background removed, recoloured):
        </p>
        <ul class="hint">
          {#each iconCredits() as c}<li>{c.author}: {c.names.join(", ")}</li>{/each}
        </ul>
        <p class="hint">
          Controller and system icons from {@render link("https://github.com/libretro/retroarch-assets", "RetroArch's assets")}
          by the libretro team and contributors, licensed {@render link("https://creativecommons.org/licenses/by/4.0/", "CC BY 4.0")},
          downloaded to this computer as they are; romforge draws its own, in their flatui style, for platforms flatui
          lacks (some generated with OpenAI's GPT Image; the ScummVM one carries the ScummVM Team's logo, GPL).
          Platform logos come from Wikimedia Commons through Wikidata (each logo's Commons page gives its terms; many
          are trademarks).
        </p>
        <p class="hint">
          Studio logos and people's photos come from {@render link("https://commons.wikimedia.org", "Wikimedia Commons")}
          through Wikidata; each picture's own page (linked under it in Development) gives its author and licence.
          Game facts from libretro, Wikidata and the sources set above. romforge's pixel wordmark and app icon are its own pixel
          art. romforge is MIT licensed.
        </p>
        <p class="hint">
          Manuals are shown with {@render link("https://github.com/mozilla/pdf.js", "pdf.js")} and their scanned pages made searchable with
          {@render link("https://github.com/naptha/tesseract.js", "tesseract.js")} and Tesseract's language data, all licensed
          {@render link("https://www.apache.org/licenses/LICENSE-2.0", "Apache-2.0")}.
        </p>
        <p class="hint">
          Arcade games' pictures, videos, facts and manuals: scraping using {@render link("https://adb.arcadeitalia.net", "Arcade Database")}
          by motoschifo; their history text © arcade-history.com. GameCube, Wii and DS covers, full covers and disc and cartridge
          labels from {@render link("https://www.gametdb.com", "GameTDB")}, where anyone can add missing ones.
        </p>
      </section>
      {/if}
      </div>

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
  .stabs { display: flex; gap: 0.25rem; flex-wrap: wrap; border-bottom: 1px solid var(--line); margin: 0.25rem 0 0.25rem; }
  .stabs button { border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 0.35rem 0.7rem; color: var(--muted); background: none; }
  .stabs button.active { color: var(--text); border-bottom-color: var(--accent); }
  .stabs + :global(.setting) { border-top: none; }
  .credits ul { margin: 0.2rem 0 0.5rem; padding-left: 1.2rem; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
</style>
