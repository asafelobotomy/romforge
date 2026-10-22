<script>
  import { listLabel } from "../listLabel.js";
  import { LABELS, ROWS, keyAt, keyMove, search, typed } from "./fskeys.js";
  import { listMove } from "./fsnav.js";

  // Quick search in the fullscreen mode (stage 2, step 6): an on-screen
  // keyboard for the pad, results as you type, across every platform. A
  // real keyboard types straight in. `open(row)` shows a game's page;
  // `close()` goes back home.
  let { all, open, close } = $props();
  let query = $state("");
  let pos = $state({ row: 0, col: 0 });
  let on = $state("keys");
  let pick = $state(0);
  let found = $derived(search(all, query));

  function press(key) {
    if (key === "done") {
      if (found.length) (on = "results"), (pick = 0);
      return;
    }
    query = typed(query, key);
  }

  // One pad action.
  export function pad(action) {
    if (on === "results") {
      if (action === "accept") return found[pick] && open(found[pick]);
      if (action === "back" || (action === "up" && pick === 0)) return void (on = "keys");
      pick = listMove(pick, found.length, action === "left" ? "up" : action === "right" ? "down" : action);
      return;
    }
    if (action === "accept") press(keyAt(pos));
    else if (action === "back") query ? (query = typed(query, "delete")) : close();
    else if (action === "north") press("space");
    else if (action === "menu") press("done");
    else pos = keyMove(pos, action);
  }

  // One key from a real keyboard: letters and digits type, the rest steer.
  export function key(e) {
    if (e.key.length === 1 && /[\p{L}\p{N} '&:-]/u.test(e.key)) {
      query = typed(query, e.key === " " ? "space" : e.key);
      on = "keys";
    } else if (e.key === "Backspace") query = typed(query, "delete");
    else if (e.key === "Escape") close();
    else if (e.key === "Enter") on === "results" ? pad("accept") : press("done");
    else if (e.key === "ArrowDown" && on === "keys" && found.length && pos.row === ROWS.length - 1) (on = "results"), (pick = 0);
    else {
      const a = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" }[e.key];
      if (!a) return;
      pad(a);
    }
    e.preventDefault();
  }
</script>

<section class="search" data-fs-search>
  <div class="line" aria-live="polite">
    <span class="q">{query || "Type a title"}</span><span class="caret">▏</span>
    <span class="n">{query ? `${found.length}${found.length === 40 ? "+" : ""} found` : ""}</span>
  </div>
  <div class="keyboard" data-fs-keyboard>
    {#each ROWS as row, r}
      <div class="row">
        {#each row as k, c}
          <button class="key" class:wide={k.length > 1} class:on={on === "keys" && pos.row === r && pos.col === c} data-fs-key={k} onclick={() => ((pos = { row: r, col: c }), press(k))}>{LABELS[k] ?? k}</button>
        {/each}
      </div>
    {/each}
  </div>
  <ol class="results">
    {#each found as g, i (g.id)}
      <li>
        <button class="hit" class:on={on === "results" && pick === i} data-fs-row={g.id} onclick={() => open(g)}>
          <span data-fs-title>{listLabel(g)}</span><span class="sys">{g.system_name}</span>
        </button>
      </li>
    {/each}
  </ol>
</section>

<style>
  .search { display: grid; grid-template-rows: auto auto minmax(0, 1fr); gap: 2.4vh; min-height: 0; padding: 4vh 8vw 2vh; }
  .line { display: flex; align-items: baseline; gap: 1vw; padding: 1.4vh 1.6vw; border-radius: var(--fs-radius); background: var(--fs-panel); border: 1px solid var(--fs-line); font-size: calc(3.4vh * var(--fs-scale)); }
  .q { font-family: var(--fs-title-font); }
  .caret { color: var(--fs-accent); animation: blink 1s steps(1) infinite; }
  .n { margin-left: auto; color: var(--fs-muted); font-size: 0.6em; }
  .keyboard { display: grid; gap: 1vh; justify-content: center; }
  .row { display: flex; gap: 0.8vw; justify-content: center; }
  .key { min-width: 6.2vh; height: 6.2vh; padding: 0 1vw; border: 1px solid var(--fs-line); border-radius: calc(var(--fs-radius) * 0.8); background: var(--fs-panel); color: var(--fs-text); font: inherit; font-size: calc(2.6vh * var(--fs-scale)); font-weight: 600; cursor: pointer; }
  .key.wide { min-width: 18vh; }
  .key.on, .hit.on { background: var(--fs-accent); color: var(--fs-accent-text); outline: 0.4vh solid var(--fs-focus); outline-offset: 0.2vh; }
  .results { list-style: none; margin: 0; padding: 0; overflow: hidden; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); grid-auto-rows: max-content; gap: 0.6vh 2vw; }
  .hit { display: flex; gap: 1vw; align-items: baseline; width: 100%; padding: 0.8vh 1vw; border: 0; border-radius: calc(var(--fs-radius) * 0.6); background: none; color: var(--fs-text); font: inherit; font-size: calc(2.2vh * var(--fs-scale)); text-align: left; cursor: pointer; }
  .hit span[data-fs-title] { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sys { color: inherit; opacity: 0.7; font-size: 0.8em; white-space: nowrap; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .caret { animation: none; } }
</style>
