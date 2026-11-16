<script>
  import { invoke } from "@tauri-apps/api/core";
  import { size } from "./lib.js";
  import { ui } from "./ui.svelte.js";

  // The Saves page (milestone 27, stage 2): the in-game saves in the
  // RomForge folder's Saves/, one tab per system, each with the game it
  // belongs to when the evidence names exactly one. The core walks the
  // folder and matches; this only shows. Always mounted, so the tab
  // picked lasts; drawn only while `active`.
  let { active, openGame } = $props();

  let tabs = $state([]);
  let key = $state(null);
  let problem = $state("");
  let loading = $state(false);

  export async function loadSaves() {
    loading = true;
    problem = "";
    try {
      tabs = await invoke("saves_page");
    } catch (e) {
      tabs = [];
      problem = String(e);
    }
    loading = false;
    // A game's Details asked for its system's tab.
    const wanted = ui.savesSystem;
    ui.savesSystem = null;
    const has = (k) => tabs.some((t) => t.system.toLowerCase() === k?.toLowerCase());
    if (has(wanted)) key = tabs.find((t) => t.system.toLowerCase() === wanted.toLowerCase()).system;
    else if (!has(key)) key = tabs[0]?.system ?? null;
  }

  let tab = $derived(tabs.find((t) => t.system === key));
  const how = { serial: "by serial", file_name: "by file name" };
  const when = (secs) => (secs ? new Date(secs * 1000).toLocaleString() : "");
</script>

{#if active}
  <section class="wide">
    <div class="bar">
      <button onclick={loadSaves} disabled={loading}>Refresh</button>
      <span class="hint">In-game saves (memory cards, battery saves, save folders) in the RomForge folder's Saves/. Read only.</span>
    </div>
    {#if problem}
      <p class="hint">{problem}</p>
    {:else if !tabs.length}
      <p class="hint">
        {loading ? "Reading Saves/…" : "No saves in Saves/ yet. Emulators romforge starts save there; an emulator's tab in Emulator Settings can move the saves it made before."}
      </p>
    {:else}
      <div class="tabs" role="tablist" aria-label="Systems with saves">
        {#each tabs as t}
          <button role="tab" aria-selected={t.system === key} class:active={t.system === key} onclick={() => (key = t.system)}>
            {t.name} <span class="count">{t.saves.length}</span>
          </button>
        {/each}
      </div>
      {#if tab}
        <p class="hint">{tab.saves.length} save{tab.saves.length === 1 ? "" : "s"}, {tab.matched} matched to a game. Saves/{tab.system}/</p>
        <table>
          <thead>
            <tr><th>Game</th><th>File</th><th class="num">Size</th><th>Modified</th></tr>
          </thead>
          <tbody>
            {#each tab.saves as s (s.file)}
              <tr>
                <td>
                  {#if s.row}
                    <button class="linkish" onclick={() => openGame(s.row)} title="Show it in the library ({how[s.matched_by]})">{s.game}</button>
                  {:else if s.game}
                    <span title={how[s.matched_by]}>{s.game}</span>
                  {:else}
                    <span class="hint" title={s.serial ? `Holds ${s.serial}, which names no single game here` : ""}>not matched</span>
                  {/if}
                </td>
                <td class="file" title={s.path}><bdi>{s.file}</bdi></td>
                <td class="num">{size(s.size)}</td>
                <td>{when(s.modified)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .tabs { display: flex; flex-wrap: wrap; gap: 0.3rem; margin: 0.2rem 0 0.4rem; }
  .tabs button { border-radius: 6px; padding: 0.25rem 0.6rem; }
  .tabs button.active { background: var(--accent); color: var(--accent-text, white); }
  .count { font-size: 0.75em; opacity: 0.75; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th, td { text-align: left; padding: 0.25rem 0.4rem; border-bottom: 1px solid var(--line); }
  .num { text-align: right; white-space: nowrap; }
  .file { overflow-wrap: anywhere; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; }
  .linkish:hover { text-decoration: underline; }
</style>
