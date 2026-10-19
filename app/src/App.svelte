<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { size } from "./lib.js";
  import SettingsDialog from "./SettingsDialog.svelte";
  import CleanupDialog from "./CleanupDialog.svelte";
  import SearchDialog from "./SearchDialog.svelte";
  import PatchDialog from "./PatchDialog.svelte";
  import ArtPicker from "./ArtPicker.svelte";
  import DatsView from "./DatsView.svelte";
  import OrganizeView from "./OrganizeView.svelte";
  import HomeDialog from "./HomeDialog.svelte";
  import SortButton from "./SortButton.svelte";
  import { sorted, remembered, remember } from "./sort.js";
  import { genres, esrb, regionFlags, stars } from "./facts.js";

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
  // The RomForge folder (milestone 13); null until it exists. With it,
  // Cleanup and rename is Import, and adding a folder offers its import.
  let homeRoot = $state(null);
  let home;
  $effect(() => {
    home?.check();
  });

  // The dialogs and the DATs and organize screens keep their own state;
  // these are their instances (for openSettings() and the like) and what
  // the app needs to know of them: which dialog is open (Escape, arrow
  // keys) and the organize screen's undo prompt (reset on every screen
  // change).
  let settings = $state(null);
  let cleanup = $state(null);
  let search = $state(null);
  let patcher = $state(null);
  let patchOpen = $state(false);
  let artPicker = $state(null);
  let pickerOpen = $state(false);
  // The fanart (and later other kinds) can be swapped for another the
  // sources have: games with a platform only.
  const canPick = (r) => !!r?.system && r.status !== "patch";
  let dats = $state(null);
  let settingsOpen = $state(false);
  let cleanOpen = $state(false);
  let searchOpen = $state(false);
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

  // A patch row's Apply: the patch dialog for the game it is for, with
  // this patch picked.
  async function applyPatchRow() {
    const p = detail.patches[0];
    const base = lib.rows.find((r) => r.id === row.base_id);
    const d = base && (await call("game_detail", { id: base.id }));
    if (d) await patcher.openPatch(base, d, { path: p.path, relation: p.relation });
  }

  async function open(id) {
    selected = id;
    const d = await call("game_detail", { id });
    // Another row was picked while this one loaded: its details win.
    if (selected !== id) return;
    if (d?.row?.id !== detail?.row?.id) dims = {};
    detail = d;
  }

  function show(v) {
    view = v;
    error = "";
    note = "";
    confirmUndo = false;
    if (v === "dats") dats.loadDats();
  }

  async function refresh() {
    const r = await run("Rescanning your folders…", "refresh", {});
    if (r) note = `Rescanned ${r.folders} folders, ${r.entries} entries` + (r.skipped ? `; skipped for their extension: ${r.skipped}` : "") + (r.problems.length ? `; ${r.problems.length} problems: ${r.problems.join("; ")}` : "");
    await load();
  }

  async function addFolder() {
    const r = await run("Scanning…", "scan", { dir: addDir });
    if (r) {
      const problems = [
        ...r.errors.map(([path, member, e]) => `${member ? `${path}#${member}` : path}: ${e}`),
        ...r.sheet_problems.map(([path, e]) => `${path}: ${e}`),
      ];
      const kinds = r.skipped_kinds.slice(0, 6).map((k) => (k.ext ? `.${k.ext} ${k.count}` : `no extension ${k.count}`)).join(", ");
      note = `Scanned ${r.hashed} entries in ${r.root}` + (kinds ? `; skipped for their extension: ${kinds}${r.skipped_kinds.length > 6 ? ", …" : ""}` : "") + (problems.length ? `. ${problems.length} problem${problems.length === 1 ? "" : "s"}: ${problems.slice(0, 3).join("; ")}${problems.length > 3 ? "; …" : ""}` : "");
      adding = false;
      addDir = "";
      await load();
      // What it holds that is identified, ready to move into RomForge.
      if (homeRoot) await cleanup.openCleanup(r.root);
      return;
    }
    await load();
  }

  async function enrich() {
    const system = platform === "*" || platform === "" ? null : platform;
    const r = await run(`Fetching metadata and art${system ? ` for ${system}` : ""}…`, "enrich", { system });
    if (r) {
      note = `${r.games} games: ${r.games_with_metadata} with metadata; ${r.art_saved} images saved, ${r.art_missing} not found, ${r.art_cached} already cached`;
      if (r.wikidata) note += `. Wikidata: ${r.wikidata.matched} matched, ${r.wikidata.ambiguous} to settle in Search, ${r.wikidata.unmatched} not found`;
      if (r.errors?.length) note += `. ${r.errors.length} problem${r.errors.length === 1 ? "" : "s"}: ${r.errors.slice(0, 3).join("; ")}${r.errors.length > 3 ? "; …" : ""}`;
    }
    await load();
  }

  // Undo a title picked in Search for the selected row.
  async function forget() {
    await run("Forgetting…", "forget_identity", { rowId: selected });
    if (!error) note = "Forgotten: the files show as unknown again";
    await load();
  }

  async function play() {
    const row = lib.rows.find((r) => r.id === selected);
    if (!row) return;
    const r = await run(`Starting ${row.label ?? row.title}…`, "launch", { rowId: row.id });
    if (r) note = r;
  }

  const statusLabel = {
    verified: "Verified",
    incomplete: "Incomplete",
    candidate: "Candidate",
    wrong_system: "Wrong system or DAT",
    duplicate: "Duplicate",
    by_you: "Identified by you",
    derivative: "Translation or hack (identified by you)",
    detected: "Identified by ScummVM's game list",
    unknown: "Unknown",
    missing: "Not in your library",
    patch: "Patch file",
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
  // `flags`: the regions are shown as flags, so leave them out.
  function edition(n, flags = false) {
    if (!n) return "";
    return [
      flags ? "" : n.regions.join(", "),
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
  // known; an empty group isn't shown. Studios and people Wikidata names get cards (with a logo or photo);
  // what only libretro knows stays a line of text.
  const roleLabel = { developer: "Developer", publisher: "Publisher", director: "Director", designer: "Designer", composer: "Composer" };
  const studioRoles = ["developer", "publisher"];
  const peopleRoles = ["director", "designer", "composer"];
  function cardsFor(d, roles) {
    return roles
      .map((role) => [roleLabel[role], (d.credits ?? []).filter((c) => c.role === role)])
      .filter(([, cs]) => cs.length);
  }
  function devGroups(f, d) {
    const carded = new Set((d.credits ?? []).map((c) => c.role));
    const text = (role, v) => [roleLabel[role], carded.has(role) ? null : v];
    const g = [
      ["Studios", [text("developer", f.developer), text("publisher", f.publisher), ["Country", f.country]]],
      ["Credits", [text("director", f.director), text("designer", f.designer), text("composer", f.composer)]],
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
    const cardsOf = { Studios: studioRoles, Credits: peopleRoles };
    return g
      .map(([h, rows]) => [h, rows.filter((r) => r[1])])
      .filter(([h, rows]) => rows.length || cardsFor(d, cardsOf[h] ?? []).length);
  }

  // The Series tab: each title once (the platforms you have it on
  // together), numbered entries apart from the rest.
  let seriesSort = $state(remembered("series", { key: "order", dir: 1 }));
  $effect(() => remember("series", seriesSort));
  const ordNum = (o) => {
    const n = parseFloat(o);
    return Number.isFinite(n) ? n : null;
  };
  const yearNum = (y) => (y ? Number(String(y).slice(0, 4)) || null : null);
  const seriesGetters = {
    order: (e) => ordNum(e.ordinal) ?? yearNum(e.year),
    year: (e) => yearNum(e.year),
    title: (e) => e.title,
    have: (e) => (e.mine.length ? 0 : 1),
  };
  function seriesParts(list) {
    const by = new Map();
    for (const s of list) {
      const k = s.title.toLowerCase();
      let e = by.get(k);
      if (!e) {
        e = { title: s.title, ordinal: s.ordinal, year: s.year, current: false, mine: [] };
        by.set(k, e);
      }
      e.ordinal ??= s.ordinal;
      if (yearNum(s.year) && (!yearNum(e.year) || yearNum(s.year) < yearNum(e.year))) e.year = s.year;
      e.current ||= s.current;
      if (s.in_library) e.mine.push(s);
    }
    const all = [...by.values()];
    // Unnumbered games have no series order: by year then.
    const others = seriesSort.key === "order" ? { key: "year", dir: seriesSort.dir } : seriesSort;
    return {
      numbered: sorted(all.filter((e) => e.ordinal), seriesSort, seriesGetters),
      others: sorted(all.filter((e) => !e.ordinal), others, seriesGetters),
      total: all.length,
      have: all.filter((e) => e.mine.length).length,
    };
  }

  // 1991, 1991-08 or 1991-08-21.
  function released(f) {
    if (!f.releaseyear) return "";
    return [f.releaseyear, f.releasemonth, f.releaseday]
      .filter(Boolean)
      .map((v, i) => (i ? String(v).padStart(2, "0") : v))
      .join("-");
  }

  // One row per game: its original (listed even when you only have
  // translations, hacks or patches of it), expanding to its other versions
  // under headings (regions, editions, translations, hacks, and patch
  // files apart), each heading folding on its own. A group is listed when
  // any of its versions matches the filter.
  const CATS = [
    ["region", "Other regions"],
    ["edition", "Editions"],
    ["translation", "Translations"],
    ["hack", "Hacks"],
  ];
  const PATCH_CATS = [
    ["patch_translation", "Translations"],
    ["patch_hack", "Hacks"],
  ];
  let expanded = $state({});
  // The list's sort (games by it, and the versions under each heading).
  const statusOrder = ["verified", "incomplete", "by_you", "derivative", "detected", "candidate", "duplicate", "wrong_system", "missing", "patch", "unknown"];
  const listGetters = {
    title: (r) => r.label ?? r.title,
    platform: (r) => r.system_name,
    year: (r) => r.year,
    status: (r) => statusOrder.indexOf(r.status),
  };
  let listSort = $state(remembered("games", { key: "title", dir: 1 }));
  $effect(() => remember("games", listSort));
  // Headings start open; groups start closed.
  const isOpen = (key) => expanded[key] ?? key.includes("|");
  function toggle(key) {
    expanded[key] = !isOpen(key);
  }
  let games = $derived(lib.rows.filter((r) => r.main).length);
  let rows = $derived.by(() => {
    const match = (r) =>
      (platform === "*" || (platform === "" ? r.system === null : r.system === platform)) &&
      (!filter || r.title.toLowerCase().includes(filter.toLowerCase()));
    const all = sorted(lib.rows, listSort, listGetters);
    const members = new Map();
    for (const r of all) if (r.group) members.set(r.group, [...(members.get(r.group) ?? []), r]);
    const out = [];
    const heading = (parent, cat, label, items, depth) => {
      if (!items.length) return;
      const key = `${parent}|${cat}`;
      out.push({ head: true, id: `h:${key}`, key, label, n: items.length, depth });
      if (isOpen(key)) for (const m of items) out.push({ ...m, depth: depth + 1 });
    };
    for (const r of all) {
      if (!r.main) continue;
      const group = r.group ? members.get(r.group) : [r];
      if (!group.some(match)) continue;
      out.push(r);
      if (!r.group || !isOpen(r.group)) continue;
      const others = group.filter((m) => !m.main);
      for (const [c, label] of CATS) heading(r.group, c, label, others.filter((m) => m.category === c), 1);
      const patches = others.filter((m) => m.category?.startsWith("patch_"));
      if (patches.length) {
        const key = `${r.group}|patches`;
        out.push({ head: true, id: `h:${key}`, key, label: "Patches", n: patches.length, depth: 1 });
        if (isOpen(key)) for (const [c, label] of PATCH_CATS) heading(key, c, label, patches.filter((m) => m.category === c), 2);
      }
      // Anything without a heading (shouldn't happen) still shows.
      out.push(...others.filter((m) => !m.category).map((m) => ({ ...m, depth: 1 })));
    }
    return out;
  });
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
    if (view !== "library" || settingsOpen || searchOpen || patchOpen || pickerOpen || !["ArrowUp", "ArrowDown"].includes(e.key)) return;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
    e.preventDefault();
    const i = rows.findIndex((r) => r.id === selected);
    let j = Math.max(0, Math.min(rows.length - 1, e.key === "ArrowDown" ? i + 1 : i < 0 ? 0 : i - 1));
    // Headings are passed over.
    const step = e.key === "ArrowDown" ? 1 : -1;
    while (rows[j]?.head && rows[j + step]) j += step;
    if (!rows[j] || rows[j].head) return;
    open(rows[j].id);
    if (scroller) {
      const top = j * ROW;
      const head = 28;
      if (top < scroller.scrollTop) scroller.scrollTop = top;
      else if (top + ROW + head > scroller.scrollTop + viewH) scroller.scrollTop = top + ROW + head - viewH;
    }
  }
  let f = $derived(detail?.fields ?? {});

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
    else if (e.key === "Escape" && patchOpen) patchOpen = false;
    else if (e.key === "Escape" && pickerOpen && !busy) pickerOpen = false;
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
    <button class="tool" onclick={() => search.openSearch()} disabled={!!busy} title="Identify games one by one: DAT title suggestions or your own title, and settle Wikidata matches">
      <span class="glyph">⌕</span>Search
    </button>
    <button class="tool" onclick={enrich} disabled={!!busy} title="Fetch metadata and art for the games you have on this platform">
      <span class="glyph">✦</span>Enrich
    </button>
    {#if homeRoot}
      <button class="tool" onclick={() => cleanup.openCleanup()} disabled={!!busy} title="Move identified games from your folders into RomForge, one file each, named by the DAT, with art and details inside">
        <span class="glyph">✎</span>Import
      </button>
    {:else}
      <button class="tool" onclick={() => cleanup.openCleanup()} disabled={!!busy} title="Repack identified games from the Source as one file each, named by the DAT, with art and details inside">
        <span class="glyph">✎</span>Cleanup and rename
      </button>
    {/if}
    <span class="spacer"></span>
    <button class="tool" onclick={() => settings.openSettings()} disabled={!!busy} title="Source and Library folders, cleanup options">
      <span class="glyph">⚙</span>Settings
    </button>
    <button class="tool" onclick={play} disabled={!row || !row.playable || !!busy} title="Start the selected game">
      <span class="glyph">▶</span>Play
    </button>
  </header>

  <SettingsDialog bind:this={settings} bind:open={settingsOpen} bind:note bind:showEmpty bind:homeRoot {call} />

  <CleanupDialog bind:this={cleanup} bind:open={cleanOpen} bind:busy bind:note {selected} {homeRoot} {call} {load} />

  <HomeDialog bind:this={home} bind:root={homeRoot} bind:note {load} />

  <SearchDialog bind:this={search} bind:open={searchOpen} bind:busy bind:note {selected} {call} {load} />

  <PatchDialog bind:this={patcher} bind:open={patchOpen} bind:busy bind:note {call} {load} />
  <ArtPicker bind:this={artPicker} bind:open={pickerOpen} bind:busy bind:note view={() => (viewer = { kind: "hero", src: detail.art.hero })} reload={() => open(row.id)} />

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
        <span class="icon all">ALL</span><span class="pname">All games</span><span class="count">{games}</span>
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
          <div class="table" class:allsys={platform === "*"} bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
            <div class="thead">
              <span><SortButton label="Title" key="title" bind:sort={listSort} /></span>
              {#if platform === "*"}<span><SortButton label="Platform" key="platform" bind:sort={listSort} /></span>{/if}
              <span class="yr"><SortButton label="Year" key="year" bind:sort={listSort} title="Sort by release year (from Enrich)" /></span>
              <span class="ind"><SortButton label="ID" key="status" bind:sort={listSort} title="Sort by status: verified first" /></span>
              <span class="ind" title="Metadata">Info</span>
              <span class="ind" title="Box art">Box</span>
              <span class="ind" title="Title screen">Title</span>
              <span class="ind" title="In-game screenshot">Snap</span>
            </div>
            <!-- Only the rows in view are drawn (fixed row height), so a list of
                 tens of thousands stays quick. -->
            <div style="height: {first * ROW}px"></div>
            {#each rows.slice(first, last) as r (r.id)}
              <div class="trow">
              {#if r.head}
                <button class="tr cat" style="padding-left: {0.7 + r.depth * 1.1}rem" onclick={() => toggle(r.key)} aria-expanded={isOpen(r.key)}>
                  <span class="title">{isOpen(r.key) ? "▾" : "▸"} {r.label}<span class="nver"> · {r.n}</span></span>
                </button>
              {:else}
              {#if r.main && r.versions > 1}
                <button
                  class="twisty"
                  onclick={() => toggle(r.group)}
                  aria-expanded={isOpen(r.group)}
                  title="{r.versions - 1} more: other regions, editions, translations, hacks and patches"
                >{isOpen(r.group) ? "▾" : "▸"}</button>
              {/if}
              <button class="tr" class:sel={r.id === selected} class:version={!r.main} class:missing={r.status === "missing"} onclick={() => open(r.id)}>
                <span class="title" style={r.depth ? `padding-left: ${0.7 + r.depth * 1.1}rem` : undefined} title={r.label ? r.title : undefined}>{r.label ?? r.title}{#if r.main && r.versions > 1}<span class="nver"> · {r.versions}</span>{/if}</span>
                {#if platform === "*"}<span class="sys">{r.system ?? "—"}</span>{/if}
                <span class="yr">{r.year ?? ""}</span>
                <span class="ind {r.status}" title={statusLabel[r.status]}>{r.status === "verified" ? "✓" : r.status === "by_you" || r.status === "derivative" ? "✎" : r.status === "detected" ? "◆" : r.status === "unknown" ? "?" : r.status === "missing" ? "○" : r.status === "patch" ? "✚" : "~"}</span>
                <span class="ind" class:yes={r.metadata}>{r.metadata ? "✓" : "·"}</span>
                {#each ["boxart", "title", "snap"] as k}
                  <span class="ind" class:yes={r.art.includes(k)}>{r.art.includes(k) ? "✓" : "·"}</span>
                {/each}
              </button>
              {/if}
              </div>
            {/each}
            <div style="height: {(rows.length - last) * ROW}px"></div>
          </div>
          <p class="foot">{rows.filter((r) => r.main).length} of {games} games</p>
        {/if}
      </section>

      <section class="detail">
        {#if !row || !detail}
          <div class="empty"><p>Pick a game to see its details.</p></div>
        {:else}
          <div class="dhead">
            <h1>{row.label ?? row.title}</h1>
            {#if row.label}<p class="meta">Set <code>{row.title}</code></p>{/if}
            <p class="meta">
              {row.system_name ?? "Unidentified"} · <span class="badge {row.status}">{statusLabel[row.status]}</span>
              {#if row.status === "by_you" || row.status === "derivative"}
                <button class="link" onclick={forget} disabled={!!busy} title="Undo naming these files: they show as unknown again">Forget</button>
              {/if}
              {#if detail.patch_links?.length && (row.status === "verified" || row.status === "by_you")}
                <button class="link" onclick={() => patcher.openPatch(row, detail)} disabled={!!busy} title="Apply a translation or hack you downloaded: a new copy, your original untouched">{detail.patches?.length ? `Patch… (${detail.patches.length} found)` : "Patch…"}</button>
              {/if}
              {#if row.status === "patch" && detail.patches?.[0]}
                {#if row.base_id}
                  <button class="link" onclick={applyPatchRow} disabled={!!busy} title="Make a patched copy of the game it is for; your original untouched">Apply…</button>
                {:else}
                  <span class="hint">{row.system ? "Add the game it is for to apply it" : "No game found for it"}</span>
                {/if}
              {/if}
              {#if row.status === "missing"}<span class="hint">You have only versions of it{detail.patches?.length ? ` (${detail.patches.length} patch${detail.patches.length > 1 ? "es" : ""} found for it)` : ""}</span>{/if}
              {#if detail.patched}<span class="badge" title="romforge made it from {detail.patched.patch} ({detail.patched.kind}) on {detail.patched.created_at}">patched from {detail.patched.patch}</span>{/if}
              {#if row.cue_differs && row.cue_fits}<span class="badge" title="All tracks match; the cue sheet was rewritten but names exactly these tracks">cue rewritten</span>{:else if row.cue_differs}<span class="badge" title="All tracks match; the cue sheet differs from the DAT's">cue differs</span>{/if}
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
                {#if canPick(row)}
                  <button class="shot pick" onclick={() => artPicker.openPicker(row, "hero", "Fanart", detail.art.hero)} title="{caption('hero')} — click to choose another fanart">
                    <img src={detail.art.hero} alt="Fanart of {row.title}" onload={(e) => sized("hero", e)} />
                    <span class="change" aria-hidden="true">Change fanart</span>
                  </button>
                {:else}
                  {@render shot("hero", "")}
                {/if}
                <figcaption>{caption("hero")}</figcaption>
              </figure>
            {:else if showEmpty || canPick(row)}
              <figure class="fanart empty">
                {#if canPick(row)}
                  <button class="emptypick" onclick={() => artPicker.openPicker(row, "hero", "Fanart", null)}>No fanart yet: find one…</button>
                {:else}
                  <span>No fanart. Keys for SteamGridDB, ScreenScraper or IGDB in Settings can fill it.</span>
                {/if}
                <figcaption>Fanart</figcaption>
              </figure>
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
                  {#if f.genre}
                    {@const gs = genres(f.genre)}
                    <dt>Genre</dt>
                    <dd class="icons">{#each gs as g}<span class="genre" title={g.name} aria-label={g.name}>{@html g.svg}</span>{/each}{#if gs.length === 1}<span class="gname">{gs[0].name}</span>{/if}</dd>
                  {/if}
                  {#if f.users}<dt>Players</dt><dd>{f.users}</dd>{/if}
                  {#if f.game_mode}<dt>Modes</dt><dd>{f.game_mode}</dd>{/if}
                  {#snippet studio(role, text)}
                    {@const logos = (detail.credits ?? []).filter((c) => c.role === role && c.picture && c.kind === "logo")}
                    {#if logos.length}<span class="logos">{#each logos as c}<img class="studio" src={c.picture} alt={c.name} title={c.name} />{/each}</span>{:else}{text}{/if}
                  {/snippet}
                  {#if f.developer}<dt>Developer</dt><dd>{@render studio("developer", f.developer)}</dd>{/if}
                  {#if f.publisher}<dt>Publisher</dt><dd>{@render studio("publisher", f.publisher)}</dd>{/if}
                  {#if f.esrb}
                    {@const r = esrb(f.esrb)}
                    <dt>Rating</dt>
                    <dd>{#if r}<span class="esrb" title="ESRB: {r.label}" aria-label="ESRB rating {r.label}"><b>{r.code}</b><small>ESRB</small></span>{:else}{f.esrb}{/if}</dd>
                  {/if}
                  {#if f.community_rating}
                    {@const st = stars(f.community_rating)}
                    <dt>Community</dt>
                    <dd>{#if st}<span class="stars" title="{st.value} out of {st.max}" aria-label="{st.value} out of {st.max} stars"><span class="on" style="width: {st.pct}%">★★★★★</span>★★★★★</span> <span class="score">{st.value}{st.votes ? ` · ${st.votes.toLocaleString()} votes` : ""}</span>{:else}{f.community_rating}{/if}</dd>
                  {/if}
                  {#if detail.derived}<dt>{detail.derived.relation === "translation" ? "Translation of" : "Hack of"}</dt><dd>{detail.derived.base}{detail.derived.tag ? ` (${detail.derived.tag})` : ""}</dd>{/if}
                  {#if detail.proof}<dt>Named by</dt><dd title="Not verified: {detail.proof}, beside it, applied to your verified {detail.derived?.base} gives exactly this file">romforge, from {detail.proof}</dd>{/if}
                  {#if regionFlags(detail.name).length || edition(detail.name, true)}
                    <dt>Edition</dt>
                    <dd class="icons">
                      {#each regionFlags(detail.name) as r}{#if r.flag}<span class="flag" title={r.name} aria-label={r.name}>{r.flag}</span>{:else}<span class="gname">{r.name}</span>{/if}{/each}
                      {#if edition(detail.name, true)}<span class="gname">{edition(detail.name, true)}</span>{/if}
                    </dd>
                  {/if}
                  <dt>Files</dt><dd>{row.files} · {size(row.size)}</dd>
                  {#if row.folder}<dt>Folder</dt><dd class="fpath" title="The game is this whole folder (or archive): Import and Play take all of it"><bdi>{row.folder}</bdi></dd>{/if}
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
            {#snippet cards(roles)}
              {#each cardsFor(detail, roles) as [label, cs]}
                <p class="role">{label}{cs.length > 1 ? "s" : ""}</p>
                <div class="cards">
                  {#each cs as c (c.qid + c.role)}
                    <figure class="credit {c.kind ?? 'none'}">
                      <button class="face" onclick={() => openUrl(`https://www.wikidata.org/wiki/${c.qid}`).catch((err) => (error = String(err)))} title="{c.name} on Wikidata">
                        {#if c.picture}<img src={c.picture} alt={c.kind === "logo" ? `${c.name} logo` : c.name} />
                        {:else}<span class="initials" aria-hidden="true">{c.name.split(/\s+/).map((w) => w[0]).join("").slice(0, 2)}</span>{/if}
                      </button>
                      <figcaption>
                        {c.name}
                        {#if c.source}<button class="linkish pic" onclick={() => openUrl(c.source).catch((err) => (error = String(err)))} title="The picture on Wikimedia Commons: its author and licence">picture: Commons</button>{/if}
                      </figcaption>
                    </figure>
                  {/each}
                </div>
              {/each}
            {/snippet}
            {#each groups as [heading, rows]}
              {@const roles = heading === "Studios" ? studioRoles : heading === "Credits" ? peopleRoles : []}
              {#if rows.length || cardsFor(detail, roles).length}
                <section class="group">
                  <h3>{heading}</h3>
                  {@render cards(roles)}
                  {#if rows.length}
                    <dl class="facts wide">
                      {#each rows as [k, v, url]}
                        <dt>{k}</dt>
                        <dd>{#if url}<a href={url} onclick={(e) => { e.preventDefault(); openUrl(url).catch((err) => (error = String(err))); }}>{v}</a>{:else}{v}{/if}</dd>
                      {/each}
                    </dl>
                  {/if}
                </section>
              {/if}
            {/each}
            {#if !f.wikidata}
              <p class="hint">Not matched on Wikidata yet, so studios and credits are what libretro knows. Enrich looks it up; Search settles games it can't tell apart.</p>
            {/if}
          {:else if tab === "series"}
            {#if detail.series.length}
              {@const sp = seriesParts(detail.series)}
              <p class="meta">
                {f.series ? `${f.series} · from Wikidata` : `${f.franchise} franchise · the games romforge has metadata for`}
                · {sp.total} game{sp.total === 1 ? "" : "s"} · you have {sp.have}
              </p>
              <p class="sortbar">
                Sort:
                <SortButton label="Series order" key="order" bind:sort={seriesSort} title="Numbered entries by their number, the rest by year" />
                <SortButton label="Year" key="year" bind:sort={seriesSort} />
                <SortButton label="Title" key="title" bind:sort={seriesSort} />
                <SortButton label="Yours first" key="have" bind:sort={seriesSort} title="The games you have first" />
              </p>
              {#each [[sp.numbered.length ? "Main series" : "", sp.numbered], [sp.numbered.length ? "Other games in the series" : "Games", sp.others]] as [heading, es]}
                {#if es.length}
                  {#if heading}<h3>{heading}<span class="nver"> · {es.length}</span></h3>{/if}
                  <ol class="serieslist">
                    {#each es as e (e.title)}
                      <li class:current={e.current} class:owned={e.mine.length}>
                        <span class="ord">{e.ordinal ?? ""}</span>
                        <span class="stitle">
                          {#if e.mine.length && !e.current}
                            {@const r = lib.rows.find((x) => x.system === e.mine[0].system && x.title === e.mine[0].title)}
                            {#if r}<button class="linkish" onclick={() => open(r.id)} title="Show this game">{e.title}</button>{:else}{e.title}{/if}
                          {:else}{e.title}{/if}
                          {#if e.current}<span class="badge">this game</span>{/if}
                        </span>
                        <span class="syear">{e.year ?? "—"}</span>
                        <span class="shave">
                          {#each e.mine as m}<span class="badge verified" title="In your library on {lib.platforms.find((p) => p.system === m.system)?.name ?? m.system}">{lib.platforms.find((p) => p.system === m.system)?.short ?? m.system}</span>{/each}
                        </span>
                      </li>
                    {/each}
                  </ol>
                {/if}
              {/each}
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
    {/if}
    <DatsView bind:this={dats} active={view === "dats"} {busy} bind:note bind:error {call} {run} {load} />
    <OrganizeView active={view === "organize"} {busy} bind:note bind:confirmUndo {homeRoot} openImport={() => cleanup.openCleanup()} {run} {load} />
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
  :global(.app button) { font: inherit; color: inherit; background: transparent; border: 1px solid var(--line); border-radius: 6px; padding: 0.25rem 0.75rem; cursor: pointer; }
  :global(.app button:disabled) { opacity: 0.45; cursor: default; }
  :global(.app input) { font: inherit; color: inherit; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 0.3rem 0.5rem; }
  h1 { font-size: 1.25rem; margin: 0; font-weight: 600; }
  :global(.app h2) { font-size: 1rem; margin: 0 0 0.25rem; }
  :global(.app h3) { font-size: 0.9rem; margin: 1rem 0 0.25rem; color: var(--muted); font-weight: 600; }

  .toolbar { display: flex; gap: 0.25rem; padding: 0.4rem 0.75rem; border-bottom: 1px solid var(--line); background: var(--panel); }
  .tool { display: flex; flex-direction: column; align-items: center; border: none; padding: 0.2rem 0.6rem; font-size: 0.8rem; gap: 0.1rem; }
  .tool:hover:not(:disabled) { background: var(--accent-bg); }
  .glyph { font-size: 1.25rem; line-height: 1.2rem; }
  :global(.app .spacer) { flex: 1; }
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
  .thead, .tr { display: grid; grid-template-columns: minmax(0, 1fr) 2.8rem repeat(5, 2.4rem); align-items: center; gap: 0.25rem; padding: 0.3rem 0.6rem; }
  .allsys .thead, .allsys .tr { grid-template-columns: minmax(0, 1fr) auto 2.8rem repeat(5, 2.4rem); }
  .yr { color: var(--muted); font-size: 0.8rem; text-align: center; font-variant-numeric: tabular-nums; }
  .thead { position: sticky; top: 0; background: var(--panel); font-size: 0.75rem; color: var(--muted); border-bottom: 1px solid var(--line); }
  .trow { position: relative; }
  .twisty { position: absolute; left: 0; top: 0; height: 32px; width: 1.2rem; padding: 0; border: none; border-radius: 0; color: var(--muted); z-index: 1; }
  .tr .title { padding-left: 0.7rem; }
  .tr.version .title { padding-left: 1.8rem; color: var(--muted); }
  .tr.cat .title { color: var(--muted); font-size: 0.85rem; font-weight: 600; padding-left: 0; }
  .tr.missing .title { font-style: italic; color: var(--muted); }
  .nver { color: var(--muted); font-size: 0.8rem; }
  .tr { width: 100%; height: 32px; box-sizing: border-box; padding-top: 0; padding-bottom: 0; border: none; border-radius: 0; border-bottom: 1px solid var(--line); text-align: left; }
  .tr:hover { background: var(--accent-bg); }
  .tr.sel { background: var(--accent-bg); box-shadow: inset 3px 0 var(--accent); }
  .title { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  :global(.app .sys) { color: var(--muted); font-size: 0.8rem; white-space: nowrap; }
  .ind { text-align: center; color: var(--muted); }
  .ind.yes, .ind.verified { color: var(--ok); }
  .ind.incomplete, .ind.candidate, .ind.duplicate { color: var(--warn); }
  .ind.unknown, .ind.wrong_system { color: var(--bad); }
  .foot { margin: 0; padding: 0.3rem 0.6rem; font-size: 0.8rem; color: var(--muted); border-top: 1px solid var(--line); }

  /* Details: centred, its facts on the left of the cover. */
  .detail { overflow-y: auto; padding: 0.75rem 1rem 1.5rem; text-align: center; }
  .detail .tabs { justify-content: center; }
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
  .fanart .shot { margin: 0 auto; }
  .fanart .shot.pick { position: relative; cursor: pointer; }
  .fanart .change { position: absolute; right: 0.5rem; bottom: 0.5rem; padding: 0.2rem 0.55rem; border-radius: 4px; background: #000a; color: #fff; font-size: 0.75rem; line-height: 1.4; opacity: 0; transition: opacity 0.15s; }
  .fanart .shot.pick:hover .change, .fanart .shot.pick:focus-visible .change { opacity: 1; }
  .emptypick { width: 100%; height: 3rem; border: 1px dashed var(--line); border-radius: 6px; background: none; color: var(--muted); cursor: pointer; }
  .emptypick:hover { color: var(--accent); border-color: var(--accent); }
  .fanart .shot img { max-height: 16rem; border-radius: 6px; }
  .fanart.empty span { height: 5rem; }
  .overview { display: flex; gap: 1.25rem; align-items: flex-start; justify-content: center; }
  .overview .about { order: 0; }
  .overview .cover { order: 1; }
  /* Portrait (most boxes) and landscape (SNES, N64) both fit: the cover
     takes the width its own aspect needs, up to 45% of the panel. */
  .cover { flex: 0 1 auto; max-width: 45%; }
  .cover .shot img { border-radius: 4px; max-height: 15rem; margin: 0; }
  .cover.empty span { height: 14rem; }
  .about { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0.6rem; align-items: center; }
  .logo { align-self: center; }
  .detail .facts { justify-content: center; }
  .detail .facts dt { text-align: right; }
  .detail .facts dd { text-align: left; }
  .facts dd.icons { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
  .genre { display: inline-grid; place-items: center; width: 1.9rem; height: 1.9rem; border-radius: 6px; background: var(--accent-bg); color: var(--accent); cursor: help; }
  .gname { color: var(--muted); font-size: 0.85rem; }
  .flag { font-size: 1.35rem; line-height: 1; cursor: help; font-family: "Noto Color Emoji", "Apple Color Emoji", "Segoe UI Emoji", sans-serif; }
  .logos { display: inline-flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  .studio { max-height: 2.2rem; max-width: 8rem; object-fit: contain; background: #fff; border-radius: 4px; padding: 0.15rem 0.3rem; }
  /* Drawn here, not the ESRB's own artwork: the letter in a white box with
     a black rim, as the rating marks look. */
  .esrb { display: inline-flex; flex-direction: column; align-items: center; justify-content: center; width: 2rem; height: 2.6rem; background: #fff; color: #000; border: 2px solid #000; outline: 1px solid #fff; border-radius: 2px; line-height: 1; cursor: help; }
  .esrb b { font-size: 1.15rem; font-family: Arial, Helvetica, sans-serif; font-weight: 900; }
  .esrb small { font-size: 0.45rem; font-weight: 700; letter-spacing: 0.03em; margin-top: 0.15rem; }
  .stars { position: relative; display: inline-block; color: var(--line); letter-spacing: 0.08em; font-size: 1.05rem; line-height: 1; }
  .stars .on { position: absolute; inset: 0 auto 0 0; overflow: hidden; white-space: nowrap; color: #f5b301; }
  .score { color: var(--muted); font-size: 0.85rem; }
  .logo .shot img { max-height: 4.5rem; max-width: 16rem; margin: 0; }
  .logo.empty span { height: 3rem; width: 10rem; }
  :global(.app .facts) { display: grid; grid-template-columns: auto 1fr; gap: 0.2rem 0.75rem; margin: 0; align-content: start; font-size: 0.9rem; }
  .facts.wide { max-width: 40rem; margin: 0 auto; }
  :global(.app dt) { color: var(--muted); }
  :global(.app dd) { margin: 0; }
  .detail h3 { font-size: 0.95rem; margin: 1rem 0 0.4rem; }
  .detail h3 .src { font-weight: 400; font-size: 0.8rem; }
  :global(.app .desc) { line-height: 1.55; margin: 0; }
  .gallery { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: flex-end; justify-content: center; }
  .media .shot img { max-height: 10rem; max-width: min(100%, 18rem); background: #0002; }
  .media.empty span { height: 6rem; width: 8rem; }
  .viewer { cursor: zoom-out; }
  .viewer figure { display: flex; flex-direction: column; align-items: center; }
  .viewer img { max-width: 92vw; max-height: 82vh; object-fit: contain; box-shadow: 0 8px 30px #0008; }
  .viewer img.pixels { image-rendering: pixelated; max-width: none; max-height: none; }
  .viewer figcaption { color: #ddd; font-size: 0.85rem; padding-top: 0.5rem; }
  :global(.app .files) { list-style: none; padding: 0; margin: 0; font-size: 0.8rem; }
  :global(.app .files li) { display: flex; gap: 0.5rem; align-items: baseline; padding: 0.15rem 0; overflow: hidden; }
  .detail .files li { justify-content: center; }
  /* rtl on the box and <bdi> inside: long paths are cut at the start, the
     file name stays visible, and the text itself keeps its order */
  :global(.app .fpath) { font-family: ui-monospace, monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; min-width: 0; }
  .fid { flex-shrink: 0; color: var(--muted); font-size: 0.75rem; }
  .group h3 { margin-top: 0.5rem; }
  .role { margin: 0.5rem 0 0.3rem; color: var(--muted); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .cards { display: flex; flex-wrap: wrap; gap: 0.9rem; justify-content: center; margin-bottom: 0.4rem; }
  .credit { width: 7.5rem; display: flex; flex-direction: column; align-items: center; }
  .credit .face { width: 6rem; height: 6rem; padding: 0.35rem; border: 1px solid var(--line); border-radius: 10px; background: #fff; display: grid; place-items: center; cursor: pointer; }
  .credit.photo .face { border-radius: 50%; padding: 0; overflow: hidden; }
  .credit img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .credit.photo img { width: 100%; height: 100%; object-fit: cover; object-position: top; }
  .credit.none .face { background: var(--accent-bg); }
  .initials { font-size: 1.6rem; font-weight: 600; color: var(--accent); }
  .credit figcaption { text-align: center; font-size: 0.85rem; color: var(--text); padding-top: 0.35rem; }
  .credit .pic { display: block; margin: 0.1rem auto 0; font-size: 0.7rem; color: var(--muted); }
  .group + .group { border-top: 1px solid var(--line); margin-top: 0.75rem; }
  .group .hint { margin: 0 0 0.3rem; }
  :global(.sortbar) { display: flex; gap: 0.9rem; justify-content: center; align-items: baseline; flex-wrap: wrap; color: var(--muted); font-size: 0.85rem; margin: 0.3rem 0 0.6rem; }
  .serieslist { list-style: none; padding: 0; margin: 0 auto; max-width: 40rem; font-size: 0.9rem; }
  .serieslist li { display: grid; grid-template-columns: 2.2rem minmax(0, 1fr) 3.2rem 7rem; gap: 0.5rem; align-items: center; padding: 0.35rem 0.4rem; border-bottom: 1px solid var(--line); text-align: left; }
  .serieslist li.current { background: var(--accent-bg); font-weight: 600; }
  .serieslist li:not(.owned) .stitle { color: var(--muted); }
  .serieslist .ord { color: var(--muted); font-variant-numeric: tabular-nums; text-align: right; }
  .serieslist .syear { color: var(--muted); font-variant-numeric: tabular-nums; text-align: center; }
  .serieslist .shave { display: flex; gap: 0.2rem; flex-wrap: wrap; justify-content: flex-end; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; }
  .linkish:hover { text-decoration: underline; }
  .links { list-style: none; padding: 0; margin: 0; }
  .links li { display: flex; gap: 0.6rem; align-items: baseline; justify-content: center; padding: 0.25rem 0; }
  .links a { color: var(--accent); min-width: 9rem; }
  .links .host { font-size: 0.8rem; }
  :global(.app .hint) { color: var(--muted); font-size: 0.85rem; }
  :global(.app .empty) { padding: 2rem 1rem; color: var(--muted); text-align: center; }
  .empty h2 { color: var(--text); }

  :global(.app .badge) { display: inline-block; padding: 0 0.4rem; border-radius: 3px; font-size: 0.75rem; white-space: nowrap; background: var(--line); }
  :global(.app .badge.verified) { color: var(--ok); }
  :global(.app .badge.incomplete), :global(.app .badge.candidate), :global(.app .badge.duplicate) { color: var(--warn); }
  :global(.app .badge.unknown), :global(.app .badge.wrong_system) { color: var(--bad); }

  :global(.app .wide) { grid-column: 2 / 4; overflow: auto; padding: 0 0.75rem 1rem; }
  :global(.app .bar) { display: flex; gap: 0.5rem; align-items: center; padding: 0.5rem 0; flex-wrap: wrap; }
  :global(.app .bar input) { flex: 1; min-width: 12rem; }
  :global(.app table) { border-collapse: collapse; width: 100%; background: var(--panel); }
  :global(.app th), :global(.app td) { text-align: left; padding: 0.3rem 0.5rem; border-bottom: 1px solid var(--line); vertical-align: top; font-size: 0.9rem; }
  :global(.app th) { position: sticky; top: 0; background: var(--panel); color: var(--muted); font-weight: 600; font-size: 0.8rem; }
  :global(.app .num) { text-align: right; white-space: nowrap; }
  :global(.app .meta) { color: var(--muted); font-size: 0.85rem; }

  :global(.app .scrim) { position: fixed; inset: 0; background: #0008; display: grid; place-items: center; z-index: 10; }
  :global(.app .dialog) { width: min(40rem, calc(100vw - 2rem)); max-height: calc(100vh - 2rem); overflow-y: auto; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 1.25rem 1.5rem; }
  :global(.app .dialog h2) { font-size: 1.15rem; margin-bottom: 0.75rem; }
  :global(.app .setting) { padding: 0.6rem 0; border-top: 1px solid var(--line); }
  :global(.app .setting > label) { font-weight: 600; }
  :global(.app .setting .hint) { margin: 0.15rem 0 0.4rem; }
  :global(.app .pick) { display: flex; gap: 0.4rem; }
  :global(.app .pick input) { flex: 1; min-width: 0; }
  :global(.app .toggle) { display: flex; gap: 0.5rem; align-items: center; }
  :global(.app .setting label) { display: block; font-size: 0.85rem; margin-top: 0.4rem; }
  :global(.app .setting > input) { width: 100%; box-sizing: border-box; }
  :global(.app h3.sub) { color: var(--text); margin: 0; }
  :global(.app .err) { color: var(--bad); }
  :global(.app .actions) { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 0.75rem; }
  :global(.app button.primary) { background: var(--accent); border-color: var(--accent); color: white; }
  :global(.app .wide-dialog) { width: min(48rem, calc(100vw - 2rem)); }
  :global(.app .search-dialog) { width: min(64rem, calc(100vw - 2rem)); }
  :global(.app .ctitle) { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button.link { background: none; border: none; padding: 0 0.3rem; color: var(--accent); text-decoration: underline; cursor: pointer; font-size: 0.8rem; }
  :global(.app .badge.by_you), .ind.by_you, :global(.app .badge.derivative), .ind.derivative, :global(.app .badge.detected), .ind.detected { color: var(--accent); }
</style>
