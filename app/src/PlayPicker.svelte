<script>
  import UiIcon from "./UiIcon.svelte";
  import { displayTitle } from "./labels.js";
  import { versionKind, versionName } from "./versions.js";
  import { ui } from "./ui.svelte.js";

  // Play on a game with several versions: each playable one with its own
  // Play button, and "Always run" (unticked to start). Ticked, the version
  // played is kept and starts without asking next time; Details' Versions
  // tab asks again. `ask(game, versions, start)` opens it; `call` keeps the
  // choice.
  let { open = $bindable(false), call } = $props();
  let game = $state(null);
  let versions = $state([]);
  let always = $state(false);
  let list = $state(null);
  let start = () => {};

  export function ask(g, vs, then) {
    game = g;
    start = then;
    versions = vs;
    always = false;
    open = true;
  }
  // The first Play has the focus, so Enter (or a pad's A) plays the main version.
  $effect(() => void (open && list?.querySelector("button")?.focus()));

  async function play(v) {
    open = false;
    if (always) {
      await call("set_always_run", { rowId: game.id, version: v.id });
      ui.alwaysRun++;
    }
    start(v);
  }
</script>

{#if open && game}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="play-title">
      <h2 id="play-title">Play {displayTitle(game)}</h2>
      <p class="hint">You have {versions.length} versions of this game. Pick the one to play.</p>
      <ul class="versions" bind:this={list}>
        {#each versions as v (v.id)}
          <li>
            <span class="name" title={versionName(v)}>{versionName(v)}</span>
            <span class="kind">{versionKind(v)}</span>
            <button class="primary" onclick={() => play(v)} aria-label="Play {versionName(v)}"><UiIcon name="play" size={14} />Play</button>
          </li>
        {/each}
      </ul>
      <label class="toggle always"><input type="checkbox" bind:checked={always} />Always run</label>
      <p class="hint">Ticked, the version you play starts straight away next time. Details' Versions tab can ask again.</p>
      <div class="actions"><button onclick={() => (open = false)}>Cancel</button></div>
    </div>
  </div>
{/if}

<style>
  .versions { list-style: none; padding: 0; margin: 0.4rem 0 0.6rem; max-height: 22rem; overflow-y: auto; }
  .versions li { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 0.75rem; align-items: center; padding: 0.35rem 0; border-bottom: 1px solid var(--line); }
  .name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kind { color: var(--muted); font-size: 0.75rem; }
  .versions button { display: inline-flex; align-items: center; gap: 0.3rem; }
  .always { font-weight: 600; }
  .always + .hint { margin: 0.15rem 0 0; }
</style>
