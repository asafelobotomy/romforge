<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFile } from "@tauri-apps/plugin-dialog";

  // Advanced, in Emulator Settings → Global: an emulator of your own for a
  // platform, tried before romforge's defaults (what the CLI's
  // `emulator add` and `emulator remove` do).
  let { note = $bindable(""), error = $bindable(""), call } = $props();

  let opened = $state(false);
  let mine = $state([]);
  let systems = $state([]);
  let name = $state("");
  let system = $state("");
  let program = $state("");
  let args = $state("{rom}");
  let archives = $state(false);
  let working = $state(false);

  async function load() {
    mine = (await call("user_emulators", undefined, true)) ?? [];
    if (!systems.length) systems = (await call("all_systems", undefined, true)) ?? [];
  }
  $effect(() => {
    if (opened) load();
  });

  async function browse() {
    const f = await pickFile({ multiple: false, directory: false });
    if (typeof f === "string") program = f;
  }

  async function add() {
    error = "";
    working = true;
    try {
      await invoke("add_emulator", { name, system, program, args, archives });
      note = `${name} added for ${systems.find((s) => s.key === system)?.name ?? system}: it is tried before romforge's own choices.`;
      name = "";
      program = "";
      args = "{rom}";
      archives = false;
      await load();
    } catch (e) {
      error = String(e).replace(/^(settings|launch): /, "");
    }
    working = false;
  }

  async function remove(e) {
    error = "";
    try {
      await invoke("remove_emulator", { name: e.name });
      note = `${e.name} removed.`;
      await load();
    } catch (err) {
      error = String(err);
    }
  }
</script>

<details bind:open={opened} class="mine">
  <summary>Advanced: your own emulators</summary>
  <p class="hint">An emulator romforge doesn't know, or your own build of one, for a platform. It is tried before the defaults, and a game can be set to always use it.</p>
  {#if mine.length}
    <ul>
      {#each mine as e}
        <li>
          <span><b>{e.name}</b> · {systems.find((s) => s.key === e.system)?.name ?? e.system} · <code>{e.program} {e.args.join(" ")}</code></span>
          <span class="spacer"></span>
          <button onclick={() => remove(e)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}
  <section class="setting">
    <label for="ce-name">Name</label>
    <input id="ce-name" bind:value={name} placeholder="My Snes9x" />
    <label for="ce-system">Platform</label>
    <select id="ce-system" bind:value={system}>
      <option value="">Choose…</option>
      {#each systems as s}<option value={s.key}>{s.name}</option>{/each}
    </select>
    <label for="ce-program">Program (a name on your PATH, or a full path)</label>
    <div class="pick">
      <input id="ce-program" bind:value={program} placeholder="/opt/snes9x/snes9x" />
      <button onclick={browse}>Browse…</button>
    </div>
    <label for="ce-args">Arguments, with <code>{"{rom}"}</code> for the game's file</label>
    <input id="ce-args" bind:value={args} placeholder={"--fullscreen {rom}"} />
    <label class="toggle"><input type="checkbox" bind:checked={archives} /> It opens ROMs inside zip and 7z files itself</label>
    <div class="actions">
      <button class="primary" onclick={add} disabled={working || !name.trim() || !system || !program.trim()}>Add emulator</button>
    </div>
  </section>
</details>

<style>
  .mine { margin-top: 0.8rem; }
  ul { list-style: none; padding: 0; margin: 0.3rem 0; }
  li { display: flex; gap: 0.5rem; align-items: center; padding: 0.2rem 0; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  .spacer { flex: 1; }
  .pick { display: flex; gap: 0.4rem; }
  .pick input { flex: 1; }
</style>
