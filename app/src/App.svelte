<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";

  // Which screen: the library (default), DATs, or organize.
  let view = $state("library");
  let error = $state("");
  let busy = $state("");
  let note = $state("");

  let lib = $state({ platforms: [], rows: [] });
  let platform = $state("*"); // "*" all, "" unidentified, else system key
  let filter = $state("");
  let selected = $state(null); // row id
  let detail = $state(null);
  let tab = $state("details");
  let adding = $state(false);
  let addDir = $state("");

  // Settings panel: a working copy, saved on "Save".
  let settingsOpen = $state(false);
  let draft = $state({ source_dir: null, library_dir: null, keep_originals: false, staging_dir: null, keys: {} });
  let defaultStaging = $state("");
  let settingsError = $state("");

  async function openSettings() {
    settingsError = "";
    const s = await call("get_settings");
    if (!s) return;
    draft = s;
    defaultStaging = (await call("settings_info"))?.default_staging ?? "";
    settingsOpen = true;
  }

  async function browse(field) {
    const dir = await pickFolder({ directory: true, multiple: false, defaultPath: draft[field] ?? undefined });
    if (typeof dir === "string") draft[field] = dir;
  }

  async function saveSettings() {
    settingsError = "";
    try {
      draft = await invoke("save_settings", { settings: draft });
      settingsOpen = false;
      note = "Settings saved. Refresh scans the Source and Library folders.";
    } catch (e) {
      settingsError = String(e).replace(/^settings: /, "");
    }
  }

  let summary = $state({ files_by_status: [], dats: [] });
  let datId = $state(null);
  let datGames = $state([]);
  let gameFilter = $state("all");

  let from = $state("");
  let dest = $state("");
  let oneG1R = $state(false);
  let plan = $state(null);
  let confirmUndo = $state(false);

  async function call(cmd, args) {
    try {
      error = "";
      return await invoke(cmd, args);
    } catch (e) {
      error = String(e);
      return null;
    }
  }

  async function run(label, cmd, args) {
    busy = label;
    note = "";
    const r = await call(cmd, args);
    busy = "";
    return r;
  }

  async function load() {
    lib = (await call("library_view")) ?? { platforms: [], rows: [] };
    if (selected && !lib.rows.some((r) => r.id === selected)) selected = null;
    if (selected) await open(selected);
  }

  async function open(id) {
    selected = id;
    detail = await call("game_detail", { id });
  }

  function show(v) {
    view = v;
    error = "";
    note = "";
    confirmUndo = false;
    if (v === "dats") loadDats();
  }

  async function refresh() {
    const r = await run("Rescanning your folders…", "refresh", {});
    if (r) note = `Rescanned ${r.folders} folders, ${r.entries} entries` + (r.problems.length ? `; ${r.problems.length} problems: ${r.problems.join("; ")}` : "");
    await load();
  }

  async function addFolder() {
    const r = await run("Scanning…", "scan", { dir: addDir });
    if (r) {
      note = `Scanned ${r.hashed} entries in ${r.root}`;
      adding = false;
      addDir = "";
    }
    await load();
  }

  async function enrich() {
    const system = platform === "*" || platform === "" ? null : platform;
    const r = await run(`Fetching metadata and art${system ? ` for ${system}` : ""}…`, "enrich", { system });
    if (r) note = `${r.games} games: ${r.games_with_metadata} with metadata; ${r.art_saved} images saved, ${r.art_missing} not found, ${r.art_cached} already cached`;
    await load();
  }

  async function play() {
    const row = lib.rows.find((r) => r.id === selected);
    if (!row) return;
    const r = await run(`Starting ${row.title}…`, "launch", { query: row.title });
    if (r) note = r;
  }

  async function loadDats() {
    summary = (await call("summary")) ?? { files_by_status: [], dats: [] };
    if (datId !== null) await openDat(datId);
  }

  async function openDat(id) {
    datId = id;
    datGames = (await call("dat_games", { datId: id })) ?? [];
  }

  async function downloadDats() {
    const r = await run("Downloading DATs… (a few minutes the first time)", "download_dats", { extras: false });
    if (r) note = `DATs: ${r.imported} imported, ${r.unchanged} unchanged` + (r.errors.length ? `; errors: ${r.errors.join("; ")}` : "");
    await loadDats();
    await load();
  }

  async function planOrganize() {
    plan = await run("Planning…", "organize_plan", { from, dest, oneG1r: oneG1R });
  }

  async function applyOrganize() {
    const r = await run("Moving files…", "organize_apply", { from, dest, oneG1r: oneG1R });
    if (r) note = r.batch_id ? `Batch #${r.batch_id}: moved ${r.moved}, failed ${r.failed.length}. Undo moves them back.` : "Nothing to move";
    plan = null;
    await load();
  }

  async function undoOrganize() {
    confirmUndo = false;
    const r = await run("Moving files back…", "organize_undo", {});
    if (r) note = `Batch #${r.batch_id}: restored ${r.restored}, failed ${r.failed.length}`;
    plan = null;
    await load();
  }

  const statusLabel = {
    verified: "Verified",
    incomplete: "Incomplete",
    candidate: "Candidate",
    wrong_system: "Wrong system",
    duplicate: "Duplicate",
    unknown: "Unknown",
  };
  const artKinds = [
    ["boxart", "Box art"],
    ["title", "Title screen"],
    ["snap", "In-game"],
  ];
  // Art libretro doesn't have; filled by milestone 9 sources.
  const laterArt = ["Banner", "Hero", "3D box", "Logo", "Icon"];

  // 1991, 1991-08 or 1991-08-21.
  function released(f) {
    if (!f.releaseyear) return "";
    return [f.releaseyear, f.releasemonth, f.releaseday]
      .filter(Boolean)
      .map((v, i) => (i ? String(v).padStart(2, "0") : v))
      .join("-");
  }

  function size(n) {
    return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MiB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`;
  }

  let rows = $derived(
    lib.rows.filter(
      (r) =>
        (platform === "*" || (platform === "" ? r.system === null : r.system === platform)) &&
        (!filter || r.title.toLowerCase().includes(filter.toLowerCase())),
    ),
  );
  let row = $derived(lib.rows.find((r) => r.id === selected) ?? null);

  // Virtual list: row height in px must match .tr in the styles.
  const ROW = 32;
  let scroller = $state(null);
  let scrollTop = $state(0);
  let viewH = $state(600);
  let first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  let last = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewH) / ROW) + 10));

  // Up/Down move through the list when no text field has focus.
  function listKeys(e) {
    if (view !== "library" || settingsOpen || !["ArrowUp", "ArrowDown"].includes(e.key)) return;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
    e.preventDefault();
    const i = rows.findIndex((r) => r.id === selected);
    const j = Math.max(0, Math.min(rows.length - 1, e.key === "ArrowDown" ? i + 1 : i < 0 ? 0 : i - 1));
    if (!rows[j]) return;
    open(rows[j].id);
    if (scroller) {
      const top = j * ROW;
      const head = 28;
      if (top < scroller.scrollTop) scroller.scrollTop = top;
      else if (top + ROW + head > scroller.scrollTop + viewH) scroller.scrollTop = top + ROW + head - viewH;
    }
  }
  let f = $derived(detail?.fields ?? {});
  let visibleGames = $derived(datGames.filter((g) => gameFilter === "all" || (gameFilter === "have") === g.have));
  let currentDat = $derived(summary.dats.find((d) => d.dat_id === datId));

  load();
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && settingsOpen) settingsOpen = false;
    else listKeys(e);
  }}
/>

<div class="app">
  <header class="toolbar">
    <button class="tool" onclick={refresh} disabled={!!busy} title="Rescan every source folder">
      <span class="glyph">⟳</span>Refresh
    </button>
    <button class="tool" onclick={() => (adding = !adding)} disabled={!!busy} title="Scan a new folder">
      <span class="glyph">＋</span>Add folder
    </button>
    <button class="tool" disabled title="Identify games one by one: hash, DAT title, or your own pick (milestone 8)">
      <span class="glyph">⌕</span>Search
    </button>
    <button class="tool" onclick={enrich} disabled={!!busy} title="Fetch metadata and art for the games you have on this platform">
      <span class="glyph">✦</span>Enrich
    </button>
    <button class="tool" disabled title="Repack and rename with art and details inside (milestone 10)">
      <span class="glyph">✎</span>Cleanup and rename
    </button>
    <span class="spacer"></span>
    <button class="tool" onclick={openSettings} disabled={!!busy} title="Source and Library folders, cleanup options">
      <span class="glyph">⚙</span>Settings
    </button>
    <button class="tool" onclick={play} disabled={!row || row.status !== "verified" || !!busy} title="Start the selected game">
      <span class="glyph">▶</span>Play
    </button>
  </header>

  {#if settingsOpen}
    <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (settingsOpen = false)}>
      <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <h2 id="settings-title">Settings</h2>

        <section class="setting">
          <label for="source">Source</label>
          <p class="hint">Where your unprocessed ROMs are. Refresh scans it.</p>
          <div class="pick">
            <input id="source" bind:value={draft.source_dir} placeholder="Not set" />
            <button onclick={() => browse("source_dir")}>Browse…</button>
            {#if draft.source_dir}<button onclick={() => (draft.source_dir = null)}>Clear</button>{/if}
          </div>
        </section>

        <section class="setting">
          <label for="library">Library <span class="hint">(optional)</span></label>
          <p class="hint">Where Cleanup and rename puts processed games, sorted by platform. Refresh scans it too, and Play uses what's there.</p>
          <div class="pick">
            <input id="library" bind:value={draft.library_dir} placeholder="Not set" />
            <button onclick={() => browse("library_dir")}>Browse…</button>
            {#if draft.library_dir}<button onclick={() => { draft.library_dir = null; draft.keep_originals = false; }}>Clear</button>{/if}
          </div>
        </section>

        <section class="setting">
          <label class="toggle">
            <input type="checkbox" bind:checked={draft.keep_originals} disabled={!draft.library_dir} />
            Keep originals
          </label>
          <p class="hint">
            {#if draft.library_dir}
              {draft.keep_originals
                ? "Source files stay untouched. Cleanup copies each game into the staging folder, extracts, renames and repacks it there, then moves the result into the Library."
                : "Cleanup processes games from the Source and moves them into the Library; the originals go to the trash once the new file is verified."}
            {:else}
              Needs a Library folder for the processed copies.
            {/if}
          </p>
        </section>

        <section class="setting">
          <label for="staging">Staging folder</label>
          <p class="hint">Scratch space for cleanup. Must be outside the Source and Library.</p>
          <div class="pick">
            <input id="staging" bind:value={draft.staging_dir} placeholder={defaultStaging} />
            <button onclick={() => browse("staging_dir")}>Browse…</button>
            {#if draft.staging_dir}<button onclick={() => (draft.staging_dir = null)}>Use default</button>{/if}
          </div>
        </section>

        <section class="setting">
          <h3 class="sub">Online sources <span class="hint">(optional)</span></h3>
          <p class="hint">Free sources that need no account are always used first. These fill only what they leave empty.</p>
          <label for="sgdb">SteamGridDB API key</label>
          <input id="sgdb" type="password" autocomplete="off" bind:value={draft.keys.steamgriddb_key} placeholder="Not set" />
          <div class="two">
            <div><label for="igdb-id">IGDB client ID</label><input id="igdb-id" autocomplete="off" bind:value={draft.keys.igdb_client_id} placeholder="Twitch app" /></div>
            <div><label for="igdb-secret">IGDB client secret</label><input id="igdb-secret" type="password" autocomplete="off" bind:value={draft.keys.igdb_client_secret} /></div>
          </div>
          <div class="two">
            <div><label for="ss-dev">ScreenScraper developer ID</label><input id="ss-dev" autocomplete="off" bind:value={draft.keys.screenscraper_dev_id} /></div>
            <div><label for="ss-devpw">Developer password</label><input id="ss-devpw" type="password" autocomplete="off" bind:value={draft.keys.screenscraper_dev_password} /></div>
          </div>
          <div class="two">
            <div><label for="ss-user">ScreenScraper user</label><input id="ss-user" autocomplete="off" bind:value={draft.keys.screenscraper_user} placeholder="Optional" /></div>
            <div><label for="ss-pw">Password</label><input id="ss-pw" type="password" autocomplete="off" bind:value={draft.keys.screenscraper_password} /></div>
          </div>
          <p class="hint">Stored in romforge's database on this computer.</p>
        </section>

        <p class="hint">Cleanup and rename itself arrives in milestone 10; these folders are used by Refresh and Play now.</p>
        {#if settingsError}<p class="err" role="alert">{settingsError}</p>{/if}
        <div class="actions">
          <button onclick={() => (settingsOpen = false)}>Cancel</button>
          <button class="primary" onclick={saveSettings}>Save</button>
        </div>
      </div>
    </div>
  {/if}

  {#if adding}
    <div class="addbar">
      <input placeholder="/home/me/roms" bind:value={addDir} onkeydown={(e) => e.key === "Enter" && addDir && addFolder()} />
      <button onclick={addFolder} disabled={!addDir || !!busy}>Scan folder</button>
      <button onclick={() => (adding = false)}>Cancel</button>
    </div>
  {/if}

  <!-- One fixed-height line, so the controls below never move under the cursor. -->
  <p class="status" class:error={!!error && !busy} title={error || busy || note}>{busy || error || note}</p>

  <div class="body">
    <nav class="platforms" aria-label="Platforms">
      <button class="plat" class:active={view === "library" && platform === "*"} onclick={() => { show("library"); platform = "*"; }}>
        <span class="icon all">ALL</span><span class="pname">All games</span><span class="count">{lib.rows.length}</span>
      </button>
      {#each lib.platforms as p}
        <button
          class="plat"
          class:active={view === "library" && platform === (p.system ?? "")}
          onclick={() => { show("library"); platform = p.system ?? ""; }}
          title={p.name}
        >
          <span class="icon" data-maker={p.manufacturer ?? "none"}>{p.short}</span>
          <span class="pname">{p.name}</span>
          <span class="count">{p.games}</span>
        </button>
      {/each}
      <span class="spacer"></span>
      <button class="plat small" class:active={view === "dats"} onclick={() => show("dats")}>DATs</button>
      <button class="plat small" class:active={view === "organize"} onclick={() => show("organize")}>Organize</button>
    </nav>

    {#if view === "library"}
      <section class="list">
        <input class="search" placeholder="Filter by title" bind:value={filter} />
        {#if lib.rows.length === 0}
          <div class="empty">
            <h2>Add your first folder</h2>
            <p>Download DATs on the DATs screen, then add a folder of ROMs with the button above.</p>
          </div>
        {:else}
          <div class="table" bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
            <div class="thead">
              <span>Title</span>
              {#if platform === "*"}<span>Platform</span>{/if}
              <span class="ind" title="Identified">ID</span>
              <span class="ind" title="Metadata">Info</span>
              <span class="ind" title="Box art">Box</span>
              <span class="ind" title="Title screen">Title</span>
              <span class="ind" title="In-game screenshot">Snap</span>
            </div>
            <!-- Only the rows in view are drawn (fixed row height), so a list of
                 tens of thousands stays quick. -->
            <div style="height: {first * ROW}px"></div>
            {#each rows.slice(first, last) as r (r.id)}
              <button class="tr" class:sel={r.id === selected} onclick={() => open(r.id)}>
                <span class="title">{r.title}</span>
                {#if platform === "*"}<span class="sys">{r.system ?? "—"}</span>{/if}
                <span class="ind {r.status}" title={statusLabel[r.status]}>{r.status === "verified" ? "✓" : r.status === "unknown" ? "?" : "~"}</span>
                <span class="ind" class:yes={r.metadata}>{r.metadata ? "✓" : "·"}</span>
                {#each ["boxart", "title", "snap"] as k}
                  <span class="ind" class:yes={r.art.includes(k)}>{r.art.includes(k) ? "✓" : "·"}</span>
                {/each}
              </button>
            {/each}
            <div style="height: {(rows.length - last) * ROW}px"></div>
          </div>
          <p class="foot">{rows.length} of {lib.rows.length}</p>
        {/if}
      </section>

      <section class="detail">
        {#if !row || !detail}
          <div class="empty"><p>Pick a game to see its details.</p></div>
        {:else}
          <div class="dhead">
            <h1>{row.title}</h1>
            <p class="meta">
              {row.system_name ?? "Unidentified"} · <span class="badge {row.status}">{statusLabel[row.status]}</span>
              {#if row.cue_differs}<span class="badge" title="All tracks match; the cue sheet differs from the DAT's">cue differs</span>{/if}
            </p>
          </div>
          <div class="tabs" role="tablist">
            {#each [["details", "Details"], ["development", "Development"], ["series", "Series"], ["links", "Links"]] as [k, label]}
              <button role="tab" aria-selected={tab === k} class:active={tab === k} onclick={() => (tab = k)}>{label}</button>
            {/each}
          </div>

          {#if tab === "details"}
            <div class="hero">
              {#if detail.art.snap}<img src={detail.art.snap} alt="In-game screenshot of {row.title}" />{:else}<span>Hero art (milestone 9)</span>{/if}
            </div>
            <div class="overview">
              <div class="cover">
                {#if detail.art.boxart}<img src={detail.art.boxart} alt="Box art of {row.title}" />{:else}<span>No box art</span>{/if}
              </div>
              <dl class="facts">
                {#if f.releaseyear}<dt>Released</dt><dd>{released(f)}</dd>{/if}
                {#if f.genre}<dt>Genre</dt><dd>{f.genre}</dd>{/if}
                {#if f.users}<dt>Players</dt><dd>{f.users}</dd>{/if}
                {#if f.publisher}<dt>Publisher</dt><dd>{f.publisher}</dd>{/if}
                {#if f.esrb}<dt>Rating</dt><dd>{f.esrb}</dd>{/if}
                <dt>Files</dt><dd>{row.files} · {size(row.size)}</dd>
              </dl>
            </div>
            {#if f.description}<p class="desc">{f.description}</p>{/if}
            <div class="gallery">
              {#each artKinds as [k, label]}
                <figure class:missing={!detail.art[k]}>
                  {#if detail.art[k]}<img src={detail.art[k]} alt="{label} of {row.title}" />{:else}<span>—</span>{/if}
                  <figcaption>{label}</figcaption>
                </figure>
              {/each}
              {#each laterArt as label}
                <figure class="missing" title="Comes with the art sources of milestone 9">
                  <span>—</span>
                  <figcaption>{label}</figcaption>
                </figure>
              {/each}
            </div>
            <h3>Files</h3>
            <ul class="files">
              {#each detail.files as file}
                <li title={file.sha1}>
                  <span class="badge {file.status === 'matched' ? 'verified' : file.status}">{file.status.replace("_", " ")}</span>
                  <span class="fpath"><bdi>{file.path}</bdi></span>
                </li>
              {/each}
            </ul>
          {:else if tab === "development"}
            <dl class="facts wide">
              <dt>Developer</dt><dd>{f.developer ?? "—"}</dd>
              <dt>Publisher</dt><dd>{f.publisher ?? "—"}</dd>
              <dt>Released</dt><dd>{released(f) || "—"}</dd>
              {#if f.serial}<dt>Serial</dt><dd>{f.serial}</dd>{/if}
              {#if f.franchise}<dt>Franchise</dt><dd>{f.franchise}</dd>{/if}
              <dt>DATs</dt><dd>{detail.dats.join(", ") || "—"}</dd>
            </dl>
            <p class="hint">Country, director, composer and other credits come from Wikidata (milestone 9).</p>
          {:else if tab === "series"}
            {#if detail.series.length}
              <ol class="series">
                {#each detail.series as s}
                  <li class:current={s.current}><span class="year">{s.year ?? "—"}</span>{s.title}<span class="sys">{s.system}</span></li>
                {/each}
              </ol>
              <p class="hint">Games in the “{f.franchise}” franchise that romforge has metadata for. The full series, in order, comes from Wikidata (milestone 9).</p>
            {:else}
              <p class="hint">No series known for this game yet.</p>
            {/if}
          {:else}
            {#if detail.links.length}
              <ul class="links">
                {#each detail.links as l}
                  <li><a href={l.url} onclick={(e) => { e.preventDefault(); openUrl(l.url).catch((err) => (error = String(err))); }}>{l.label}</a><span class="hint">search</span></li>
                {/each}
              </ul>
              <p class="hint">Search links for now; direct links to each site's page come with Wikidata IDs (milestone 9).</p>
            {:else}
              <p class="hint">Identify this file first to get links.</p>
            {/if}
          {/if}
        {/if}
      </section>
    {:else if view === "dats"}
      <section class="wide">
        <div class="bar"><button onclick={downloadDats} disabled={!!busy}>Download DATs for all systems</button></div>
        <div class="dats">
          <div class="datpane">
            <table>
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
            <div class="datpane">
              <h2>{currentDat.name}</h2>
              <p class="meta">From {currentDat.source_path}, imported {currentDat.imported_at}</p>
              <div class="bar">
                {#each ["all", "have", "missing"] as v}
                  <label><input type="radio" bind:group={gameFilter} value={v} /> {v}</label>
                {/each}
              </div>
              <table>
                <thead><tr><th></th><th>Game</th><th class="num">ROMs</th></tr></thead>
                <tbody>
                  {#each visibleGames as g (g.game_id)}
                    <tr>
                      <td><span class="badge {g.have ? 'verified' : 'unknown'}">{g.have ? "have" : "missing"}</span></td>
                      <td>{g.name}</td>
                      <td class="num">{g.roms_have}/{g.roms_total}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>
      </section>
    {:else}
      <section class="wide">
        <div class="bar">
          <input placeholder="Organize files in, /home/me/roms" bind:value={from} />
          <input placeholder="Into, /home/me/roms-sorted" bind:value={dest} />
          <label><input type="checkbox" bind:checked={oneG1R} /> 1G1R</label>
          <button onclick={planOrganize} disabled={!from || !dest || !!busy}>Plan</button>
          <button onclick={applyOrganize} disabled={!plan || !plan.moves.length || !!busy}>Move {plan ? plan.moves.length : ""} files</button>
          {#if confirmUndo}
            <span>Move the last batch back?</span>
            <button onclick={undoOrganize} disabled={!!busy}>Yes, move back</button>
            <button onclick={() => (confirmUndo = false)}>Cancel</button>
          {:else}
            <button onclick={() => (confirmUndo = true)} disabled={!!busy}>Undo last batch…</button>
          {/if}
        </div>
        {#if plan}
          <p class="meta">Nothing has moved yet. {plan.moves.length} files would move; {plan.skipped.length} stay where they are.</p>
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
      </section>
    {/if}
  </div>
</div>

<style>
  :global(:root) {
    --bg: #f4f5f7;
    --panel: #ffffff;
    --line: #8883;
    --text: #1d1f23;
    --muted: #6b7078;
    --accent: #3b6ea8;
    --accent-bg: #dce8f5;
    --ok: #2e7d32;
    --warn: #a07800;
    --bad: #c62828;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #17191c;
      --panel: #1f2226;
      --line: #fff2;
      --text: #e4e6ea;
      --muted: #969ba3;
      --accent: #6aa0dc;
      --accent-bg: #2a3a4e;
      --ok: #6cc070;
      --warn: #d8b24a;
      --bad: #ef6b6b;
    }
  }
  :global(body) { margin: 0; font: 14px/1.4 system-ui, sans-serif; color: var(--text); background: var(--bg); }
  .app { display: flex; flex-direction: column; height: 100vh; }
  button { font: inherit; color: inherit; background: transparent; border: 1px solid var(--line); border-radius: 6px; padding: 0.25rem 0.75rem; cursor: pointer; }
  button:disabled { opacity: 0.45; cursor: default; }
  input { font: inherit; color: inherit; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 0.3rem 0.5rem; }
  h1 { font-size: 1.25rem; margin: 0; font-weight: 600; }
  h2 { font-size: 1rem; margin: 0 0 0.25rem; }
  h3 { font-size: 0.9rem; margin: 1rem 0 0.25rem; color: var(--muted); font-weight: 600; }

  .toolbar { display: flex; gap: 0.25rem; padding: 0.4rem 0.75rem; border-bottom: 1px solid var(--line); background: var(--panel); }
  .tool { display: flex; flex-direction: column; align-items: center; border: none; padding: 0.2rem 0.6rem; font-size: 0.8rem; gap: 0.1rem; }
  .tool:hover:not(:disabled) { background: var(--accent-bg); }
  .glyph { font-size: 1.25rem; line-height: 1.2rem; }
  .spacer { flex: 1; }
  .addbar { display: flex; gap: 0.5rem; padding: 0.4rem 0.75rem; border-bottom: 1px solid var(--line); }
  .addbar input { flex: 1; }
  .status { margin: 0; padding: 0.2rem 0.75rem; min-height: 1.4em; color: var(--accent); font-size: 0.85rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .status.error { color: var(--bad); }

  .body { flex: 1; display: grid; grid-template-columns: 6.5rem minmax(0, 1.2fr) minmax(0, 1fr); min-height: 0; }
  .platforms { display: flex; flex-direction: column; gap: 0.15rem; padding: 0.4rem; border-right: 1px solid var(--line); overflow-y: auto; }
  .plat { display: flex; flex-direction: column; align-items: center; border: none; padding: 0.4rem 0.2rem; border-radius: 6px; gap: 0.15rem; }
  .plat:hover { background: var(--accent-bg); }
  .plat.active { background: var(--accent); color: white; }
  .plat.small { font-size: 0.8rem; padding: 0.3rem; }
  .icon { width: 3rem; height: 2.2rem; border-radius: 6px; display: grid; place-items: center; font-size: 0.7rem; font-weight: 700; letter-spacing: 0.02em; color: white; background: #6b7078; }
  .icon[data-maker="Nintendo"] { background: #c62828; }
  .icon[data-maker="Sega"] { background: #1e5bb8; }
  .icon[data-maker="Sony"] { background: #3d3d48; }
  .icon[data-maker="Atari"] { background: #b25d00; }
  .icon[data-maker="NEC"] { background: #6a3fb5; }
  .icon[data-maker="SNK"] { background: #2b7a5a; }
  .icon[data-maker="Microsoft"] { background: #3b8a2e; }
  .icon[data-maker="Commodore"] { background: #4a6a8a; }
  .icon.all { background: var(--accent); }
  .pname { font-size: 0.72rem; text-align: center; line-height: 1.15; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .count { font-size: 0.7rem; color: var(--muted); }
  .plat.active .count { color: #fffc; }

  .list { display: flex; flex-direction: column; min-height: 0; border-right: 1px solid var(--line); }
  .search { margin: 0.5rem; }
  .table { flex: 1; overflow-y: auto; }
  .thead, .tr { display: grid; grid-template-columns: minmax(0, 1fr) auto repeat(5, 2.4rem); align-items: center; gap: 0.25rem; padding: 0.3rem 0.6rem; }
  .thead { position: sticky; top: 0; background: var(--panel); font-size: 0.75rem; color: var(--muted); border-bottom: 1px solid var(--line); }
  .tr { width: 100%; height: 32px; box-sizing: border-box; padding-top: 0; padding-bottom: 0; border: none; border-radius: 0; border-bottom: 1px solid var(--line); text-align: left; }
  .tr:hover { background: var(--accent-bg); }
  .tr.sel { background: var(--accent-bg); box-shadow: inset 3px 0 var(--accent); }
  .title { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sys { color: var(--muted); font-size: 0.8rem; white-space: nowrap; }
  .ind { text-align: center; color: var(--muted); }
  .ind.yes, .ind.verified { color: var(--ok); }
  .ind.incomplete, .ind.candidate, .ind.duplicate { color: var(--warn); }
  .ind.unknown, .ind.wrong_system { color: var(--bad); }
  .foot { margin: 0; padding: 0.3rem 0.6rem; font-size: 0.8rem; color: var(--muted); border-top: 1px solid var(--line); }

  .detail { overflow-y: auto; padding: 0.75rem 1rem 1.5rem; }
  .dhead .meta { margin: 0.2rem 0 0.5rem; color: var(--muted); }
  .tabs { display: flex; gap: 0.25rem; border-bottom: 1px solid var(--line); margin-bottom: 0.75rem; }
  .tabs button { border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 0.35rem 0.7rem; color: var(--muted); }
  .tabs button.active { color: var(--text); border-bottom-color: var(--accent); }
  .hero { height: 9rem; border-radius: 8px; background: var(--panel); border: 1px solid var(--line); overflow: hidden; display: grid; place-items: center; color: var(--muted); font-size: 0.85rem; }
  .hero img { width: 100%; height: 100%; object-fit: cover; }
  .overview { display: flex; gap: 1rem; margin-top: 0.75rem; }
  .cover { width: 8rem; flex: none; min-height: 6rem; border-radius: 6px; background: var(--panel); border: 1px solid var(--line); display: grid; place-items: center; color: var(--muted); font-size: 0.8rem; overflow: hidden; }
  .cover img { width: 100%; display: block; }
  .facts { display: grid; grid-template-columns: auto 1fr; gap: 0.2rem 0.75rem; margin: 0; align-content: start; font-size: 0.9rem; }
  .facts.wide { max-width: 40rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .desc { line-height: 1.55; }
  .gallery { display: grid; grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr)); gap: 0.5rem; margin-top: 0.75rem; }
  figure { margin: 0; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; background: var(--panel); }
  figure img { width: 100%; height: 5.5rem; object-fit: contain; display: block; background: #0002; }
  figure.missing span { height: 5.5rem; display: grid; place-items: center; color: var(--muted); }
  figcaption { font-size: 0.72rem; color: var(--muted); padding: 0.15rem 0.35rem; }
  .files { list-style: none; padding: 0; margin: 0; font-size: 0.8rem; }
  .files li { display: flex; gap: 0.5rem; align-items: baseline; padding: 0.15rem 0; overflow: hidden; }
  /* rtl on the box and <bdi> inside: long paths are cut at the start, the
     file name stays visible, and the text itself keeps its order */
  .fpath { font-family: ui-monospace, monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; min-width: 0; }
  .series { padding-left: 0; list-style: none; }
  .series li { display: flex; gap: 0.75rem; padding: 0.3rem 0; border-bottom: 1px solid var(--line); }
  .series li.current { font-weight: 600; }
  .series .year { color: var(--muted); width: 3rem; }
  .series .sys { margin-left: auto; }
  .links { list-style: none; padding: 0; }
  .links li { display: flex; gap: 0.5rem; padding: 0.3rem 0; }
  .links a { color: var(--accent); }
  .hint { color: var(--muted); font-size: 0.85rem; }
  .empty { padding: 2rem 1rem; color: var(--muted); text-align: center; }
  .empty h2 { color: var(--text); }

  .badge { display: inline-block; padding: 0 0.4rem; border-radius: 3px; font-size: 0.75rem; white-space: nowrap; background: var(--line); }
  .badge.verified { color: var(--ok); }
  .badge.incomplete, .badge.candidate, .badge.duplicate { color: var(--warn); }
  .badge.unknown, .badge.wrong_system { color: var(--bad); }

  .wide { grid-column: 2 / 4; overflow: auto; padding: 0 0.75rem 1rem; }
  .bar { display: flex; gap: 0.5rem; align-items: center; padding: 0.5rem 0; flex-wrap: wrap; }
  .bar input { flex: 1; min-width: 12rem; }
  .dats { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); gap: 1rem; align-items: start; }
  .datpane { overflow-x: auto; min-width: 0; }
  table { border-collapse: collapse; width: 100%; background: var(--panel); }
  th, td { text-align: left; padding: 0.3rem 0.5rem; border-bottom: 1px solid var(--line); vertical-align: top; font-size: 0.9rem; }
  th { position: sticky; top: 0; background: var(--panel); color: var(--muted); font-weight: 600; font-size: 0.8rem; }
  .num { text-align: right; white-space: nowrap; }
  tr.selected { background: var(--accent-bg); }
  .datpane tbody tr { cursor: pointer; }
  .path { font-family: ui-monospace, monospace; font-size: 0.8rem; white-space: pre; overflow: hidden; text-overflow: ellipsis; max-width: 40vw; direction: rtl; text-align: left; }
  tr.stay td { color: var(--muted); }
  .meta { color: var(--muted); font-size: 0.85rem; }

  .scrim { position: fixed; inset: 0; background: #0008; display: grid; place-items: center; z-index: 10; }
  .dialog { width: min(40rem, calc(100vw - 2rem)); max-height: calc(100vh - 2rem); overflow-y: auto; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 1.25rem 1.5rem; }
  .dialog h2 { font-size: 1.15rem; margin-bottom: 0.75rem; }
  .setting { padding: 0.6rem 0; border-top: 1px solid var(--line); }
  .setting > label { font-weight: 600; }
  .setting .hint { margin: 0.15rem 0 0.4rem; }
  .pick { display: flex; gap: 0.4rem; }
  .pick input { flex: 1; min-width: 0; }
  .toggle { display: flex; gap: 0.5rem; align-items: center; }
  .setting label { display: block; font-size: 0.85rem; margin-top: 0.4rem; }
  .setting > input { width: 100%; box-sizing: border-box; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem; }
  .two input { width: 100%; box-sizing: border-box; }
  h3.sub { color: var(--text); margin: 0; }
  .err { color: var(--bad); }
  .actions { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 0.75rem; }
  button.primary { background: var(--accent); border-color: var(--accent); color: white; }
</style>
