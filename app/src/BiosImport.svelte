<script>
  import { invoke } from "@tauri-apps/api/core";
  import { run } from "./jobs.svelte.js";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ask, open as pick } from "@tauri-apps/plugin-dialog";
  import { ui } from "./ui.svelte.js";

  // Dropping BIOS files on the BIOS page (milestone 20, step 3): a folder,
  // files, zips or 7z. The core matches each file by its contents and says
  // where it goes; nothing is written until "Import". `done` reloads the
  // page's report. `order`: the systems' names as the page lists them, so
  // the plan's groups follow the same order.
  let { active, busy, note = $bindable(""), error = $bindable(""), done, order = [], plan = $bindable(null) } = $props();

  let over = $state(false);
  let paths = $state([]);
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
    paths = p;
    also = [];
    // A job in the queue (footer), each file read a step; its error goes
    // to the status line.
    plan = await run("Reading BIOS files…", "firmware_import_plan", { paths: p });
    working = false;
  }

  // A BIOS file's row sent its files here ("Import to BIOS").
  $effect(() => {
    if (!active || !ui.biosPaths) return;
    const p = ui.biosPaths;
    ui.biosPaths = null;
    look(p);
  });

  // The BIOS files your scanned folders hold (shown as "BIOS file" in
  // Unidentified), as an import plan.
  async function inLibrary() {
    const found = await invoke("firmware_in_library", { fileId: null }).catch((e) => {
      error = String(e);
      return null;
    });
    if (found && !found.length) note = "No BIOS files among your scanned games (with RetroBIOS's list downloaded).";
    else if (found) look(found);
  }

  // The usual BIOS places that exist here, shown as an import plan.
  async function find() {
    const found = await invoke("firmware_found").catch((e) => {
      error = String(e);
      return null;
    });
    if (found && !found.length) note = "No BIOS folders in their usual places on this computer.";
    else if (found) look(found);
  }

  async function choose(directory) {
    const p = await pick({ multiple: !directory, directory, title: directory ? "A folder of BIOS files" : "BIOS files or archives" });
    if (p) look(Array.isArray(p) ? p : [p]);
  }

  // Importing and undoing are jobs in the queue (footer); their errors go
  // to the status line.
  async function apply() {
    working = true;
    const r = await run("Importing BIOS files…", "firmware_import", { paths, also });
    if (r) {
      note =
        `BIOS import: ${r.written} file${r.written === 1 ? "" : "s"} written, ${r.linked} more place${r.linked === 1 ? "" : "s"} linked` +
        (r.moved ? `, ${r.moved} original${r.moved === 1 ? "" : "s"} moved to the trash` : "") +
        ". Undo takes them away.";
      plan = null;
      await done();
    }
    working = false;
  }

  async function undo() {
    const ok = await ask("Take away what the last BIOS import wrote?", { title: "Undo BIOS import", kind: "warning" });
    if (!ok) return;
    working = true;
    const n = await run("Undoing the last BIOS import…", "firmware_undo", {}, { key: "firmware_undo" });
    if (n != null) {
      note = `Undone: ${n} file${n === 1 ? "" : "s"} taken away.`;
      await done();
    }
    working = false;
  }

  const accepted = $derived(plan?.files.filter((f) => f.matched === "accepted") ?? []);
  const nameOnly = $derived(plan?.files.filter((f) => f.matched === "name_only") ?? []);
  const unknown = $derived(plan?.files.filter((f) => f.matched === "unknown") ?? []);
  // Each place once: two ticked name-only files for one place fill it once
  // (the first is written), as the core's `ImportPlan::fills` counts.
  const fills = $derived(plan ? plan.new_places + new Set(nameOnly.filter((f) => also.includes(f.shown)).flatMap((f) => f.targets.filter((t) => t.state === "new").map((t) => t.place))).size : 0);
  // The recognised and name-only files by system (a file several systems'
  // emulators load is under all of their names), in the page's order.
  const groups = $derived.by(() => {
    const m = new Map();
    for (const f of [...accepted, ...nameOnly]) {
      const name = f.systems.join(" / ") || "Other";
      if (!m.has(name)) m.set(name, { name, rank: rank(f.systems[0]), accepted: [], nameOnly: [] });
      m.get(name)[f.matched === "accepted" ? "accepted" : "nameOnly"].push(f);
    }
    return [...m.values()].sort((a, b) => a.rank - b.rank || a.name.localeCompare(b.name));
  });
  const rank = (system) => (order.includes(system) ? order.indexOf(system) : order.length);
  const tick = (f, on) => (also = on ? [...also, f.shown] : also.filter((s) => s !== f.shown));
  // "Select all" ticks only name-only files with an empty place to fill: one
  // whose places hold a file already, or that a recognised file fills, stays
  // unticked.
  const fillsAny = (f) => f.targets.some((t) => t.state === "new");
  const tickAll = (files, on) => {
    const shown = files.filter(fillsAny).map((f) => f.shown);
    also = on ? [...new Set([...also, ...shown])] : also.filter((s) => !shown.includes(s));
  };
  const ticked = (files) => {
    const open = files.filter(fillsAny);
    const n = open.filter((f) => also.includes(f.shown)).length;
    return { open: open.length, all: open.length > 0 && n === open.length, some: n > 0 && n < open.length };
  };
  const STATE = { new: "goes to", already_there: "already in", taken: "another file is in", claimed: "another dropped file goes to" };
  const rel = (p) => p.replace(/^.*\/Bios\//, "Bios/");
</script>

<section class="drop" class:over>
  <p>
    <strong>Drop BIOS files here</strong> — a folder, files, or zip / 7z archives. Each file is recognised by its contents, so
    names don't matter. Your files are copied; the originals stay where they are unless Settings says otherwise.
  </p>
  <div class="pick">
    <button onclick={() => choose(true)} disabled={!!busy || working}>Add folder…</button>
    <button onclick={() => choose(false)} disabled={!!busy || working}>Add files…</button>
    <button onclick={find} disabled={!!busy || working} title="Look in the usual places: RetroArch's system folder, EmuDeck's and RetroDECK's BIOS folders, the emulators' own">Find BIOS on this computer</button>
    <button onclick={inLibrary} disabled={!!busy || working} title="BIOS files among your scanned games (Unidentified shows them as BIOS files)">From your library</button>
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
    {#if nameOnly.length}
      {@const t = ticked(nameOnly)}
      <p class="hint">Files marked "by name only" match a file an emulator loads by name alone (the list gives no hash to check them by). Tick the ones to place anyway.</p>
      <label class="toggle all">
        <input type="checkbox" checked={t.all} indeterminate={t.some} disabled={!t.open} onchange={(e) => tickAll(nameOnly, e.currentTarget.checked)} />
        Select all by name only <span class="hint">({t.open} with an empty place)</span>
      </label>
    {/if}
    {#each groups as g (g.name)}
      <details class="group" open>
        <summary>
          <strong>{g.name}</strong>
          <span class="hint">· {g.accepted.length} recognised{#if g.nameOnly.length}, {g.nameOnly.length} by name only{/if}</span>
        </summary>
        {#each g.accepted as f}
          <div class="item">
            <span class="src" title={f.shown}>{f.name}</span>
            <ul>
              {#each f.targets as t}
                <li class={t.state}>{STATE[t.state]} {rel(t.place)} <span class="hint">· {t.used_by.join(", ")}</span></li>
              {/each}
            </ul>
          </div>
        {/each}
        {#if g.nameOnly.length > 1}
          {@const t = ticked(g.nameOnly)}
          <label class="toggle item">
            <input type="checkbox" checked={t.all} indeterminate={t.some} disabled={!t.open} onchange={(e) => tickAll(g.nameOnly, e.currentTarget.checked)} />
            <span class="hint">Select all {g.name} files by name only ({t.open})</span>
          </label>
        {/if}
        {#each g.nameOnly as f}
          <label class="toggle item">
            <input type="checkbox" checked={also.includes(f.shown)} onchange={(e) => tick(f, e.currentTarget.checked)} />
            <span class="src" title={f.shown}>{f.name}</span>
            <span class="hint">by name only {f.targets.some((t) => t.state === "new") ? "→ " + f.targets.filter((t) => t.state === "new").map((t) => rel(t.place)).join(", ") : "· its places are filled"}</span>
          </label>
        {/each}
      </details>
    {/each}
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
  .drop { border: 2px dashed var(--line); border-radius: var(--radius); padding: 0.6rem 0.8rem; margin: 0.5rem 0; }
  .drop.over { border-color: var(--accent); background: var(--accent-bg); }
  .drop p { margin: 0 0 0.4rem; }
  .plan { border: 1px solid var(--line); border-radius: var(--radius); padding: 0.6rem 0.8rem; margin-bottom: 0.75rem; }
  .plan h3 { font-size: 0.95rem; margin: 0 0 0.4rem; }
  .group { border-top: 1px solid var(--line); padding: 0.3rem 0; }
  .group summary { cursor: pointer; padding: 0.1rem 0; }
  .group > .item { display: block; margin-left: 1rem; }
  .item { padding: 0.2rem 0; }
  .all { display: block; margin: 0.2rem 0 0.4rem; }
  .src { font-weight: 600; }
  .item ul { margin: 0.1rem 0 0.2rem 1rem; padding: 0; list-style: none; font-size: 0.85rem; }
  .new { color: var(--ok); }
  .taken { color: var(--bad); }
  .already_there, .claimed { color: var(--muted); }
</style>
