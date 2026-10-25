<script>
  import GameDataPanel from "./GameDataPanel.svelte";
  import SavesPanel from "./SavesPanel.svelte";
  import BackupPanel from "./BackupPanel.svelte";
  import { edit, run } from "./jobs.svelte.js";
  import { size } from "./lib.js";
  import { ask, open as pickFile } from "@tauri-apps/plugin-dialog";
  import CoresPanel from "./CoresPanel.svelte";
  import HotkeysPanel from "./HotkeysPanel.svelte";
  import RawSettings from "./RawSettings.svelte";

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
    if (tab !== "global" && tab !== "hotkeys" && !v.installed.some((e) => e.name === tab)) tab = "global";
    open = true;
  }

  const def = (key) => view.settings.find((s) => s.key === key);
  const globals = () => view.settings.filter((s) => s.global);
  // The value an emulator gets when it sets none ("" = its own config's).
  const globalValue = (key) => draft.global[key] || def(key).default;
  // A value as the dialog shows it.
  function shown(s, v) {
    if (!v) return "emulator's own";
    if (s.kind === "bool") return v === "false" ? "Off" : "On";
    return s.choices.find(([c]) => c === v)?.[1] ?? v;
  }
  // The installed emulators a global setting reaches.
  const usedBy = (key) => view.installed.filter((e) => e.settings.includes(key)).map((e) => e.label);
  // A setting's choices for a select: On/Off for a yes-or-no one.
  const options = (s) => (s.kind === "bool" ? [["true", "On"], ["false", "Off"]] : s.choices);
  // Those an emulator has (Dolphin has no automatic renderer).
  const optionsFor = (s, e) => options(s).filter(([v]) => !e.choices[s.key] || e.choices[s.key].includes(v));

  async function browse(name, key) {
    const file = await pickFile({ multiple: false, directory: false, title: "Program to start" });
    if (typeof file === "string") draft.emulators[name][key] = file;
  }

  // "Import my existing setup": copy what the emulator keeps in its usual
  // folders (memory cards, saves, controller mappings, settings) into
  // romforge's folder for it, or take the last import back.
  let setupBusy = $state(false);
  async function setup(name, undo) {
    const ok = await ask(
      undo
        ? `Take back the last setup import for ${name}?`
        : `Copy ${name}'s existing setup (saves, memory cards, mappings) into romforge?`,
      { title: undo ? "Undo setup import" : "Import setup", kind: "warning" },
    );
    if (!ok) return;
    error = "";
    setupBusy = true;
    // Jobs in the queue (footer); their errors go to the status line.
    if (undo) {
      const n = await run(`Taking back ${name}'s setup…`, "undo_emulator_setup", { emulator: name });
      if (n != null) note = `Setup import taken back: ${n} file${n === 1 ? "" : "s"} removed.`;
    } else {
      const r = await run(`Copying ${name}'s setup…`, "import_emulator_setup", { emulator: name });
      if (r) note = `Setup imported: ${r.copied} file${r.copied === 1 ? "" : "s"} copied` + (r.saves ? ` (${r.saves} saves and states)` : "") + (r.kept ? `, ${r.kept} already there kept.` : ".");
    }
    if (open) {
      const keep = draft;
      await openEmulatorSettings();
      draft = keep;
    }
    setupBusy = false;
  }

  // Install an emulator's official Flatpak (Flathub, or its developers'
  // own repository), then look again.
  let installing = $state("");
  // An emulator's official AppImage (RPCS3's), from its own releases, into
  // RomForge/Emulators/; romforge prefers it to the Flatpak.
  async function appimage(n) {
    const ok = await ask(`Download ${n.label}'s official AppImage from its own releases into RomForge's Emulators folder?`, {
      title: "Download AppImage",
      kind: "info",
    });
    if (!ok) return;
    error = "";
    installing = `appimage:${n.name}`;
    const at = await run(`Downloading ${n.label}'s AppImage…`, "install_appimage", { emulator: n.name }, { key: `appimage:${n.name}` });
    if (at) note = `${n.label}'s AppImage is in ${at}; romforge starts it from now on.`;
    if (at && open) {
      const keep = draft;
      await openEmulatorSettings();
      draft = { ...keep, emulators: { ...draft.emulators, ...keep.emulators } };
    }
    installing = null;
  }

  async function install(n) {
    const fallback = n.flatpak.includes("not official")
      ? ` ${n.label} makes no Linux build of its own: this is Flathub's, the one most used, made by others.`
      : "";
    const replace = n.unofficial_flatpak ? ` The Flatpak of it you have now isn't its developers' and is replaced.` : "";
    const ok = await ask(`Install ${n.label} from ${n.flatpak} into your Flatpak? This can take a while.${fallback}${replace}`, {
      title: "Install emulator",
      kind: "info",
    });
    if (!ok) return;
    error = "";
    installing = n.name;
    const id = await run(`Installing ${n.label} from ${n.flatpak}…`, "install_emulator", { emulator: n.name }, { key: `install:${n.name}` });
    if (id) note = `${n.label} installed (${id}).`;
    if (id && open) {
      const keep = draft;
      await openEmulatorSettings();
      draft = { ...keep, emulators: { ...draft.emulators, ...keep.emulators } };
    }
    installing = "";
  }

  async function save() {
    error = "";
    try {
      // Behind a scan or the like it waits its turn: the dialog closes.
      const onWait = (why) => {
        open = false;
        note = `Saving emulator settings ${why}`;
      };
      await edit("Saving emulator settings…", "save_emulator_settings", { settings: draft }, { onWait });
      open = false;
      note = "Emulator settings saved; they apply from the next game you start.";
    } catch (e) {
      error = String(e).replace(/^settings: /, "");
    }
  }
