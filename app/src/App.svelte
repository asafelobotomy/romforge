<script>
  import { invoke } from "@tauri-apps/api/core";

  let tab = $state("library");
  let error = $state("");

  let systems = $state([]);
  let systemId = $state("");
  let status = $state("");
  let files = $state([]);

  let summary = $state({ files_by_status: [], dats: [] });
  let datId = $state(null);
  let games = $state([]);
  let gameFilter = $state("all");

  const statuses = ["matched", "candidate", "unknown", "wrong_system", "duplicate"];

  async function call(cmd, args) {
    try {
      error = "";
      return await invoke(cmd, args);
    } catch (e) {
      error = String(e);
      return null;
    }
  }

  async function loadLibrary() {
    files =
      (await call("library", {
        systemId: systemId === "" ? null : Number(systemId),
        status: status === "" ? null : status,
      })) ?? [];
  }

  async function loadAll() {
    systems = (await call("systems")) ?? [];
    summary = (await call("summary")) ?? { files_by_status: [], dats: [] };
    await loadLibrary();
    if (datId !== null) await openDat(datId);
  }

  async function openDat(id) {
    datId = id;
    games = (await call("dat_games", { datId: id })) ?? [];
  }

  function shown(f) {
    return f.member ? `${f.path}#${f.member}` : f.path;
  }

  function size(n) {
    return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MiB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`;
  }

  let visibleGames = $derived(
    games.filter((g) => gameFilter === "all" || (gameFilter === "have") === g.have),
  );
  let currentDat = $derived(summary.dats.find((d) => d.dat_id === datId));

  $effect(() => {
    systemId;
    status;
    loadLibrary();
  });

  loadAll();
</script>

<header>
  <h1>romforge</h1>
  <nav>
    <button class:active={tab === "library"} onclick={() => (tab = "library")}>Library</button>
    <button class:active={tab === "dats"} onclick={() => (tab = "dats")}>DATs</button>
  </nav>
  <span class="counts">
    {#each summary.files_by_status as [s, n]}
      <span class="badge {s}">{n} {s.replace("_", " ")}</span>
    {/each}
  </span>
  <button onclick={loadAll} title="Reload from the database">Reload</button>
</header>

{#if error}<p class="error">{error}</p>{/if}

{#if tab === "library"}
  <section class="filters">
    <label>
      System
      <select bind:value={systemId}>
        <option value="">All systems</option>
        {#each systems as s}
          <option value={String(s.system_id)}>{s.name}</option>
        {/each}
      </select>
    </label>
    <label>
      Status
      <select bind:value={status}>
        <option value="">Any status</option>
        {#each statuses as s}<option value={s}>{s.replace("_", " ")}</option>{/each}
      </select>
    </label>
    <span>{files.length} files</span>
  </section>
  {#if files.length === 0}
    <p class="empty">
      Nothing here yet. Import DATs and scan a folder with the CLI:
      <code>romforge-cli dat import my.dat && romforge-cli scan ~/roms</code>
    </p>
  {:else}
    <table>
      <thead>
        <tr><th>Status</th><th>Game</th><th>System</th><th>File</th><th>Header</th><th class="num">Size</th></tr>
      </thead>
      <tbody>
        {#each files as f (f.file_id)}
          <tr>
            <td><span class="badge {f.status}">{f.status.replace("_", " ")}</span></td>
            <td>{f.games.join(" | ") || "—"}</td>
            <td>{f.systems.join(", ") || "—"}</td>
            <td class="path" title={shown(f)}><bdi>{shown(f)}</bdi></td>
            <td>{f.header_kind ?? ""}</td>
            <td class="num">{size(f.size)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
{:else}
  <div class="dats">
    <table class="datlist">
      <thead><tr><th>DAT</th><th>System</th><th class="num">Have</th><th class="num">Missing</th></tr></thead>
      <tbody>
        {#each summary.dats as d (d.dat_id)}
          <tr class:selected={d.dat_id === datId} onclick={() => openDat(d.dat_id)}>
            <td>{d.name}{d.version ? ` · v${d.version}` : ""}</td>
            <td>{d.system ?? "—"}</td>
            <td class="num">{d.have} / {d.games}</td>
            <td class="num">{d.missing}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if currentDat}
      <section class="games">
        <h2>{currentDat.name}</h2>
        <p class="meta">
          From {currentDat.source_path}, imported {currentDat.imported_at}
          {#if currentDat.header_hint}· header {currentDat.header_hint}{/if}
        </p>
        <div class="filters">
          {#each ["all", "have", "missing"] as v}
            <label><input type="radio" bind:group={gameFilter} value={v} /> {v}</label>
          {/each}
        </div>
        <table>
          <thead><tr><th></th><th>Game</th><th class="num">ROMs</th><th>Files</th></tr></thead>
          <tbody>
            {#each visibleGames as g (g.game_id)}
              <tr>
                <td><span class="badge {g.have ? 'matched' : 'missing'}">{g.have ? "have" : "missing"}</span></td>
                <td>{g.name}</td>
                <td class="num">{g.roms_have}/{g.roms_total}</td>
                <td class="path" title={g.files.join("\n")}><bdi>{g.files.join("\n")}</bdi></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/if}
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    font: 14px/1.4 system-ui, sans-serif;
    color: #1d1f23;
    background: #f6f7f9;
  }
  @media (prefers-color-scheme: dark) {
    :global(body) { color: #e4e6ea; background: #17191c; }
    table, .datlist { background: #1f2226 !important; }
    th { background: #26292e !important; }
    tr.selected { background: #2e3a4a !important; }
  }
  header { display: flex; align-items: center; gap: 1rem; padding: 0.5rem 1rem; border-bottom: 1px solid #8884; }
  h1 { font-size: 1.1rem; margin: 0; }
  h2 { font-size: 1rem; margin: 0 0 0.25rem; }
  nav { display: flex; gap: 0.25rem; }
  button { font: inherit; padding: 0.25rem 0.75rem; border: 1px solid #8886; border-radius: 4px; background: transparent; color: inherit; cursor: pointer; }
  button.active { background: #3b6ea8; color: white; border-color: #3b6ea8; }
  .counts { display: flex; gap: 0.25rem; margin-left: auto; flex-wrap: wrap; }
  .filters { display: flex; gap: 1rem; align-items: center; padding: 0.5rem 1rem; }
  .error { color: #c33; padding: 0 1rem; }
  .empty, .meta { padding: 0 1rem; color: #888; }
  .meta { padding: 0; font-size: 0.85rem; }
  table { border-collapse: collapse; width: calc(100% - 2rem); margin: 0 1rem 1rem; background: white; }
  th, td { text-align: left; padding: 0.3rem 0.5rem; border-bottom: 1px solid #8883; vertical-align: top; }
  th { position: sticky; top: 0; background: #eef0f3; font-weight: 600; }
  .num { text-align: right; white-space: nowrap; }
  .path { font-family: ui-monospace, monospace; font-size: 0.8rem; white-space: pre; overflow: hidden; text-overflow: ellipsis; max-width: 40vw; direction: rtl; text-align: left; }
  /* rtl + bdi: long paths are cut at the start, so the file name stays visible */
  .dats { display: grid; grid-template-columns: minmax(20rem, 1fr) 2fr; align-items: start; }
  .datlist tr { cursor: pointer; }
  tr.selected { background: #dce8f5; }
  .games { padding: 0 1rem 0 0; }
  .games table, .games .filters { margin-left: 0; padding-left: 0; width: 100%; }
  .badge { display: inline-block; padding: 0 0.4rem; border-radius: 3px; font-size: 0.8rem; white-space: nowrap; background: #8883; }
  .badge.matched { background: #2e7d3233; color: #2e7d32; }
  .badge.candidate { background: #b2860033; color: #a07800; }
  .badge.wrong_system { background: #c6282833; color: #c62828; }
  .badge.duplicate { background: #5e35b133; color: #7e57c2; }
  .badge.missing { background: #c6282822; color: #c62828; }
</style>
