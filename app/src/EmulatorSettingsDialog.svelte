<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFile } from "@tauri-apps/plugin-dialog";

  // Emulator Settings: a Global tab for every emulator at once, then one
  // tab per installed emulator. An emulator takes the global value unless
  // its own tab sets one. What the settings are comes from the core
  // (`emusettings::SETTINGS`), so this only draws them.
  let { open = $bindable(false), note = $bindable(""), call } = $props();
  let view = $state(null);
  // The values being edited, saved on "Save": { global, emulators }.
  let draft = $state({ global: {}, emulators: {} });
  let tab = $state("global");
  let error = $state("");

  export async function openEmulatorSettings() {
    error = "";
    const v = await call("get_emulator_settings");
    if (!v) return;
    view = v;
    draft = { global: { ...v.values.global }, emulators: {} };
    for (const e of v.installed) draft.emulators[e.name] = { ...(v.values.emulators[e.name] ?? {}) };
    if (tab !== "global" && !v.installed.some((e) => e.name === tab)) tab = "global";
    open = true;
  }

  const def = (key) => view.settings.find((s) => s.key === key);
  const globals = () => view.settings.filter((s) => s.global);
  // The value an emulator gets when it sets none.
  const globalValue = (key) => draft.global[key] ?? def(key).default;
  const onOff = (v) => (v === "false" ? "Off" : "On");

  async function browse(name, key) {
    const file = await pickFile({ multiple: false, directory: false, title: "Program to start" });
    if (typeof file === "string") draft.emulators[name][key] = file;
  }

  async function save() {
    error = "";
    try {
      await invoke("save_emulator_settings", { settings: draft });
      open = false;
      note = "Emulator settings saved; they apply from the next game you start.";
    } catch (e) {
      error = String(e).replace(/^settings: /, "");
    }
  }
</script>

{#if open && view}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="emu-title">
      <h2 id="emu-title">Emulator Settings</h2>

      <div class="etabs" role="tablist">
        <button role="tab" aria-selected={tab === "global"} class:active={tab === "global"} onclick={() => (tab = "global")}>Global</button>
        {#each view.installed as e}
          <button role="tab" aria-selected={tab === e.name} class:active={tab === e.name} onclick={() => (tab = e.name)}>{e.label}</button>
        {/each}
      </div>

      {#if tab === "global"}
        <p class="hint">For every emulator at once. An emulator's own tab can set something else for it.</p>
        {#each globals() as s}
          <section class="setting">
            {#if s.kind === "bool"}
              <label class="toggle">
                <input type="checkbox" checked={globalValue(s.key) === "true"} onchange={(ev) => (draft.global[s.key] = ev.currentTarget.checked ? "true" : "false")} />
                {s.label}
              </label>
            {/if}
            <p class="hint">{s.hint}</p>
          </section>
        {/each}
        {#if view.installed.length === 0}
          <p class="hint">No emulator is installed yet. romforge finds RetroArch and standalone emulators on your PATH, as Flatpaks, and as AppImages in ~/Applications or RomForge/Emulators.</p>
        {/if}
        {#if view.not_installed.length}
          <p class="hint">Not installed: {view.not_installed.join(", ")}.</p>
        {/if}
      {:else}
        {@const e = view.installed.find((x) => x.name === tab)}
        {@const mine = draft.emulators[e.name]}
        <p class="hint">Started as <code>{e.found}</code>{#if e.systems.length}; runs {e.systems.join(", ")}{/if}.</p>
        {#each e.settings.map(def) as s}
          <section class="setting">
            {#if s.kind === "bool"}
              <label for="emu-{s.key}">{s.label}</label>
              <p class="hint">{s.hint}</p>
              <select id="emu-{s.key}" value={mine[s.key] ?? ""} onchange={(ev) => (mine[s.key] = ev.currentTarget.value)}>
                <option value="">As Global ({onOff(globalValue(s.key))})</option>
                <option value="true">On</option>
                <option value="false">Off</option>
              </select>
            {:else if s.kind === "path"}
              <label for="emu-{s.key}">{s.label}</label>
              <p class="hint">{s.hint}</p>
              <div class="pick">
                <input id="emu-{s.key}" bind:value={mine[s.key]} placeholder="Found by romforge" />
                <button onclick={() => browse(e.name, s.key)}>Browse…</button>
                {#if mine[s.key]}<button onclick={() => (mine[s.key] = "")}>Clear</button>{/if}
              </div>
            {:else}
              <label for="emu-{s.key}">{s.label}</label>
              <p class="hint">{s.hint}</p>
              <input id="emu-{s.key}" bind:value={mine[s.key]} placeholder="None" />
            {/if}
          </section>
        {/each}
      {/if}

      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="actions">
        <button onclick={() => (open = false)}>Cancel</button>
        <button class="primary" onclick={save}>Save</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .etabs { display: flex; gap: 0.25rem; flex-wrap: wrap; border-bottom: 1px solid var(--line); margin: 0.25rem 0 0.5rem; }
  .etabs button { border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 0.35rem 0.7rem; color: var(--muted); background: none; }
  .etabs button.active { color: var(--text); border-bottom-color: var(--accent); }
  select { min-width: 12rem; }
  code { font-size: 0.8rem; word-break: break-all; }
</style>