</script>

{#if open && view}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog tall" role="dialog" aria-modal="true" aria-labelledby="emu-title">
      <h2 id="emu-title">Emulator Settings</h2>

      <div class="etabs" role="tablist">
        <button role="tab" aria-selected={tab === "global"} class:active={tab === "global"} onclick={() => (tab = "global")}>Global</button>
        <button role="tab" aria-selected={tab === "hotkeys"} class:active={tab === "hotkeys"} onclick={() => (tab = "hotkeys")}>Hotkeys</button>
        {#each view.installed as e}
          <button role="tab" aria-selected={tab === e.name} class:active={tab === e.name} onclick={() => (tab = e.name)}>{e.label}</button>
        {/each}
      </div>

      <div class="body">
      {#if tab === "hotkeys"}
        <HotkeysPanel installed={view.installed} on={globalValue("hotkeys") !== "false"} />
      {:else if tab === "global"}
        <p class="hint">For every emulator at once. An emulator's own tab can set something else for it.</p>
        {#each globals() as s}
          {@const users = usedBy(s.key)}
          <section class="setting">
            {#if s.kind === "bool" && s.default}
              <label class="toggle">
                <input type="checkbox" checked={globalValue(s.key) === "true"} onchange={(ev) => (draft.global[s.key] = ev.currentTarget.checked ? "true" : "false")} />
                {s.label}
              </label>
            {:else}
              <label for="g-{s.key}">{s.label}</label>
              <select id="g-{s.key}" value={draft.global[s.key] ?? ""} onchange={(ev) => (draft.global[s.key] = ev.currentTarget.value)}>
                <option value="">Each emulator's own</option>
                {#each options(s) as [v, label]}<option value={v}>{label}</option>{/each}
              </select>
            {/if}
            <p class="hint">{s.hint} {users.length ? `Used by ${users.join(", ")}.` : "No installed emulator uses it yet."}</p>
          </section>
        {/each}
        {#if view.installed.length === 0}
          <p class="hint">No emulator is installed yet. romforge finds RetroArch and standalone emulators on your PATH, as Flatpaks, and as AppImages in ~/Applications or RomForge/Emulators.</p>
        {/if}
        {#if view.not_installed.length}
          <section class="setting">
            <span class="flabel">Not installed</span>
            <p class="hint">Each emulator's official build: its developers' Flatpak (from Flathub or their own repository) into your own Flatpak installation (no password needed), or their AppImage into RomForge's Emulators folder. For one whose developers make no Linux build, the Linux build most used, marked as not official. It shows up here once done.</p>
            <ul class="missing">
              {#each view.not_installed as n}
                <li>
                  {n.label}
                  {#if n.flatpak}
                    <button onclick={() => install(n)} disabled={!!installing}>{installing === n.name ? "Installing…" : n.flatpak === "Flathub" ? "Install from Flathub" : n.flatpak.includes("not official") ? "Install Flathub's (not official)" : "Install its official Flatpak"}</button>
                  {/if}
                  {#if n.appimage}
                    <button onclick={() => appimage(n)} disabled={!!installing}>{installing === `appimage:${n.name}` ? "Downloading…" : "Download official AppImage"}</button>
                  {/if}
                  {#if n.unofficial_flatpak}<span class="hint">· the Flatpak you have isn't its developers': not used</span>{/if}
                  {#if n.flatpak || n.appimage}{""}
                  {:else if n.discontinued}<span class="hint">· discontinued (last release {n.discontinued}): romforge uses your own install and never downloads it</span>
                  {:else}<span class="hint">· no official Flatpak or AppImage: install it from your distribution</span>{/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      {:else}
        {@const e = view.installed.find((x) => x.name === tab)}
        {@const mine = draft.emulators[e.name]}
        <p class="hint">Started as <code>{e.found}</code>{#if e.systems.length}; runs {e.systems.join(", ")}{/if}.</p>
        {#if e.name === "retroarch"}<CoresPanel bind:note bind:error />{/if}
        {#if e.existing}
          <section class="setting">
            <span class="flabel">Your existing setup</span>
            {#if e.existing.sources.length}
              <p class="hint">
                romforge starts {e.label} from its own folder, so what's in its usual one isn't seen there: {e.existing.files}
                file{e.existing.files === 1 ? "" : "s"} ({size(e.existing.bytes)}) in {e.existing.sources.map((s) => s.from).join(", ")}{#if e.existing.saves}, and {e.existing.saves} save{e.existing.saves === 1 ? "" : "s"} and state{e.existing.saves === 1 ? "" : "s"} that go into romforge's folder for each platform{/if}{#if e.existing.unplaced} ({e.existing.unplaced} more whose platform can't be told stay where they are){/if}.
                Importing copies them across (memory cards, saves, controller mappings, settings; caches stay behind). Your
                files aren't touched, and nothing already in romforge's folder is replaced except its settings file.
              </p>
              <div class="pick">
                <button onclick={() => setup(e.name, false)} disabled={setupBusy}>Import my existing setup</button>
                <button onclick={() => setup(e.name, true)} disabled={setupBusy}>Undo import</button>
              </div>
            {:else}
              <p class="hint">No existing {e.label} setup found on this computer.</p>
            {/if}
          </section>
        {/if}
        {#if e.fallback}
          <section class="setting">
            <span class="flabel">Not an official build</span>
            <p class="hint">{e.fallback}</p>
          </section>
        {/if}
        {#if e.movable}<GameDataPanel emulator={e.name} label={e.label} bind:note bind:error />{/if}
        {#if e.gathers}<SavesPanel emulator={e.name} label={e.label} bind:note bind:error />{/if}
        {#if e.backs_up}<BackupPanel emulator={e.name} label={e.label} bind:note bind:error />{/if}
        {#each e.settings.map(def) as s}
          <section class="setting">
            {#if s.kind === "bool" || s.kind === "choice"}
              <label for="emu-{s.key}">{s.label}</label>
              <p class="hint">{s.hint}</p>
              <select id="emu-{s.key}" value={mine[s.key] ?? ""} onchange={(ev) => (mine[s.key] = ev.currentTarget.value)}>
                <option value="">{s.global ? `As Global (${shown(s, globalValue(s.key))})` : "Emulator's own"}</option>
                {#each optionsFor(s, e) as [v, label]}<option value={v}>{label}</option>{/each}
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
        <RawSettings emulator={e.name} label={e.label} bind:note bind:error />
      {/if}
      </div>

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
  .missing { list-style: none; padding: 0; margin: 0.3rem 0 0; }
  .missing li { display: flex; gap: 0.5rem; align-items: center; justify-content: space-between; padding: 0.2rem 0; border-top: 1px solid var(--line); }
</style>
