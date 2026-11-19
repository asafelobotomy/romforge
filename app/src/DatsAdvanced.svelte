<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFiles } from "@tauri-apps/plugin-dialog";

  // Advanced, on the DATs screen: DAT files from disk, the DATs of one
  // platform, and which of No-Intro's you lack or have an old copy of
  // (the CLI's `dat import`, `dat download KEY` and `dat check`).
  // `run` queues a job; `reload` reads the DATs and the library again.
  let { busy, extras, note = $bindable(""), run, reload } = $props();

  let opened = $state(false);
  let systems = $state([]);
  let system = $state("");
  let status = $state(null);
  let checking = $state(false);
  let failure = $state("");

  $effect(() => {
    if (opened && !systems.length) invoke("all_systems").then((s) => (systems = s ?? [])).catch(() => {});
  });

  async function importFiles() {
    failure = "";
    const files = await pickFiles({
      multiple: true,
      directory: false,
      title: "DAT files (XML, or a zip of them)",
      filters: [{ name: "DATs", extensions: ["dat", "xml", "zip"] }],
    });
    if (!files?.length) return;
    const r = await run(`Importing ${files.length} DAT file${files.length === 1 ? "" : "s"}…`, "import_dat_files", { files, system: system || null });
    if (r) {
      const fresh = r.filter((d) => !d.unchanged);
      note = `${fresh.length} DAT${fresh.length === 1 ? "" : "s"} imported (${fresh.reduce((n, d) => n + d.games, 0)} games), ${r.length - fresh.length} unchanged already.`;
      await reload();
    }
  }

  async function downloadOne() {
    const key = system;
    const name = systems.find((s) => s.key === key)?.name ?? key;
    const r = await run(`Downloading the DATs of ${name}…`, "download_dats", { extras, systems: [key] });
    if (r) {
      note = `${name}: ${r.imported} imported, ${r.unchanged} unchanged` + (r.errors.length ? `; errors: ${r.errors.join("; ")}` : "");
      await reload();
    }
  }

  async function check() {
    failure = "";
    checking = true;
    try {
      status = await invoke("nointro_status");
    } catch (e) {
      status = null;
      failure = String(e);
    }
    checking = false;
  }

  const labels = { missing: "not imported", outdated: "outdated", mirror: "from libretro's mirror", current: "current" };
  const shown = $derived((status ?? []).filter((s) => s.state !== "current"));
</script>

<details bind:open={opened} class="adv">
  <summary>Advanced: DAT files, one platform, No-Intro's list</summary>
  <section class="setting">
    <label class="flabel" for="da-system">Platform</label>
    <select id="da-system" bind:value={system}>
      <option value="">Not given</option>
      {#each systems as s}<option value={s.key}>{s.name}</option>{/each}
    </select>
    <p class="hint">For the two buttons below: the platform a DAT file is for, if it doesn't say; or whose DATs to download.</p>
    <div class="row">
      <button onclick={importFiles} disabled={!!busy}>Import DAT files…</button>
      <button onclick={downloadOne} disabled={!!busy || !system}>Download this platform's DATs</button>
    </div>
  </section>
  <section class="setting">
    <span class="flabel">No-Intro's DATs</span>
    <p class="hint">Asks No-Intro which DATs exist for the platforms romforge knows, and compares them with yours. It downloads nothing: Download DATs refreshes the mirror's, the others come from No-Intro's download page.</p>
    <button onclick={check} disabled={!!busy || checking}>{checking ? "Asking…" : "Check for newer DATs"}</button>
    {#if failure}<p class="err" role="alert">{failure}</p>{/if}
    {#if status}
      {#if shown.length}
        <ul>
          {#each shown as s}
            <li><span>{s.name}</span><span class="hint">{labels[s.state] ?? s.state}{s.imported ? ` · have ${s.imported}` : ""} · latest {s.latest}</span></li>
          {/each}
        </ul>
      {:else}
        <p class="hint">Every DAT you could have is current.</p>
      {/if}
    {/if}
  </section>
</details>

<style>
  .adv { margin: 0.8rem 0; }
  .row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  ul { list-style: none; padding: 0; margin: 0.4rem 0 0; max-height: 14rem; overflow-y: auto; }
  li { display: flex; gap: 0.6rem; justify-content: space-between; padding: 0.2rem 0; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
</style>
