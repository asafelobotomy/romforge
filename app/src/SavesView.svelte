<script>
  import { invoke } from "@tauri-apps/api/core";
  import SaveCard from "./SaveCard.svelte";
  import SaveInspector from "./SaveInspector.svelte";
  import StatesMove from "./StatesMove.svelte";
  import { ui } from "./ui.svelte.js";

  // The Saves page (milestone 27): one tab per system, one card per game
  // holding its in-game saves and its save states (Saves/<system>/ and its
  // states/ folder), the game linked when the evidence names exactly one.
  // Picking a card opens the inspector beside the cards. The core walks the
  // folders and matches; this only shows. Always mounted, so the tab and
  // card picked last; drawn only while `active`.
  let { active, openGame, note = $bindable(""), error = $bindable("") } = $props();

  let tabs = $state([]);
  let key = $state(null);
  let picked = $state(null);
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
    // A game's Details asked for its system's tab (and its card).
    const wanted = ui.savesSystem;
    ui.savesSystem = null;
    const find = (k) => tabs.find((t) => t.system.toLowerCase() === k?.toLowerCase());
    if (wanted && find(wanted.system)) {
      key = find(wanted.system).system;
      picked = wanted.game ?? picked;
    } else if (!find(key)) key = tabs[0]?.system ?? null;
  }

  let tab = $derived(tabs.find((t) => t.system === key));
  let card = $derived(tab?.cards.find((c) => c.name === picked) ?? tab?.cards[0] ?? null);
  let oldStates = $derived(tabs.reduce((n, t) => n + t.old_states, 0));
  const n = (k, one, many) => (k === 1 ? `1 ${one}` : `${k} ${many}`);
</script>

{#if active}
  <section class="wide saves">
    <div class="bar">
      <button onclick={loadSaves} disabled={loading}>Refresh</button>
      <span class="hint">In-game saves and save states in the RomForge folder's Saves/. Read only.</span>
    </div>
    <StatesMove count={oldStates} done={loadSaves} bind:note bind:error />
    {#if problem}
      <p class="hint">{problem}</p>
    {:else if !tabs.length}
      <p class="hint">
        {loading ? "Reading Saves/…" : "No saves or save states yet. Emulators romforge starts save there; an emulator's tab in Emulator Settings can move the saves it made before."}
      </p>
    {:else}
      <div class="tabs" role="tablist" aria-label="Systems with saves">
        {#each tabs as t}
          <button role="tab" aria-selected={t.system === key} class:active={t.system === key} onclick={() => (key = t.system)}>
            {t.name} <span class="count">{t.cards.length}</span>
          </button>
        {/each}
      </div>
      {#if tab}
        <div class="split">
          <div class="list">
            <p class="hint">
              {n(tab.cards.length, "game", "games")}: {n(tab.saves, "save", "saves")}, {n(tab.states, "save state", "save states")}. Newest first.
            </p>
            <div class="cards">
              {#each tab.cards as c (c.name)}
                <SaveCard card={c} picked={c === card} pick={() => (picked = c.name)} />
              {/each}
            </div>
          </div>
          {#if card}<SaveInspector {card} system={tab.system} {openGame} />{/if}
        </div>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .saves { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
  .tabs { display: flex; flex-wrap: wrap; gap: 0.3rem; margin: 0.2rem 0 0.4rem; }
  .tabs button { border-radius: 6px; padding: 0.25rem 0.6rem; }
  .tabs button.active { background: var(--accent); color: var(--accent-text, white); }
  .count { font-size: 0.75em; opacity: 0.75; }
  .split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(16rem, 1fr); gap: 0.4rem; }
  .list { overflow-y: auto; min-height: 0; padding-right: 0.3rem; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr)); gap: 0.6rem; }
  @media (max-width: 900px) {
    .split { grid-template-columns: 1fr; }
  }
</style>
