<script>
  import { thumbSrc } from "./lib.js";
  import { ago } from "./labels.js";

  // One game on the Saves page: its box art (in the shape of its box) and
  // how many saves and save states it has. Picking it opens the inspector
  // beside the cards. A game nothing matched shows its first file's type.
  let { card: c, picked = false, pick } = $props();

  const COVER_H = 120;
  let broken = $state(false);
  const src = $derived(c.cover && !broken ? thumbSrc(c.cover, COVER_H) : null);
  const width = $derived(Math.round((COVER_H * (c.box_aspect || 72)) / 100));
  const first = $derived(String((c.saves[0] ?? c.states[0])?.file ?? ""));
  const kind = $derived(first.includes(".") ? first.split(".").pop().toUpperCase().slice(0, 5) : "SAVE");
  const n = (k, one, many) => (k === 1 ? `1 ${one}` : `${k} ${many}`);
</script>

<button class="card" class:picked onclick={pick} aria-pressed={picked} title={c.game ? c.game : `${c.name} (not matched to a game)`}>
  <span class="cover" style="width: {width}px; height: {COVER_H}px">
    {#if src}
      <img {src} alt="" onerror={() => (broken = true)} />
    {:else}
      <span class="kind">{kind}</span>
    {/if}
  </span>
  <span class="about">
    <span class="game" class:none={!c.game}>{c.name}</span>
    {#if !c.game}<span class="hint">Not matched</span>{/if}
    <span class="counts">
      {#if c.saves.length}<span class="pill">{n(c.saves.length, "save", "saves")}</span>{/if}
      {#if c.states.length}<span class="pill state">{n(c.states.length, "state", "states")}</span>{/if}
    </span>
    {#if c.latest}<span class="meta" title={new Date(c.latest * 1000).toLocaleString()}>{ago(c.latest)}</span>{/if}
  </span>
</button>

<style>
  .card { display: flex; gap: 0.7rem; padding: 0.55rem; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel, transparent); min-width: 0; text-align: left; font: inherit; color: inherit; cursor: pointer; }
  .card:hover { border-color: var(--accent); }
  .card.picked { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
  .cover { flex: none; border-radius: var(--radius-small); overflow: hidden; display: grid; place-items: center; background: var(--accent-bg, #8883); }
  .cover img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .kind { font-size: 0.75rem; font-weight: 700; letter-spacing: 0.05em; color: var(--muted); }
  .about { display: flex; flex-direction: column; gap: 0.3rem; min-width: 0; font-size: 0.82rem; }
  .game { font-weight: 700; font-size: 0.92rem; line-height: 1.2; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .game.none { font-weight: 600; }
  .counts { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .pill { font-size: 0.72rem; padding: 0.05rem 0.45rem; border-radius: 0.7rem; background: var(--accent-bg, #8883); }
  .pill.state { outline: 1px solid var(--line); background: none; }
  .meta { color: var(--muted); }
</style>
