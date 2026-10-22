<script>
  import { invoke } from "@tauri-apps/api/core";
  import { run } from "./jobs.svelte.js";
  import { ask } from "@tauri-apps/plugin-dialog";

  // RetroArch's cores (milestone 18, 1.4): the ones romforge's platforms
  // use, where each is installed, its licence, and what the libretro
  // buildbot has. Downloads go into the RomForge folder's Emulators/cores.
  let { note = $bindable(""), error = $bindable("") } = $props();
  let list = $state(null);
  let working = $state("");
  let filter = $state("missing");

  async function load(refresh = false) {
    working = refresh ? "list" : working;
    try {
      list = await invoke("emulator_cores", { refresh });
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
    working = "";
  }
  load();

  async function get(c) {
    // romforge refuses a non-commercial core unless it was accepted here.
    if (c.non_commercial) {
      const ok = await ask(`${c.display_name}: its licence (${c.license}) forbids commercial use. Download it for your own use?`, { title: "Non-commercial core", kind: "warning" });
      if (!ok) return;
    }
    working = c.name;
    // A job in the queue (footer); its errors go to the status line.
    const date = await run(`Downloading ${c.display_name}…`, "download_core", { core: c.name, accept: c.non_commercial }, { key: `core:${c.name}` });
    if (date) note = `${c.display_name} (${date}) downloaded.`;
    working = "";
    await load();
  }

  async function remove(c) {
    const ok = await ask(`Remove ${c.display_name} from romforge's Emulators/cores?`, { title: "Remove core", kind: "warning" });
    if (!ok) return;
    working = c.name;
    try {
      await invoke("remove_core", { core: c.name });
      note = `${c.display_name} removed.`;
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
    working = "";
    await load();
  }

  const shown = $derived(
    (list?.cores ?? []).filter((c) => filter === "all" || (filter === "missing" ? !c.installed || c.update : !!c.ours)),
  );
</script>

<section class="setting">
  <span class="flabel">Cores</span>
  <p class="hint">
    The cores your platforms use, from the libretro buildbot (what RetroArch's own updater uses). romforge's downloads go
    in its Emulators/cores folder and are tried first.
  </p>
  <div class="pick">
    <button onclick={() => load(true)} disabled={!!working}>{working === "list" ? "Checking…" : list?.checked ? "Check for updates" : "Get the buildbot's list"}</button>
    <select bind:value={filter} aria-label="Which cores">
      <option value="missing">Missing or out of date</option>
      <option value="ours">Downloaded by romforge</option>
      <option value="all">All</option>
    </select>
  </div>
  {#if list && !list.arch}<p class="hint">The buildbot has no cores for this processor.</p>{/if}
  <table class="cores">
    <tbody>
      {#each shown as c}
        <tr>
          <td>
            <strong>{c.display_name}</strong>
            <span class="hint">· {c.systems.join(", ")}</span><br />
            <span class="hint" class:nc={c.non_commercial}>{c.license || "licence unknown"}{c.non_commercial ? " (non-commercial)" : ""}{c.experimental ? " · experimental" : ""}</span>
          </td>
          <td class="state hint">
            {#if c.ours}romforge's {c.ours}{c.update ? `, update ${c.available}` : ""}{:else if c.installed}installed{:else}not installed{/if}
          </td>
          <td class="act">
            {#if c.ours}
              {#if c.update}<button onclick={() => get(c)} disabled={!!working}>Update</button>{/if}
              <button onclick={() => remove(c)} disabled={!!working}>Remove</button>
            {:else if !c.installed && c.available}
              <button onclick={() => get(c)} disabled={!!working}>{working === c.name ? "Downloading…" : "Download"}</button>
            {/if}
          </td>
        </tr>
      {:else}
        <tr><td class="hint">{list?.checked ? "Nothing here." : "Get the buildbot's list to see what can be downloaded."}</td></tr>
      {/each}
    </tbody>
  </table>
</section>

<style>
  .cores { width: 100%; margin-top: 0.4rem; }
  .cores td { padding: 0.25rem 0.3rem; border-top: 1px solid var(--line); vertical-align: top; }
  .state { white-space: nowrap; }
  .act { text-align: right; white-space: nowrap; }
  .nc { color: var(--bad); }
</style>
