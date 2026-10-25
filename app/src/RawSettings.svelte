<script>
  import { invoke } from "@tauri-apps/api/core";
  import { edit, run } from "./jobs.svelte.js";

  // "All settings" on an emulator's tab: every key in romforge's copy of
  // its config, named as the emulator names it. romforge's own keys are
  // locked (the core says why). Changes are written when saved here,
  // journaled for undo; the core refuses them while the emulator runs.
  let { emulator, label, note = $bindable(""), error = $bindable("") } = $props();

  let view = $state(null);
  let shown = $state(false);
  let search = $state("");
  // file → key → new value ("" with `remove` set: taken out).
  let changes = $state({});
  let adding = $state({ file: "", key: "", value: "" });

  async function load() {
    view = await invoke("raw_settings", { emulator }).catch((e) => {
      error = String(e).replace(/^settings: /, "");
      return null;
    });
    changes = {};
    adding = { file: view?.files[0]?.file ?? "", key: "", value: "" };
  }
  $effect(() => {
    emulator;
    if (shown) load();
  });

  // INI keys are `[Section]Key`: grouped by their section.
  const section = (key) => (key.startsWith("[") ? key.slice(1, key.indexOf("]")) : "");
  const name = (key) => (key.startsWith("[") ? key.slice(key.indexOf("]") + 1) : key);
  const matches = (s) => !search || `${s.key} ${s.value}`.toLowerCase().includes(search.toLowerCase());
  function groups(f) {
    const out = new Map();
    for (const s of f.settings.filter(matches)) {
      const g = section(s.key);
      if (!out.has(g)) out.set(g, []);
      out.get(g).push(s);
    }
    return [...out];
  }
  const value = (f, s) => changes[f.file]?.[s.key] ?? s.value;
  function set(f, s, v) {
    changes[f.file] ??= {};
    if (v === s.value) delete changes[f.file][s.key];
    else changes[f.file][s.key] = v;
  }
  const pending = () => Object.entries(changes).flatMap(([file, keys]) => Object.entries(keys).map(([key, v]) => ({ file, key, value: v })));

  async function save(edits) {
    error = "";
    try {
      await edit(`Saving ${label}'s settings…`, "edit_raw_settings", { emulator, edits });
      note = `${label}'s settings saved (${edits.length} change${edits.length === 1 ? "" : "s"}); they apply from its next start.`;
      await load();
    } catch (e) {
      error = String(e).replace(/^settings: /, "");
    }
  }
  function add() {
    if (!adding.key.trim()) return;
    save([{ file: adding.file, key: adding.key.trim(), value: adding.value }]);
  }
  async function undo() {
    error = "";
    try {
      const n = await edit(`Undoing ${label}'s last change…`, "undo_raw_settings", { emulator });
      note = `Last change to ${label}'s settings taken back (${n} key${n === 1 ? "" : "s"}).`;
      await load();
    } catch (e) {
      error = String(e).replace(/^settings: /, "");
    }
  }
  async function defaults() {
    const at = await run("Asking MAME for all its settings…", "mame_write_defaults", {});
    if (at) {
      note = `MAME wrote all its settings into ${at}.`;
      await load();
    }
  }
</script>

<section class="setting">
  <span class="flabel">All settings</span>
  {#if !shown}
    <p class="hint">Every setting in romforge's copy of {label}'s config, as {label} names it.</p>
    <div class="pick"><button onclick={() => (shown = true)}>Show all settings</button></div>
  {:else if view}
    {#if view.running}<p class="err">{label} is running: close it to change its settings here.</p>{/if}
    <div class="pick">
      <input type="search" placeholder="Find a setting" bind:value={search} />
      {#if view.undo}<button onclick={undo} disabled={view.running}>Undo last change</button>{/if}
      {#if view.write_defaults}<button onclick={defaults} disabled={view.running}>Write MAME's defaults</button>{/if}
    </div>
    {#each view.files as f}
      <h4><code>{f.file}</code></h4>
      {#if !f.exists}
        <p class="hint">Not written yet: start a game with {label} from romforge once{f.settings.length ? "" : ", or add a setting below"}.</p>
      {/if}
      {#each groups(f) as [g, rows]}
        <details open={!!search}>
          <summary>{g || "Settings"} <span class="hint">({rows.length})</span></summary>
          <table>
            <tbody>
              {#each rows as s}
                <tr title={s.locked ?? ""}>
                  <td><code>{name(s.key)}</code></td>
                  <td>
                    {#if s.locked}
                      <span class="hint">{s.value} · locked: {s.locked}</span>
                    {:else if s.kind.kind === "bool"}
                      <input type="checkbox" checked={value(f, s) === s.kind.on} disabled={view.running} onchange={(ev) => set(f, s, ev.currentTarget.checked ? s.kind.on : s.kind.off)} />
                    {:else}
                      <input type={s.kind.kind === "number" ? "number" : "text"} step="any" value={value(f, s)} disabled={view.running} onchange={(ev) => set(f, s, ev.currentTarget.value)} />
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </details>
      {/each}
    {/each}
    <div class="pick">
      <button class="primary" disabled={view.running || !pending().length} onclick={() => save(pending())}>Save {pending().length || ""} change{pending().length === 1 ? "" : "s"}</button>
    </div>
    <p class="hint">Add a setting by its exact name ({"[Section]Key"} in an INI file, <code>Video/Renderer</code> in RPCS3's), written as the file writes it.</p>
    <div class="pick">
      <select bind:value={adding.file}>{#each view.files as f}<option value={f.file}>{f.file}</option>{/each}</select>
      <input placeholder="Name" bind:value={adding.key} />
      <input placeholder="Value" bind:value={adding.value} />
      <button onclick={add} disabled={view.running || !adding.key.trim()}>Add</button>
    </div>
  {/if}
</section>

<style>
  h4 { margin: 0.6rem 0 0.2rem; font-size: 0.85rem; }
  table { width: 100%; border-collapse: collapse; }
  td { padding: 0.1rem 0.3rem; border-top: 1px solid var(--line); vertical-align: middle; }
  td:first-child { width: 45%; word-break: break-all; }
  td input[type="text"], td input[type="number"] { width: 100%; }
  summary { cursor: pointer; margin: 0.2rem 0; }
</style>
