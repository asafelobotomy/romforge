<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as pick } from "@tauri-apps/plugin-dialog";

  // Dropping BIOS files on the BIOS page (milestone 20, step 3): a folder,
  // files, zips or 7z. The core matches each file by its contents and says
  // where it goes; nothing is written until "Import". `done` reloads the
  // page's report.
  let { active, busy, note = $bindable(""), error = $bindable(""), done } = $props();

  let over = $state(false);
  let paths = $state([]);
  let plan = $state(null);
  let also = $state([]);
  let working = $state(false);

  // Listen for drops only while the page is shown.
  $effect(() => {
    if (!active) return;
    let unlisten = null;
    let gone = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const t = e.payload.type;
        if (t === "enter" || t === "over") over = true;
        else if (t === "leave") over = false;
        else if (t === "drop") {
          over = false;
          if (e.payload.paths?.length) look(e.payload.paths);
        }
      })
      .then((f) => (gone ? f() : (unlisten = f)));
    return () => {
      gone = true;
      unlisten?.();
    };
  });

  async function look(p) {
    if (busy || working) return;
    working = true;
    error = "";
    try {
      paths = p;
      also = [];
      plan = await invoke("firmware_import_plan", { paths: p });
    } catch (e) {
      plan = null;
      error = String(e).replace(/^(launch|settings): /, "");
    }
    working = false;
  }

  async function choose(directory) {
    const p = await pick({ multiple: !directory, directory, title: directory ? "A folder of BIOS files" : "BIOS files or archives" });
    if (p) look(Array.isArray(p) ? p : [p]);
  }

  async function apply() {
    working = true;
    try {
      const r = await invoke("firmware_import", { paths, also });
      note = `BIOS import: ${r.written} file${r.written === 1 ? "" : "s"} written, ${r.linked} more place${r.linked === 1 ? "" : "s"} linked. Undo takes them away.`;
      plan = null;
      await done();
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
    working = false;
  }

  async function undo() {
    working = true;
    try {
      const n = await invoke("firmware_undo");
      note = `Undone: ${n} file${n === 1 ? "" : "s"} taken away.`;
      await done();
    } catch (e) {
      error = String(e).replace(/^(launch|settings): /, "");
    }
    working = false;
  }

  const accepted = $derived(plan?.files.filter((f) => f.matched === "accepted") ?? []);
  const nameOnly = $derived(plan?.files.filter((f) => f.matched === "name_only") ?? []);
  const unknown = $derived(plan?.files.filter((f) => f.matched === "unknown") ?? []);
  const fills = $derived(plan ? plan.new_places + nameOnly.filter((f) => also.includes(f.shown)).reduce((n, f) => n + f.targets.filter((t) => t.state === "new").length, 0) : 0);
  const STATE = { new: "goes to", already_there: "already in", taken: "another file is in", claimed: "another dropped file goes to" };
  const rel = (p) => p.replace(/^.*\/Bios\//, "Bios/");
</script>

<section class="drop" class:over>
  <p>
    <strong>Drop BIOS files here</strong> — a folder, files, or zip / 7z archives. Each file is recognised by its contents, so
    names don't matter; your files are copied, never moved.
  </p>
  <div class="pick">
    <button onclick={() => choose(true)} disabled={!!busy || working}>Add folder…</button>
    <button onclick={() => choose(false)} disabled={!!busy || working}>Add files…</button>
    <span class="spacer"></span>
    <button onclick={undo} disabled={!!busy || working} title="Take away what the last import wrote">Undo last import</button>
  </div>
  {#if working}<p class="hint">Reading…</p>{/if}
</section>

{#if plan}
  <section class="plan">
    <h3>
      {accepted.length} recognised, {nameOnly.length} by name only, {unknown.length} not BIOS files
    </h3>
    {#each accepted as f}
      <div class="item">
        <span class="src" title={f.shown}>{f.name}</span>
        <ul>
          {#each f.targets as t}
            <li class={t.state}>{STATE[t.state]} {rel(t.place)} <span class="hint">· {t.used_by.join(", ")}</span></li>
          {/each}
        </ul>
      </div>
    {/each}
    {#if nameOnly.length}
      <p class="hint">These match a file an emulator loads by name only (the list gives no hash to check them by). Tick the ones to place anyway.</p>
      {#each nameOnly as f}
        <label class="toggle item">
          <input type="checkbox" checked={also.includes(f.shown)} onchange={(e) => (also = e.currentTarget.checked ? [...also, f.shown] : also.filter((s) => s !== f.shown))} />
          <span class="src" title={f.shown}>{f.name}</span>
          <span class="hint">→ {f.targets.map((t) => rel(t.place)).join(", ")}</span>
        </label>
      {/each}
    {/if}
    {#if unknown.length}
      <details><summary class="hint">{unknown.length} file{unknown.length === 1 ? "" : "s"} no emulator here loads</summary>
        <ul class="hint">{#each unknown as f}<li title={f.shown}>{f.shown}</li>{/each}</ul>
      </details>
    {/if}
    {#each plan.skipped as [what, why]}<p class="hint">Skipped {what}: {why}</p>{/each}
    <div class="actions">
      <button onclick={() => (plan = null)}>Cancel</button>
      <button class="primary" onclick={apply} disabled={!fills || working}>Import ({fills} place{fills === 1 ? "" : "s"})</button>
    </div>
  </section>
{/if}

<style>
  .drop { border: 2px dashed var(--line); border-radius: 8px; padding: 0.6rem 0.8rem; margin: 0.5rem 0; }
  .drop.over { border-color: var(--accent); background: var(--accent-bg); }
  .drop p { margin: 0 0 0.4rem; }
  .plan { border: 1px solid var(--line); border-radius: 8px; padding: 0.6rem 0.8rem; margin-bottom: 0.75rem; }
  .plan h3 { font-size: 0.95rem; margin: 0 0 0.4rem; }
  .item { padding: 0.2rem 0; }
  .src { font-weight: 600; }
  .item ul { margin: 0.1rem 0 0.2rem 1rem; padding: 0; list-style: none; font-size: 0.85rem; }
  .new { color: var(--ok); }
  .taken { color: var(--bad); }
  .already_there, .claimed { color: var(--muted); }
</style>
