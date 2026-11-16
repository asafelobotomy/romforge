<script>
  import { size, thumbSrc } from "./lib.js";
  import { ago } from "./labels.js";

  // The game picked on the Saves page: its in-game saves and its save
  // states, each on a tab. A state shows the screenshot its emulator took
  // of the moment, when there is one. Read only.
  let { card: c, system, openGame } = $props();

  let tab = $state("saves");
  // Another game picked: its saves first, or its states when it has only
  // those. The same game reloaded keeps its tab.
  let shown = null;
  $effect(() => {
    if (c.name === shown) return;
    shown = c.name;
    tab = c.saves.length || !c.states.length ? "saves" : "states";
  });
  const how = { serial: "matched by serial", file_name: "matched by file name" };
  const parts = (file) => {
    const all = String(file).split("/");
    return [all.pop(), all.join("/")];
  };
  const slotLabel = (s) => (s.slot === "auto" ? "Auto" : s.slot != null ? `Slot ${s.slot}` : "State");
  let broken = $state({});
</script>

<aside class="inspector" aria-label="Saves of {c.name}">
  <header>
    <h2>{c.name}</h2>
    {#if c.row}
      <button class="linkish" onclick={() => openGame(c.row)}>Show in the library</button>
    {:else if !c.game}
      <p class="hint">Not matched to a game: these files name no single game in your library.</p>
    {/if}
  </header>
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "saves"} class:active={tab === "saves"} onclick={() => (tab = "saves")}>Saves <span class="count">{c.saves.length}</span></button>
    <button role="tab" aria-selected={tab === "states"} class:active={tab === "states"} onclick={() => (tab = "states")}>States <span class="count">{c.states.length}</span></button>
  </div>

  {#if tab === "saves"}
    {#if c.saves.length}
      <ul class="files">
        {#each c.saves as s (s.file)}
          {@const [name, folder] = parts(s.file)}
          <li>
            <span class="name" title={s.path}><bdi>{name}</bdi></span>
            <span class="meta">{size(s.size)}{#if s.modified}{" · "}<span title={new Date(s.modified * 1000).toLocaleString()}>{ago(s.modified)}</span>{/if}</span>
            <span class="hint">{[folder && `in ${folder}/`, how[s.matched_by]].filter(Boolean).join(" · ") || `Saves/${system}/`}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="hint">No in-game saves for this game.</p>
    {/if}
  {:else if c.states.length}
    <div class="states">
      {#each c.states as s (s.path)}
        <figure class="state" title={s.path}>
          {#if s.picture && !broken[s.path]}
            <img src={thumbSrc(s.picture, 120)} alt="Screenshot of {slotLabel(s)}" onerror={() => (broken = { ...broken, [s.path]: true })} />
          {:else}
            <span class="noshot">{slotLabel(s)}</span>
          {/if}
          <figcaption>
            <strong>{slotLabel(s)}</strong>
            <span class="meta">{size(s.size)}{#if s.modified}{" · "}{ago(s.modified)}{/if}</span>
            {#if s.old_place}<span class="hint">in the old States/ folder</span>{/if}
          </figcaption>
        </figure>
      {/each}
    </div>
  {:else}
    <p class="hint">No save states for this game.</p>
  {/if}
</aside>

<style>
  .inspector { border-left: 1px solid var(--line); padding: 0 0 1rem 0.9rem; min-width: 0; overflow-y: auto; }
  header h2 { margin: 0.4rem 0 0.2rem; font-size: 1.1rem; overflow-wrap: anywhere; }
  .tabs { display: flex; gap: 0.3rem; margin: 0.6rem 0; border-bottom: 1px solid var(--line); }
  .tabs button { border: none; border-bottom: 2px solid transparent; border-radius: 0; background: none; padding: 0.3rem 0.6rem; }
  .tabs button.active { border-bottom-color: var(--accent); color: var(--accent); }
  .count { font-size: 0.75em; opacity: 0.75; }
  .files { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; font-size: 0.85rem; }
  .files li { display: flex; flex-direction: column; gap: 0.1rem; padding: 0.45rem 0.55rem; border: 1px solid var(--line); border-radius: 6px; }
  .name { font-weight: 600; overflow-wrap: anywhere; }
  .meta { color: var(--muted); }
  .states { display: grid; grid-template-columns: repeat(auto-fill, minmax(10rem, 1fr)); gap: 0.6rem; }
  .state { margin: 0; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; display: flex; flex-direction: column; font-size: 0.8rem; }
  .state img, .noshot { width: 100%; aspect-ratio: 4 / 3; object-fit: cover; display: grid; place-items: center; background: var(--accent-bg, #8883); }
  .noshot { color: var(--muted); font-weight: 700; }
  figcaption { display: flex; flex-direction: column; gap: 0.1rem; padding: 0.35rem 0.5rem; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; }
  .linkish:hover { text-decoration: underline; }
</style>
