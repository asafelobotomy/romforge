<script>
  import { invoke } from "@tauri-apps/api/core";
  import { artSrc } from "./lib.js";

  // Details' art picker: every picture of one kind (fanart for now) the
  // sources have for the game, to replace the one shown. The pick is kept
  // even when Enrich refreshes. `view` opens the current one full size;
  // `reload` shows the game again after a pick.
  let { open = $bindable(false), busy = $bindable(""), note = $bindable(""), view, reload } = $props();

  const sourceLabel = { libretro: "libretro", screenscraper: "ScreenScraper", steamgriddb: "SteamGridDB", igdb: "IGDB", launchbox: "LaunchBox" };

  let row = $state(null);
  let kind = $state("hero");
  let label = $state("Fanart");
  let current = $state(null);
  let found = $state(null);
  let loading = $state(false);
  let failure = $state("");

  export async function openPicker(r, k, name, shown) {
    row = r;
    kind = k;
    label = name;
    current = shown ?? null;
    found = null;
    failure = "";
    open = true;
    loading = true;
    try {
      found = await invoke("art_choices", { rowId: r.id, kind: k });
    } catch (e) {
      failure = String(e);
    }
    loading = false;
  }

  async function pick(c) {
    failure = "";
    busy = `Saving the ${label.toLowerCase()}…`;
    try {
      await invoke("choose_art", { rowId: row.id, kind, url: c.url, source: c.source });
      note = `${label} for ${row.label ?? row.title}: ${sourceLabel[c.source] ?? c.source}${c.label ? ` (${c.label})` : ""}; Enrich keeps it`;
      open = false;
      await reload();
    } catch (e) {
      failure = String(e);
    }
    busy = "";
  }
</script>

{#if open && row}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog picker" role="dialog" aria-modal="true" aria-labelledby="art-title">
      <h2 id="art-title">{label} for {row.label ?? row.title}</h2>
      {#if current}
        <div class="now">
          <img src={current} alt="The {label.toLowerCase()} now" />
          <button class="link" onclick={() => view()}>View full size</button>
        </div>
      {/if}
      {#if loading}
        <p class="hint">Asking the sources for every {label.toLowerCase()} they have…</p>
      {:else if found}
        {#if found.choices.length}
          <p class="hint">Pick one to use instead. romforge keeps your pick, even when Enrich refreshes.</p>
          <div class="grid">
            {#each found.choices as c (c.url)}
              <button class="choice" class:current={c.current} onclick={() => pick(c)} disabled={!!busy || c.current} title={c.current ? "In use now" : `Use this one (${c.url})`}>
                <img src={artSrc(c.preview)} loading="lazy" alt="{label} from {sourceLabel[c.source] ?? c.source}" />
                <span class="src">{sourceLabel[c.source] ?? c.source}{c.label ? ` · ${c.label}` : ""}{c.current ? " · in use" : ""}</span>
              </button>
            {/each}
          </div>
        {:else}
          <p class="hint">
            No {label.toLowerCase()} found for it. The LaunchBox database (DATs → Game databases) and keys for SteamGridDB,
            ScreenScraper or IGDB in Settings give more to choose from.
          </p>
        {/if}
        {#if found.errors.length}
          <details><summary class="hint">{found.errors.length} source{found.errors.length === 1 ? "" : "s"} had trouble</summary>
            <ul class="hint">{#each found.errors as e}<li>{e}</li>{/each}</ul>
          </details>
        {/if}
      {/if}
      {#if failure}<p class="err" role="alert">{failure}</p>{/if}
      <div class="actions">
        <span class="spacer"></span>
        <button onclick={() => (open = false)} disabled={!!busy}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .picker { width: min(60rem, calc(100vw - 2rem)); }
  .now { display: flex; flex-direction: column; align-items: center; gap: 0.3rem; margin-bottom: 0.6rem; }
  .now img { max-height: 9rem; max-width: 100%; border-radius: 6px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr)); gap: 0.75rem; }
  .choice { display: flex; flex-direction: column; align-items: center; gap: 0.3rem; padding: 0.35rem; border: 2px solid var(--line); border-radius: 8px; background: var(--panel); cursor: pointer; }
  .choice:hover:not(:disabled) { border-color: var(--accent); }
  .choice.current { border-color: var(--ok); cursor: default; }
  .choice img { width: 100%; aspect-ratio: 16 / 9; object-fit: cover; border-radius: 4px; background: #0003; }
  .src { font-size: 0.75rem; color: var(--muted); text-align: center; }
</style>
