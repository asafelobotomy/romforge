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

  let busy = $state("");
  let note = $state("");
  let scanDir = $state("");

  let from = $state("");
  let dest = $state("");
  let oneG1R = $state(false);
  let plan = $state(null);

  let query = $state("");
  let found = $state([]);

  let confirmUndo = $state(false);

  function show(t) {
    tab = t;
    error = "";
    note = "";
    confirmUndo = false;
  }

  async function run(label, cmd, args) {
    busy = label;
    note = "";
    const r = await call(cmd, args);
    busy = "";
    return r;
  }

  async function scan() {
    const r = await run("Scanning…", "scan", { dir: scanDir });
    if (r) note = `Scanned ${r.hashed} entries in ${r.root}; skipped ${r.skipped} non-ROM files` +
      (r.errors.length ? `; ${r.errors.length} errors` : "") +
      (r.sheet_problems.length ? `; ${r.sheet_problems.length} cue/gdi problems` : "");
    await loadAll();
  }

  async function downloadDats() {
    const r = await run("Downloading DATs… (a few minutes the first time)", "download_dats", { extras: false });
    if (r) note = `DATs: ${r.imported} imported, ${r.unchanged} unchanged` + (r.errors.length ? `; errors: ${r.errors.join("; ")}` : "");
    await loadAll();
  }

  async function planOrganize() {
    plan = await run("Planning…", "organize_plan", { from, dest, oneG1r: oneG1R });
  }

  async function applyOrganize() {
    const r = await run("Moving files…", "organize_apply", { from, dest, oneG1r: oneG1R });
    if (r) note = `Batch #${r.batch_id}: moved ${r.moved}, failed ${r.failed.length}. Undo moves them back.`;
    plan = null;
    await loadAll();
  }

  async function undoOrganize() {
    const r = await run("Moving files back…", "organize_undo", {});
    if (r) note = `Batch #${r.batch_id}: restored ${r.restored}, failed ${r.failed.length}`;
    plan = null;
    await loadAll();
  }

  async function enrich() {
    const r = await run("Fetching metadata and box art…", "enrich", {});
    if (r) note = `${r.games} games: ${r.games_with_metadata} with metadata; ${r.art_saved} images saved, ${r.art_cached} cached`;
    if (query) await search();
  }

  async function search() {
    found = (await call("game_info", { query })) ?? [];
  }

  async function play(game) {
    const r = await run(`Starting ${game}…`, "launch", { query: game });
    if (r) note = r;
  }

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
    <button class:active={tab === "library"} onclick={() => show("library")}>Library</button>
    <button class:active={tab === "dats"} onclick={() => show("dats")}>DATs</button>
    <button class:active={tab === "organize"} onclick={() => show("organize")}>Organize</button>
    <button class:active={tab === "games"} onclick={() => show("games")}>Games</button>
  </nav>
  <span class="counts">
    {#each summary.files_by_status as [s, n]}
      <span class="badge {s}">{n} {s.replace("_", " ")}</span>
    {/each}
  </span>
  <button onclick={loadAll} title="Reload from the database">Reload</button>
</header>

<!-- One fixed-height line, so the controls below never move under the cursor. -->
<p class="status" class:error={!!error && !busy} title={error || busy || note}>{busy || error || note}</p>

{#if tab === "library"}
  <section class="filters">
    <input class="wide" placeholder="Folder to scan, e.g. /home/me/roms" bind:value={scanDir} />
    <button onclick={scan} disabled={!scanDir || busy}>Scan</button>
  </section>
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
      Nothing here yet. Download DATs on the DATs tab, then scan a folder above
      (or use the CLI: <code>romforge-cli dat download --all && romforge-cli scan ~/roms</code>).
    </p>
  {:else}
    <table>
      <thead>
        <tr><th>Status</th><th>Game</th><th>System</th><th>File</th><th>Header</th><th class="num">Size</th><th></th></tr>
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
            <td>{#if f.status === "matched" && f.games.length === 1}<button onclick={() => play(f.games[0])}>Play</button>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
{:else if tab === "organize"}
  <section class="filters">
    <input class="wide" placeholder="Organize files in, e.g. /home/me/roms" bind:value={from} />
    <input class="wide" placeholder="Into, e.g. /home/me/roms-sorted" bind:value={dest} />
    <label><input type="checkbox" bind:checked={oneG1R} /> 1G1R (one version per title)</label>
    <button onclick={planOrganize} disabled={!from || !dest || busy}>Plan</button>
    <button onclick={applyOrganize} disabled={!plan || !plan.moves.length || busy}>Move {plan ? plan.moves.length : ""} files</button>
    {#if confirmUndo}
      <span>Move the last batch back?</span>
      <button onclick={() => { confirmUndo = false; undoOrganize(); }} disabled={busy}>Yes, move back</button>
      <button onclick={() => (confirmUndo = false)}>Cancel</button>
    {:else}
      <button onclick={() => (confirmUndo = true)} disabled={busy}>Undo last batch…</button>
    {/if}
  </section>
  {#if plan}
    <p class="meta pad">Nothing has moved yet. {plan.moves.length} files would move; {plan.skipped.length} stay where they are.</p>
    <table>
      <thead><tr><th>From</th><th>To</th></tr></thead>
      <tbody>
        {#each plan.moves as m}
          <tr><td class="path"><bdi>{m.from}</bdi></td><td class="path"><bdi>{m.to}</bdi></td></tr>
        {/each}
        {#each plan.skipped as s}
          <tr class="stay"><td class="path"><bdi>{s.path}</bdi></td><td>stays: {s.reason}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
{:else if tab === "games"}
  <section class="filters">
    <input class="wide" placeholder="Game name" bind:value={query} onkeydown={(e) => e.key === "Enter" && search()} />
    <button onclick={search} disabled={!query}>Search</button>
    <button onclick={enrich} disabled={busy} title="From libretro; saved under ~/.cache/romforge">Fetch metadata and art</button>
  </section>
  {#if query && found.length === 0}
    <p class="empty">No metadata yet for that name. Scan, then fetch metadata and art.</p>
  {/if}
  <div class="cards">
    {#each found as g}
      <article class="card">
        {#if g.art.boxart}<img src={g.art.boxart} alt="Box art of {g.title}" />{/if}
        <div>
          <h2>{g.title}</h2>
          <p class="meta">{g.system}</p>
          <dl>
            {#each Object.entries(g.fields).filter(([k]) => k !== "description") as [k, v]}
              <dt>{k}</dt><dd>{v}</dd>
            {/each}
          </dl>
          {#if g.fields.description}<p>{g.fields.description}</p>{/if}
          <button onclick={() => play(g.title)}>Play</button>
        </div>
      </article>
    {/each}
  </div>
{:else}
  <section class="filters">
    <button onclick={downloadDats} disabled={busy}>Download DATs for all systems</button>
  </section>
  <div class="dats">
    <div class="datpane">
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
    </div>
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
                <td><span class="badge {g.have ? 'matched' : 'missing'}">{g.have ? "have" : "missing"}</span>{#if g.cue_differs}<span class="badge" title="All tracks match; the cue sheet differs from the DAT's">cue differs</span>{/if}</td>
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
  .dats { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2fr); gap: 1rem; align-items: start; }
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
  .status { margin: 0.25rem 0; padding: 0 1rem; color: #3b6ea8; min-height: 1.4em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .status.error { color: #c33; }
  .datpane { overflow-x: auto; min-width: 0; }
  .datpane table { width: calc(100% - 1rem); margin-right: 0; }
  .pad { padding: 0 1rem; }
  input.wide { flex: 1; min-width: 16rem; font: inherit; padding: 0.25rem 0.5rem; border: 1px solid #8886; border-radius: 4px; background: transparent; color: inherit; }
  button:disabled { opacity: 0.5; cursor: default; }
  tr.stay td { color: #888; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(28rem, 1fr)); gap: 1rem; padding: 0 1rem 1rem; }
  .card { display: flex; gap: 1rem; border: 1px solid #8884; border-radius: 6px; padding: 0.75rem; }
  .card img { width: 10rem; height: auto; object-fit: contain; align-self: start; }
  .card dl { display: grid; grid-template-columns: auto 1fr; gap: 0 0.75rem; margin: 0.5rem 0; font-size: 0.85rem; }
  .card dt { color: #888; }
  .card dd { margin: 0; }
</style>
