<script>
  import { invoke } from "@tauri-apps/api/core";

  // The Advanced side of Scan folder and Refresh: what the CLI's `scan` and
  // `refresh` take as flags. `ask("scan" | "refresh", folder)` opens it and
  // resolves with `{ action, options }` (action: "scan", "preview" or
  // "refresh"), or null when it is closed without going on.
  let { open = $bindable(false) } = $props();

  let kind = $state("scan");
  let folder = $state("");
  let finish = null;
  let systems = $state([]);
  let dats = $state([]);

  let scope = $state("");
  let jobs = $state(0);
  let rehash = $state(false);
  let allFiles = $state(false);
  let noLists = $state(false);
  let everyFolder = $state(false);

  // Closed any other way (Escape, a click outside): the same as Cancel.
  $effect(() => {
    if (!open && finish) {
      finish(null);
      finish = null;
    }
  });

  export async function ask(what, dir = "") {
    kind = what;
    folder = dir;
    if (what === "scan") {
      if (!systems.length) systems = (await invoke("all_systems").catch(() => [])) ?? [];
      const summary = await invoke("summary").catch(() => null);
      dats = summary?.dats ?? [];
    }
    open = true;
    return new Promise((resolve) => (finish = resolve));
  }

  function done(action) {
    const [type, value] = scope.split(":");
    const options = {
      jobs: Number(jobs) || 0,
      rehash,
      ...(kind === "scan"
        ? { allFiles, system: type === "system" ? value : null, dat: type === "dat" ? Number(value) : null }
        : { noLists, everyFolder }),
    };
    open = false;
    finish?.(action ? { action, options } : null);
    finish = null;
  }
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && done(null)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="runopts-title">
      <h2 id="runopts-title">{kind === "scan" ? "Scan folder" : "Refresh"}: advanced</h2>
      {#if kind === "scan"}<p class="meta"><bdi>{folder}</bdi></p>{/if}
      <details open>
        <summary>Advanced</summary>
        {#if kind === "scan"}
          <section class="setting">
            <label class="flabel" for="ro-scope">Identify against</label>
            <select id="ro-scope" bind:value={scope}>
              <option value="">Every DAT (or what this folder was scanned with before)</option>
              <optgroup label="One platform">
                {#each systems as s}<option value="system:{s.key}">{s.name}</option>{/each}
              </optgroup>
              {#if dats.length}
                <optgroup label="One DAT">
                  {#each dats as d}<option value="dat:{d.dat_id}">{d.name}</option>{/each}
                </optgroup>
              {/if}
            </select>
            <p class="hint">A folder scanned for one platform or DAT stays that way when it is scanned again.</p>
          </section>
          <section class="setting">
            <label class="toggle"><input type="checkbox" bind:checked={allFiles} /> Read every file, not only those with a ROM extension</label>
            <p class="hint">Slower; finds ROMs with an odd extension.</p>
          </section>
        {:else}
          <section class="setting">
            <label class="toggle"><input type="checkbox" bind:checked={noLists} /> Don't go online for the lists</label>
            <p class="hint">Only rescan the folders: leave the DATs, ScummVM's and MAME's lists as they are.</p>
          </section>
          <section class="setting">
            <label class="toggle"><input type="checkbox" bind:checked={everyFolder} /> Every folder ever scanned</label>
            <p class="hint">Not only the Source and Library folders of Settings (and the RomForge folder).</p>
          </section>
        {/if}
        <section class="setting">
          <label class="toggle"><input type="checkbox" bind:checked={rehash} /> Read every file again, even ones unchanged since the last scan</label>
        </section>
        <section class="setting">
          <label class="flabel" for="ro-jobs">Threads</label>
          <select id="ro-jobs" bind:value={jobs}>
            <option value={0}>As in Settings</option>
            <option value={1}>1 (a slow spinning disk)</option>
            {#each [2, 4, 8, 16] as n}<option value={n}>{n}</option>{/each}
          </select>
        </section>
      </details>
      <div class="actions">
        <span class="spacer"></span>
        <button onclick={() => done(null)}>Cancel</button>
        {#if kind === "scan"}
          <button onclick={() => done("preview")} title="See what the folder holds first; nothing is kept until you say so">Dry run</button>
          <button class="primary" onclick={() => done("scan")}>Scan</button>
        {:else}
          <button class="primary" onclick={() => done("refresh")}>Refresh</button>
        {/if}
      </div>
    </div>
  </div>
{/if}
