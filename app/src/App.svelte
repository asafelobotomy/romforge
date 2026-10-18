<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
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
  let draft = $state({ source_dir: null, library_dir: null, keep_originals: false, staging_dir: null, keys: {}, show_empty_media: false });
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
      showEmpty = draft.show_empty_media;
      settingsOpen = false;
      note = "Settings saved. Refresh scans the Source and Library folders.";
    } catch (e) {
      settingsError = String(e).replace(/^settings: /, "");
    }
  }

  // Cleanup and rename dialog: a plan first, applied on confirmation.
  let cleanOpen = $state(false);
  let cleanOnly = $state(false); // just the selected game
  let cleanPlan = $state(null);
  let cleanBatches = $state([]);
  let cleanError = $state("");
  let confirmCleanUndo = $state(false);

  async function openCleanup() {
    cleanOnly = false;
    confirmCleanUndo = false;
    cleanOpen = true;
    await planCleanup();
  }

  async function planCleanup() {
    cleanError = "";
    try {
      cleanPlan = await invoke("cleanup_plan", { only: cleanOnly && selected ? [selected] : null });
    } catch (e) {
      cleanPlan = null;
      cleanError = String(e).replace(/^settings: /, "");
    }
    cleanBatches = ((await call("cleanup_batches")) ?? []).filter((b) => !b.undone_at);
  }

  async function applyCleanup() {
    cleanError = "";
    busy = `Repacking ${cleanPlan.items.length} games…`;
    try {
      const r = await invoke("cleanup_apply", { only: cleanOnly && selected ? [selected] : null });
      note = r.batch_id
        ? `Batch #${r.batch_id}: wrote ${r.done}, ${cleanPlan.keep_originals ? "originals kept" : `${r.trashed} originals to .romforge-trash`}` +
          (r.failed.length ? `; ${r.failed.length} failed: ${r.failed.map(([t, e]) => `${t}: ${e}`).join("; ")}` : "")
        : "Nothing was written" + (r.failed.length ? `: ${r.failed.map(([t, e]) => `${t}: ${e}`).join("; ")}` : "");
      cleanOpen = false;
    } catch (e) {
      cleanError = String(e);
    }
    busy = "";
    await load();
  }

  async function undoCleanup() {
    confirmCleanUndo = false;
    cleanError = "";
    busy = "Undoing the last cleanup…";
    try {
      const r = await invoke("cleanup_undo");
      note = `Batch #${r.batch_id}: removed ${r.removed} new files, restored ${r.restored} originals` + (r.failed.length ? `; failed: ${r.failed.join("; ")}` : "");
      cleanOpen = false;
    } catch (e) {
      cleanError = String(e);
    }
    busy = "";
    await load();
  }

  const base = (p) => p.split("/").pop();

  // Search dialog, after tinyMediaManager's: one queue item at a time, a
  // scraper and search text on top, results | picture | details, then what
  // to fetch once a result is taken.
  let searchOpen = $state(false);
  let queue = $state([]);
  let qi = $state(0);
  let qDetail = $state(null);
  let scraper = $state("dat");
  let sText = $state("");
  let sSystem = $state("");
  let results = $state([]);
  let picked = $state(-1);
  let preview = $state(null);
  let searching = $state(false);
  let ownTitle = $state("");
  let qError = $state("");
  let allSystems = $state([]);
  const fetchItems = [
    ["wikidata", "Wikidata details"],
    ["boxart", "Box art"],
    ["title", "Title screen"],
    ["snap", "Screenshot"],
    ["box3d", "3D box"],
    ["hero", "Hero"],
    ["logo", "Logo"],
    ["icon", "Icon"],
  ];
  let fetchOn = $state(["wikidata", "boxart", "title", "snap"]);
  let noOverwrite = $state(true);
  let searchSeq = 0;

  async function openSearch() {
    qError = "";
    queue = (await call("search_queue")) ?? [];
    if (!queue.length) {
      note = "Nothing needs identifying: every game is matched by hash or named by you, and Wikidata is settled.";
      return;
    }
    if (!allSystems.length) allSystems = (await call("all_systems")) ?? [];
    const start = queue.findIndex((q) => q.row_id === selected);
    qi = start >= 0 ? start : 0;
    searchOpen = true;
    await loadItem();
  }

  async function loadItem() {
    qError = "";
    const item = queue[qi];
    if (!item) return;
    scraper = item.need === "wikidata" ? "wikidata" : "dat";
    sSystem = item.system ?? "";
    sText = "";
    ownTitle = "";
    results = [];
    preview = null;
    searching = true;
    qDetail = await call("game_detail", { id: item.row_id });
    await doSearch();
  }

  async function doSearch() {
    const seq = ++searchSeq;
    qError = "";
    searching = true;
    results = [];
    picked = -1;
    preview = null;
    try {
      const r = await invoke("search_find", { rowId: queue[qi].row_id, scraper, text: sText || null, system: sSystem || null });
      if (seq !== searchSeq) return;
      results = r;
      if (r.length) await pick(0);
    } catch (e) {
      if (seq === searchSeq) qError = String(e);
    }
    if (seq === searchSeq) searching = false;
  }

  async function pick(i) {
    picked = i;
    preview = null;
    const found = results[i];
    const p = await call("search_preview", { rowId: queue[qi].row_id, found });
    if (results[picked] === found) preview = p;
  }

  async function step(d) {
    qi = Math.max(0, Math.min(queue.length - 1, qi + d));
    await loadItem();
  }

  // After an answer: drop the item and go on with the next one.
  async function answered(msg) {
    note = msg;
    queue = queue.filter((_, i) => i !== qi);
    await load();
    if (!queue.length) {
      searchOpen = false;
      note = msg + " That was the last one.";
      return;
    }
    qi = Math.min(qi, queue.length - 1);
    await loadItem();
  }

  async function take(found) {
    qError = "";
    busy = `Taking ${found.title}…`;
    try {
      const r = await invoke("search_choose", {
        rowId: queue[qi].row_id,
        found,
        system: sSystem || null,
        art: fetchOn.filter((k) => k !== "wikidata"),
        wikidata: fetchOn.includes("wikidata"),
        overwrite: !noOverwrite,
      });
      busy = "";
      const e = r.enrich;
      const fetched = e.art_saved ? `; ${e.art_saved} images saved` : "";
      const what = found.scraper === "wikidata" ? `matched to ${found.id}` : "identified by you";
      await answered(`${found.title}: ${what}${fetched}.` + (e.errors?.length ? ` Fetching: ${e.errors.join("; ")}` : ""));
    } catch (e) {
      busy = "";
      qError = String(e);
    }
  }

  function takeOwn() {
    take({ scraper: "dat", id: ownTitle.trim(), title: ownTitle.trim(), system: sSystem || null, year: null, score: 1, description: null });
  }

  function toggleFetch(k) {
    fetchOn = fetchOn.includes(k) ? fetchOn.filter((x) => x !== k) : [...fetchOn, k];
  }

  function reveal() {
    const f = qDetail?.files?.[0]?.path;
    if (f) revealItemInDir(f.split("#")[0]).catch((e) => (qError = String(e)));
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
    const d = await call("game_detail", { id });
    if (d?.row?.id !== detail?.row?.id) dims = {};
    detail = d;
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

  // Third-party databases the user downloads (milestone 11).
  let extdbs = $state([]);
  let extBusy = $state("");

  async function loadExtdbs() {
    extdbs = (await call("extdb_list")) ?? [];
  }

  async function getExtdb(d) {
    extBusy = d.db;
    note = `Downloading ${d.name}… (runs in the background; the rest of romforge stays usable)`;
    try {
      const r = await invoke("extdb_download", { name: d.db });
      note = `${r.name}: ${r.entries?.toLocaleString()} entries. Enrich now uses it to fill what libretro and Wikidata leave empty.`;
    } catch (e) {
      error = `${d.name}: ${e}`;
    }
    extBusy = "";
    await loadExtdbs();
  }

  async function dropExtdb(d) {
    await call("extdb_remove", { name: d.db });
    note = `${d.name} removed. What Enrich already filled from it stays until you enrich with refresh.`;
    await loadExtdbs();
  }

  async function loadDats() {
    loadExtdbs();
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
    by_you: "Identified by you",
    derivative: "Translation or hack (identified by you)",
    unknown: "Unknown",
  };
  const artLabel = {
    boxart: "Box art",
    box3d: "3D box",
    title: "Title screen",
    snap: "In-game",
    hero: "Fanart",
    logo: "Logo",
    icon: "Icon",
  };
  // Box art, fanart and logo have their own places; the rest sit under Media.
  const mediaKinds = ["box3d", "title", "snap", "icon"];
  const sourceLabel = { libretro: "libretro", screenscraper: "ScreenScraper", steamgriddb: "SteamGridDB", igdb: "IGDB", launchbox: "LaunchBox" };
  const extdbLabel = { openvgdb: "OpenVGDB", launchbox: "LaunchBox" };

  // Details: empty media boxes only when Settings says so.
  let showEmpty = $state(false);
  // kind -> [width, height] of the image as loaded (its real resolution).
  let dims = $state({});
  // The image open at full size, if any.
  let viewer = $state(null);
  let winW = $state(1200);
  let winH = $state(800);

  function sized(kind, e) {
    dims[kind] = [e.currentTarget.naturalWidth, e.currentTarget.naturalHeight];
  }

  // What the name says, as one line: "USA · En, Ja · Rev 1 · Beta".
  function edition(n) {
    if (!n) return "";
    return [
      n.regions.join(", "),
      n.languages.join(", "),
      n.version,
      n.status,
      n.disc && `Disc ${n.disc}`,
      n.translation && `translation: ${n.translation}`,
      n.hack && `hack: ${n.hack}`,
      n.bad_dump && "bad dump",
      n.alternate && "alternate dump",
      n.unlicensed && "unlicensed",
    ].filter(Boolean).join(" · ");
  }

  function caption(kind) {
    const d = dims[kind];
    const src = detail?.art_sources?.[kind];
    return [artLabel[kind], d ? `${d[0]}×${d[1]}` : "", src ? sourceLabel[src] ?? src : ""].filter(Boolean).join(" · ");
  }

  // Full size: its own pixels, smaller only to fit the window. Title screens
  // and screenshots of old systems are tiny, so they are scaled up by whole
  // steps with sharp pixels instead of a blur.
  function viewerScale(kind) {
    const d = dims[kind];
    if (!d) return 1;
    const fit = Math.min((winW * 0.92) / d[0], (winH * 0.82) / d[1]);
    if (fit < 1) return fit;
    return ["title", "snap"].includes(kind) ? Math.max(1, Math.floor(fit)) : 1;
  }

  // The Development tab: groups of (label, value, link?) with only what is
  // known; an empty group isn't shown.
  function devGroups(f, d) {
    const g = [
      ["Studios", [["Developer", f.developer], ["Publisher", f.publisher], ["Country", f.country]]],
      ["Credits", [["Director", f.director], ["Designer", f.designer], ["Composer", f.composer]]],
      ["Release", [["Released", released(f)], ["Serial", f.serial], ["Rating", f.esrb]]],
      [
        "Identification",
        [
          ["Status", statusLabel[d.row.status]],
          ["DATs", d.dats.join(", ")],
          ["Wikidata", f.wikidata, f.wikidata ? `https://www.wikidata.org/wiki/${f.wikidata}` : null],
        ],
      ],
    ];
    return g.map(([h, rows]) => [h, rows.filter((r) => r[1])]).filter(([, rows]) => rows.length);
  }

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
    if (view !== "library" || settingsOpen || searchOpen || !["ArrowUp", "ArrowDown"].includes(e.key)) return;
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
  call("get_settings").then((s) => s && (showEmpty = s.show_empty_media));
</script>

<svelte:window
  bind:innerWidth={winW}
  bind:innerHeight={winH}
  onkeydown={(e) => {
    if (e.key === "Escape" && viewer) viewer = null;
    else if (e.key === "Escape" && settingsOpen) settingsOpen = false;
    else if (e.key === "Escape" && cleanOpen) cleanOpen = false;
    else if (e.key === "Escape" && searchOpen) searchOpen = false;
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
    <button class="tool" onclick={openSearch} disabled={!!busy} title="Identify games one by one: DAT title suggestions or your own title, and settle Wikidata matches">
      <span class="glyph">⌕</span>Search
    </button>
    <button class="tool" onclick={enrich} disabled={!!busy} title="Fetch metadata and art for the games you have on this platform">
      <span class="glyph">✦</span>Enrich
    </button>
    <button class="tool" onclick={openCleanup} disabled={!!busy} title="Repack identified games from the Source as one file each, named by the DAT, with art and details inside">
      <span class="glyph">✎</span>Cleanup and rename
    </button>
    <span class="spacer"></span>
    <button class="tool" onclick={openSettings} disabled={!!busy} title="Source and Library folders, cleanup options">
      <span class="glyph">⚙</span>Settings
    </button>
    <button class="tool" onclick={play} disabled={!row || !["verified", "by_you", "derivative"].includes(row.status) || !!busy} title="Start the selected game">
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
                ? "Source files stay untouched. Cleanup writes each repacked game into the Library."
                : "Cleanup writes each repacked game into the Library; the originals go to .romforge-trash next to them once the new file is verified."}
            {:else}
              Needs a Library folder for the processed copies.
            {/if}
          </p>
        </section>

        <section class="setting">
          <label for="staging">Staging folder</label>
          <p class="hint">Scratch space for cleanup (tracks inside 7z archives are unpacked here). Must be outside the Source and Library.</p>
          <div class="pick">
            <input id="staging" bind:value={draft.staging_dir} placeholder={defaultStaging} />
            <button onclick={() => browse("staging_dir")}>Browse…</button>
            {#if draft.staging_dir}<button onclick={() => (draft.staging_dir = null)}>Use default</button>{/if}
          </div>
        </section>

        <section class="setting">
          <label class="toggle">
            <input type="checkbox" bind:checked={draft.show_empty_media} />
            Show empty media boxes
          </label>
          <p class="hint">In Details, keep a box for every kind of art (box, fanart, logo, 3D box, title screen, in-game, icon) even when there's none yet. Off: only the art there is.</p>
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

        {#if settingsError}<p class="err" role="alert">{settingsError}</p>{/if}
        <div class="actions">
          <button onclick={() => (settingsOpen = false)}>Cancel</button>
          <button class="primary" onclick={saveSettings}>Save</button>
        </div>
      </div>
    </div>
  {/if}

  {#if cleanOpen}
    <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (cleanOpen = false)}>
      <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="clean-title">
        <h2 id="clean-title">Cleanup and rename</h2>
        <p class="hint">
          Each identified game in the Source becomes one file named by its DAT: a zip for cartridges, a CHD for discs (art and
          details in a folder beside it). Every new file is read back and checked against the original hashes before anything
          else happens. Nothing is ever overwritten.
        </p>
        <label class="toggle">
          <input type="checkbox" bind:checked={cleanOnly} disabled={!selected} onchange={planCleanup} />
          Only the selected game
        </label>
        {#if cleanPlan}
          <p class="meta">
            Into <bdi>{cleanPlan.dest}</bdi> ·
            {cleanPlan.keep_originals ? "originals stay untouched" : "originals go to .romforge-trash next to them, until you undo or empty it"}
          </p>
          {#if cleanPlan.items.length}
            <ul class="choices">
              {#each cleanPlan.items as it}
                <li title={it.parts.map((p) => p.name).join("\n")}>
                  <span class="ctitle">{base(it.target)}</span>
                  <span class="sys">{it.system} · {it.parts.length} file{it.parts.length === 1 ? "" : "s"}{it.kept.length ? " · original stays (holds other files)" : ""}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="hint">Nothing to do: no identified game in the Source that isn't processed yet.</p>
          {/if}
          {#if cleanPlan.skipped.length}
            <details>
              <summary class="hint">{cleanPlan.skipped.length} skipped</summary>
              <ul class="files">
                {#each cleanPlan.skipped as [t, why]}<li>{t}: {why}</li>{/each}
              </ul>
            </details>
          {/if}
        {/if}
        {#if cleanError}<p class="err" role="alert">{cleanError}</p>{/if}
        <div class="actions">
          {#if cleanBatches.length}
            {#if confirmCleanUndo}
              <button onclick={undoCleanup} disabled={!!busy}>Yes, undo batch #{cleanBatches[cleanBatches.length - 1].batch_id}</button>
            {:else}
              <button onclick={() => (confirmCleanUndo = true)} disabled={!!busy} title="Remove the last batch's new files and put its originals back">Undo last…</button>
            {/if}
          {/if}
          <span class="spacer"></span>
          <button onclick={() => (cleanOpen = false)} disabled={!!busy}>Close</button>
          <button class="primary" onclick={applyCleanup} disabled={!cleanPlan?.items.length || !!busy}>
            {busy || `Repack ${cleanPlan?.items.length ?? 0}`}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if searchOpen && queue[qi]}
    {@const item = queue[qi]}
    {@const chosen = results[picked]}
    <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (searchOpen = false)}>
      <div class="dialog search-dialog" role="dialog" aria-modal="true" aria-labelledby="search-title">
        <div class="qhead">
          <h2 id="search-title">Search</h2>
          <span class="qnav">
            <button class="mini" onclick={() => step(-1)} disabled={qi === 0 || !!busy} aria-label="Previous">‹</button>
            <span class="hint">{qi + 1} of {queue.length}</span>
            <button class="mini" onclick={() => step(1)} disabled={qi >= queue.length - 1 || !!busy} aria-label="Next">›</button>
          </span>
        </div>
        <div class="spath">
          <span class="fpath" title={qDetail?.files?.[0]?.path}><bdi>{qDetail?.files?.[0]?.path ?? item.title}</bdi></span>
          <span class="badge {item.status}">{item.need === "wikidata" ? (item.wikidata === "ambiguous" ? "Wikidata: several match" : "Wikidata: no match") : item.status.replace("_", " ")}</span>
          <button class="mini" onclick={reveal} title="Show in the file manager" aria-label="Show in the file manager">📂</button>
        </div>

        <div class="sform">
          <label for="scraper">Scraper</label>
          <div class="pick">
            <select id="scraper" bind:value={scraper} onchange={doSearch}>
              <option value="dat">DAT titles (offline)</option>
              <option value="wikidata">Wikidata</option>
            </select>
            <input bind:value={sText} placeholder={qDetail ? (item.need === "identify" ? qDetail.row.title.split("#").pop() : item.title) : ""} aria-label="Search text" onkeydown={(e) => e.key === "Enter" && doSearch()} />
            <button onclick={doSearch} disabled={searching}>⌕ Search</button>
          </div>
          <label for="splat">Platform</label>
          <div class="pick">
            <select id="splat" bind:value={sSystem} onchange={doSearch}>
              <option value="">{scraper === "dat" ? "Any that fits the file" : "Pick one…"}</option>
              {#each allSystems as s}<option value={s.key}>{s.name}</option>{/each}
            </select>
          </div>
        </div>

        <div class="sgrid">
          <div class="stable" role="listbox" aria-label="Search results">
            <div class="srow shead"><span>Search result</span><span>Year</span><span>Platform</span><span>ID</span><span title="How well it fits">%</span></div>
            {#each results as r, i}
              <div
                class="srow"
                class:sel={i === picked}
                role="option"
                aria-selected={i === picked}
                tabindex="0"
                onclick={() => pick(i)}
                ondblclick={() => take(r)}
                onkeydown={(e) => e.key === "Enter" && take(r)}
              >
                <span class="ctitle" title={r.title}>{r.title}</span>
                <span>{r.year ?? ""}</span>
                <span class="sys">{r.system ?? ""}</span>
                <span class="sys" title={r.band ? `${r.description}: never counted as verified` : ""}>{r.scraper === "wikidata" ? r.id : (r.description?.split(":")[0] ?? "")}</span>
                <span class="score">{Math.round(r.score * 100)}%</span>
              </div>
            {:else}
              <p class="hint empty">{searching ? "Searching…" : scraper === "dat" ? "No DAT game fits it: not by name, header, serial or CRC." : "Wikidata knows nothing by that name."}</p>
            {/each}
          </div>
          <div class="sposter">
            {#if preview?.image}<img src={preview.image} alt="Box art of {preview.title}" />{:else}<span class="hint">{chosen ? (preview ? "No box art" : "…") : ""}</span>{/if}
          </div>
          <div class="sinfo">
            {#if chosen}
              <h3 class="sub">{chosen.title}{chosen.year ? ` (${chosen.year})` : ""}</h3>
              {#if preview?.system_name}<p class="meta">{preview.system_name}</p>{/if}
              {#if chosen.evidence?.length}
                <p class="meta">{chosen.description} (a suggestion, never verified)</p>
                <ul class="evidence">
                  {#each chosen.evidence as e}<li class:agrees={e.agrees}><span aria-hidden="true">{e.agrees ? "+" : "−"}</span> {e.text}</li>{/each}
                </ul>
              {/if}
              {#if preview?.description}<p class="desc">{preview.description}</p>{/if}
              {#if preview?.facts?.length}
                <dl class="facts">
                  {#each preview.facts as [k, v]}<dt>{k}</dt><dd>{v}</dd>{/each}
                </dl>
              {/if}
            {/if}
            <div class="own">
              <label for="own">Not listed?</label>
              <div class="pick">
                <input id="own" bind:value={ownTitle} placeholder="Your own title (e.g. a translation)" />
                <button onclick={takeOwn} disabled={!ownTitle.trim() || !sSystem || !!busy} title={sSystem ? "Name it yourself: shown as identified by you, never as verified" : "Pick a platform first"}>Use</button>
              </div>
            </div>
          </div>
        </div>

        <section class="setting">
          <span class="flabel">Fetch after choosing</span>
          <div class="chips">
            {#each fetchItems as [k, label]}
              <button class="chip" class:on={fetchOn.includes(k)} aria-pressed={fetchOn.includes(k)} onclick={() => toggleFetch(k)}>
                {label}{#if fetchOn.includes(k)} <span aria-hidden="true">×</span>{/if}
              </button>
            {/each}
          </div>
          <label class="toggle"><input type="checkbox" bind:checked={noOverwrite} /> Do not overwrite existing data</label>
        </section>

        {#if qError}<p class="err" role="alert">{qError}</p>{/if}
        <div class="actions">
          <button onclick={() => step(1)} disabled={qi >= queue.length - 1 || !!busy}>Skip</button>
          <span class="spacer"></span>
          <button onclick={() => (searchOpen = false)} disabled={!!busy}>Cancel</button>
          <button class="primary" onclick={() => take(chosen)} disabled={!chosen || !!busy}>{busy || "OK"}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if viewer}
    {@const k = viewer.kind}
    {@const scale = viewerScale(k)}
    <div class="scrim viewer" role="presentation" onclick={() => (viewer = null)}>
      <figure>
        <img
          src={viewer.src}
          alt="{artLabel[k]} of {row?.title}"
          class:pixels={scale > 1}
          style:width={dims[k] ? `${Math.round(dims[k][0] * scale)}px` : null}
          style:height={dims[k] ? `${Math.round(dims[k][1] * scale)}px` : null}
        />
        <figcaption>{caption(k)}{scale > 1 ? ` · shown ×${scale}` : scale < 1 ? ` · shown at ${Math.round(scale * 100)}%` : " · actual size"}</figcaption>
      </figure>
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
                <span class="ind {r.status}" title={statusLabel[r.status]}>{r.status === "verified" ? "✓" : r.status === "by_you" || r.status === "derivative" ? "✎" : r.status === "unknown" ? "?" : "~"}</span>
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
            {#snippet shot(kind, cls)}
              <button class="shot {cls}" onclick={() => (viewer = { kind, src: detail.art[kind] })} title="{caption(kind)} — click for full size">
                <img src={detail.art[kind]} alt="{artLabel[kind]} of {row.title}" onload={(e) => sized(kind, e)} />
              </button>
            {/snippet}
            {#if detail.art.hero}
              <figure class="fanart">
                {@render shot("hero", "")}
                <figcaption>{caption("hero")}</figcaption>
              </figure>
            {:else if showEmpty}
              <figure class="fanart empty"><span>No fanart. Keys for SteamGridDB, ScreenScraper or IGDB in Settings can fill it.</span><figcaption>Fanart</figcaption></figure>
            {/if}

            <div class="overview">
              {#if detail.art.boxart}
                <figure class="cover">
                  {@render shot("boxart", "")}
                  <figcaption>{caption("boxart")}</figcaption>
                </figure>
              {:else if showEmpty}
                <figure class="cover empty"><span>No box art</span><figcaption>Box art</figcaption></figure>
              {/if}
              <div class="about">
                {#if detail.art.logo}
                  <figure class="logo">
                    {@render shot("logo", "")}
                    <figcaption>{caption("logo")}</figcaption>
                  </figure>
                {:else if showEmpty}
                  <figure class="logo empty"><span>No logo</span><figcaption>Logo</figcaption></figure>
                {/if}
                <dl class="facts">
                  {#if f.releaseyear}<dt>Released</dt><dd>{released(f)}</dd>{/if}
                  {#if f.genre}<dt>Genre</dt><dd>{f.genre}</dd>{/if}
                  {#if f.users}<dt>Players</dt><dd>{f.users}</dd>{/if}
                  {#if f.game_mode}<dt>Modes</dt><dd>{f.game_mode}</dd>{/if}
                  {#if f.developer}<dt>Developer</dt><dd>{f.developer}</dd>{/if}
                  {#if f.publisher}<dt>Publisher</dt><dd>{f.publisher}</dd>{/if}
                  {#if f.esrb}<dt>Rating</dt><dd>{f.esrb}</dd>{/if}
                  {#if f.community_rating}<dt>Community</dt><dd>{f.community_rating}</dd>{/if}
                  {#if detail.derived}<dt>{detail.derived.relation === "translation" ? "Translation of" : "Hack of"}</dt><dd>{detail.derived.base}{detail.derived.tag ? ` (${detail.derived.tag})` : ""}</dd>{/if}
                  {#if edition(detail.name)}<dt>Edition</dt><dd>{edition(detail.name)}</dd>{/if}
                  <dt>Files</dt><dd>{row.files} · {size(row.size)}</dd>
                </dl>
              </div>
            </div>

            {#if f.description}
              <h3>Overview{#if detail.field_sources?.description}<span class="hint src"> · from {extdbLabel[detail.field_sources.description] ?? detail.field_sources.description}</span>{/if}</h3>
              <p class="desc">{f.description}</p>
            {/if}

            {#if mediaKinds.some((k) => detail.art[k]) || showEmpty}
              <h3>Media</h3>
              <div class="gallery">
                {#each mediaKinds as k}
                  {#if detail.art[k]}
                    <figure class="media">
                      {@render shot(k, "")}
                      <figcaption>{caption(k)}</figcaption>
                    </figure>
                  {:else if showEmpty}
                    <figure class="media empty"><span>—</span><figcaption>{artLabel[k]}</figcaption></figure>
                  {/if}
                {/each}
              </div>
            {/if}
            <h3>Files</h3>
            <ul class="files">
              {#each detail.files as file}
                <li title={file.sha1}>
                  <span class="badge {file.status === 'matched' ? 'verified' : file.status}">{file.status.replace("_", " ")}</span>
                  <span class="fpath"><bdi>{file.path}</bdi></span>
                  {#if file.game_id}
                    {@const g = file.game_id}
                    <span class="fid" title="What the game's own header or boot files say (evidence, not a match)">{[g.title, g.serial, g.region].filter(Boolean).join(" · ")}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {:else if tab === "development"}
            {@const groups = devGroups(f, detail)}
            {#each groups as [heading, rows]}
              <section class="group">
                <h3>{heading}</h3>
                <dl class="facts wide">
                  {#each rows as [k, v, url]}
                    <dt>{k}</dt>
                    <dd>{#if url}<a href={url} onclick={(e) => { e.preventDefault(); openUrl(url).catch((err) => (error = String(err))); }}>{v}</a>{:else}{v}{/if}</dd>
                  {/each}
                </dl>
              </section>
            {/each}
            {#if !f.wikidata}
              <p class="hint">Not matched on Wikidata yet, so studios and credits are what libretro knows. Enrich looks it up; Search settles games it can't tell apart.</p>
            {/if}
          {:else if tab === "series"}
            {#if detail.series.length}
              <p class="meta">
                {f.series ? `${f.series} · from Wikidata, in series order` : `${f.franchise} franchise · the games romforge has metadata for, oldest first`}
              </p>
              <table class="series">
                <thead><tr><th class="num">#</th><th class="num">Year</th><th>Title</th><th>In your library</th></tr></thead>
                <tbody>
                  {#each detail.series as s}
                    {@const mine = s.in_library && !s.current ? lib.rows.find((r) => r.system === s.system && r.title === s.title) : null}
                    <tr class:current={s.current}>
                      <td class="num">{s.ordinal ?? ""}</td>
                      <td class="num">{s.year ?? "—"}</td>
                      <td>
                        {#if mine}<button class="linkish" onclick={() => open(mine.id)} title="Show this game">{s.title}</button>
                        {:else}{s.title}{#if s.current} <span class="badge">this game</span>{/if}{/if}
                      </td>
                      <td class="sys">{s.in_library ? (lib.platforms.find((p) => p.system === s.system)?.name ?? s.system ?? "yes") : ""}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {:else}
              <p class="hint">No series known for this game yet. Enrich fills it from Wikidata (series) or libretro (franchise).</p>
            {/if}
          {:else}
            {#if detail.links.length}
              {#each [[true, "Game pages", "This game's own page, from the IDs Wikidata keeps."], [false, "Searches", "No ID known yet: these search the site for the title."]] as [direct, heading, hint]}
                {@const ls = detail.links.filter((l) => l.direct === direct)}
                {#if ls.length}
                  <section class="group">
                    <h3>{heading}</h3>
                    <p class="hint">{hint}</p>
                    <ul class="links">
                      {#each ls as l}
                        <li>
                          <a href={l.url} onclick={(e) => { e.preventDefault(); openUrl(l.url).catch((err) => (error = String(err))); }}>{l.label}</a>
                          <span class="hint host">{new URL(l.url).host}</span>
                        </li>
                      {/each}
                    </ul>
                  </section>
                {/if}
              {/each}
            {:else}
              <p class="hint">Identify this file first to get links.</p>
            {/if}
          {/if}
        {/if}
      </section>
    {:else if view === "dats"}
      <section class="wide">
        <div class="bar"><button onclick={downloadDats} disabled={!!busy}>Download DATs for all systems</button></div>
        <section class="extdbs" aria-labelledby="extdb-title">
          <h3 id="extdb-title">Game databases <span class="hint">(optional, downloaded by you)</span></h3>
          <p class="hint">
            Free to download, but published without a licence, so romforge doesn't include them: you fetch them from their
            makers onto this computer. Enrich then uses them only to fill what libretro and Wikidata leave empty.
          </p>
          <ul>
            {#each extdbs as d}
              <li>
                <div class="ehead">
                  <a href={d.homepage} onclick={(e) => { e.preventDefault(); openUrl(d.homepage).catch((err) => (error = String(err))); }}>{d.name}</a>
                  <span class="hint">
                    {d.downloaded_at
                      ? `downloaded ${d.downloaded_at.slice(0, 10)} · ${d.entries?.toLocaleString()} entries · ${size(d.bytes)}`
                      : "not downloaded"}
                  </span>
                  <span class="spacer"></span>
                  {#if d.downloaded_at}<button onclick={() => dropExtdb(d)} disabled={!!extBusy}>Remove</button>{/if}
                  <button onclick={() => getExtdb(d)} disabled={!!extBusy}>
                    {extBusy === d.db ? "Downloading…" : d.downloaded_at ? "Update" : "Download"}
                  </button>
                </div>
                <p class="hint">{d.about}</p>
              </li>
            {/each}
          </ul>
        </section>
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
  /* Every image keeps its own aspect ratio and is never scaled past its
     own pixels in Details; click one for full size. */
  .shot { display: block; padding: 0; border: none; background: none; cursor: zoom-in; border-radius: 0; line-height: 0; }
  .shot img { display: block; max-width: 100%; height: auto; margin: 0 auto; }
  figure { margin: 0; }
  figcaption { font-size: 0.72rem; color: var(--muted); padding: 0.2rem 0.1rem 0; line-height: 1.3; }
  figure.empty span { display: grid; place-items: center; text-align: center; padding: 0.5rem; color: var(--muted); font-size: 0.8rem; border: 1px dashed var(--line); border-radius: 6px; }
  .fanart { margin-bottom: 0.75rem; }
  .fanart .shot img { max-height: 16rem; border-radius: 6px; }
  .fanart.empty span { height: 5rem; }
  .overview { display: flex; gap: 1rem; align-items: flex-start; }
  /* Portrait (most boxes) and landscape (SNES, N64) both fit: the cover
     takes the width its own aspect needs, up to 45% of the panel. */
  .cover { flex: 0 1 auto; max-width: 45%; }
  .cover .shot img { border-radius: 4px; max-height: 15rem; margin: 0; }
  .cover.empty span { height: 14rem; }
  .about { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0.6rem; }
  .logo { align-self: flex-start; }
  .logo .shot img { max-height: 4.5rem; max-width: 16rem; margin: 0; }
  .logo.empty span { height: 3rem; width: 10rem; }
  .facts { display: grid; grid-template-columns: auto 1fr; gap: 0.2rem 0.75rem; margin: 0; align-content: start; font-size: 0.9rem; }
  .facts.wide { max-width: 40rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .detail h3 { font-size: 0.95rem; margin: 1rem 0 0.4rem; }
  .detail h3 .src { font-weight: 400; font-size: 0.8rem; }
  .desc { line-height: 1.55; margin: 0; }
  .gallery { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: flex-end; }
  .media .shot img { max-height: 10rem; max-width: min(100%, 18rem); background: #0002; }
  .media.empty span { height: 6rem; width: 8rem; }
  .viewer { cursor: zoom-out; }
  .viewer figure { display: flex; flex-direction: column; align-items: center; }
  .viewer img { max-width: 92vw; max-height: 82vh; object-fit: contain; box-shadow: 0 8px 30px #0008; }
  .viewer img.pixels { image-rendering: pixelated; max-width: none; max-height: none; }
  .viewer figcaption { color: #ddd; font-size: 0.85rem; padding-top: 0.5rem; }
  .files { list-style: none; padding: 0; margin: 0; font-size: 0.8rem; }
  .files li { display: flex; gap: 0.5rem; align-items: baseline; padding: 0.15rem 0; overflow: hidden; }
  /* rtl on the box and <bdi> inside: long paths are cut at the start, the
     file name stays visible, and the text itself keeps its order */
  .fpath { font-family: ui-monospace, monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; min-width: 0; }
  .fid { flex-shrink: 0; color: var(--muted); font-size: 0.75rem; }
  .group h3 { margin-top: 0.5rem; }
  .group + .group { border-top: 1px solid var(--line); margin-top: 0.75rem; }
  .group .hint { margin: 0 0 0.3rem; }
  table.series { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
  .series th { text-align: left; font-weight: 500; color: var(--muted); font-size: 0.8rem; border-bottom: 1px solid var(--line); padding: 0.2rem 0.4rem; }
  .series td { padding: 0.3rem 0.4rem; border-bottom: 1px solid var(--line); }
  .series .num { width: 3rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .series tr.current td { background: var(--accent-bg); font-weight: 600; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; }
  .linkish:hover { text-decoration: underline; }
  .links { list-style: none; padding: 0; margin: 0; }
  .links li { display: flex; gap: 0.6rem; align-items: baseline; padding: 0.25rem 0; }
  .links a { color: var(--accent); min-width: 9rem; }
  .links .host { font-size: 0.8rem; }
  .hint { color: var(--muted); font-size: 0.85rem; }
  .empty { padding: 2rem 1rem; color: var(--muted); text-align: center; }
  .empty h2 { color: var(--text); }

  .badge { display: inline-block; padding: 0 0.4rem; border-radius: 3px; font-size: 0.75rem; white-space: nowrap; background: var(--line); }
  .badge.verified { color: var(--ok); }
  .badge.incomplete, .badge.candidate, .badge.duplicate { color: var(--warn); }
  .badge.unknown, .badge.wrong_system { color: var(--bad); }

  .wide { grid-column: 2 / 4; overflow: auto; padding: 0 0.75rem 1rem; }
  .bar { display: flex; gap: 0.5rem; align-items: center; padding: 0.5rem 0; flex-wrap: wrap; }
  .extdbs { border: 1px solid var(--line); border-radius: 6px; padding: 0.5rem 0.75rem; margin-bottom: 0.75rem; }
  .extdbs h3 { font-size: 0.95rem; margin: 0 0 0.2rem; }
  .extdbs ul { list-style: none; padding: 0; margin: 0.4rem 0 0; }
  .extdbs li { padding: 0.4rem 0; border-top: 1px solid var(--line); }
  .extdbs .ehead { display: flex; gap: 0.6rem; align-items: baseline; flex-wrap: wrap; }
  .extdbs .ehead a { color: var(--accent); font-weight: 600; }
  .extdbs li .hint { margin: 0.15rem 0 0; }
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
  .wide-dialog { width: min(48rem, calc(100vw - 2rem)); }
  .search-dialog { width: min(64rem, calc(100vw - 2rem)); }
  .qnav { display: flex; gap: 0.4rem; align-items: center; }
  .mini { padding: 0.1rem 0.45rem; }
  .spath { display: flex; gap: 0.5rem; align-items: center; margin-bottom: 0.6rem; }
  .spath .fpath { flex: 1; font-size: 0.85rem; }
  .sform { display: grid; grid-template-columns: auto 1fr; gap: 0.4rem 0.6rem; align-items: center; margin-bottom: 0.6rem; }
  .sform label { font-weight: 600; font-size: 0.9rem; }
  .sform .pick select { max-width: 16rem; }
  .sgrid { display: grid; grid-template-columns: minmax(0, 1.3fr) 9.5rem minmax(0, 1fr); gap: 0.75rem; border-top: 1px solid var(--line); padding-top: 0.6rem; }
  .stable { border: 1px solid var(--line); border-radius: 4px; height: 17rem; overflow-y: auto; font-size: 0.85rem; }
  .srow { display: grid; grid-template-columns: minmax(0, 1fr) 3rem 5.5rem 5.5rem 2.6rem; gap: 0.4rem; padding: 0.2rem 0.5rem; cursor: default; align-items: baseline; }
  .srow:nth-child(even):not(.shead) { background: color-mix(in srgb, var(--line) 35%, transparent); }
  .srow.sel { background: var(--accent-bg); }
  .srow:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .shead { position: sticky; top: 0; background: var(--panel); color: var(--muted); border-bottom: 1px solid var(--line); font-size: 0.8rem; }
  .stable .empty { padding: 0.5rem; }
  .sposter { height: 13rem; display: grid; place-items: center; border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
  .sposter img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .sinfo { display: flex; flex-direction: column; gap: 0.35rem; min-width: 0; max-height: 17rem; overflow-y: auto; }
  .sinfo .desc { font-size: 0.85rem; color: var(--muted); }
  .sinfo .facts { gap: 0.15rem 0.6rem; font-size: 0.8rem; }
  .sinfo .facts dd { overflow-wrap: anywhere; }
  .own { margin-top: auto; padding-top: 0.4rem; }
  .own label { font-size: 0.85rem; font-weight: 600; }
  .flabel { font-weight: 600; font-size: 0.9rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.3rem; margin: 0.4rem 0; }
  .chip { padding: 0.1rem 0.5rem; border-radius: 3px; font-size: 0.8rem; opacity: 0.6; }
  .chip.on { opacity: 1; background: var(--accent-bg); }
  @media (max-width: 52rem) {
    .sgrid { grid-template-columns: 1fr; }
    .sposter { height: 10rem; }
  }
  .qhead { display: flex; justify-content: space-between; align-items: baseline; }
  .choices { list-style: none; padding: 0; margin: 0.4rem 0 0; max-height: 16rem; overflow-y: auto; }
  .choices li { display: flex; gap: 0.6rem; align-items: center; padding: 0.25rem 0; border-bottom: 1px solid var(--line); }
  .ctitle { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .score { width: 2.6rem; color: var(--muted); font-size: 0.8rem; text-align: right; }
  .evidence { list-style: none; padding: 0; margin: 0.2rem 0 0.5rem; font-size: 0.8rem; color: var(--muted); }
  .evidence li.agrees { color: inherit; }
  .evidence span { display: inline-block; width: 1em; }
  .pick select { max-width: 14rem; }
  .badge.by_you, .ind.by_you, .badge.derivative, .ind.derivative { color: var(--accent); }
</style>
