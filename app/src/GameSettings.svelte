<script>
  import { invoke } from "@tauri-apps/api/core";
  import { edit } from "./jobs.svelte.js";

  // "Settings for this game": the settings RetroArch (a game override) and
  // DuckStation (its game settings file) can have for one game, each "the
  // emulator's setting" unless set here. The core writes them where the
  // emulator reads them when the game starts.
  let { row, note = $bindable(""), error = $bindable("") } = $props();

  let view = $state(null);
  let draft = $state({});

  async function load() {
    view = await invoke("game_settings", { system: row.system, title: row.title }).catch(() => null);
    draft = { ...(view?.values ?? {}) };
  }
  $effect(() => {
    row.system;
    row.title;
    load();
  });

  const options = (s) => (s.kind === "bool" ? [["true", "On"], ["false", "Off"]] : s.choices);
  const names = { retroarch: "RetroArch", duckstation: "DuckStation" };

  async function save() {
    error = "";
    try {
      await edit("Saving this game's settings…", "set_game_settings", { system: row.system, title: row.title, values: draft });
      note = "This game's settings are saved; they apply when it next starts.";
      await load();
    } catch (e) {
      error = String(e).replace(/^settings: /, "");
    }
  }
</script>

{#if view?.settings.length}
  <div class="group">
    <h3>Settings for this game</h3>
    <p class="hint">Only for this game, over the emulator's own. DuckStation needs the disc's serial, read when it was scanned.</p>
    <dl class="facts">
      {#each view.settings as g}
        <dt><label for="game-{g.setting.key}">{g.setting.label}</label></dt>
        <dd>
          <select id="game-{g.setting.key}" value={draft[g.setting.key] ?? ""} onchange={(ev) => (draft[g.setting.key] = ev.currentTarget.value)} title={g.setting.hint}>
            <option value="">The emulator's setting</option>
            {#each options(g.setting) as [v, label]}<option value={v}>{label}</option>{/each}
          </select>
          <span class="hint">{g.emulators.map((e) => names[e] ?? e).join(", ")}</span>
        </dd>
      {/each}
    </dl>
    <button onclick={save}>Save for this game</button>
  </div>
{:else if view}
  <p class="hint">No emulator for this platform reads settings of a game's own (RetroArch and DuckStation do).</p>
{/if}
