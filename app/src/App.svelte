<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { size } from "./lib.js";
  import SettingsDialog from "./SettingsDialog.svelte";
  import CleanupDialog from "./CleanupDialog.svelte";
  import SearchDialog from "./SearchDialog.svelte";
  import DatsView from "./DatsView.svelte";
  import OrganizeView from "./OrganizeView.svelte";

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

  // The dialogs and the DATs and organize screens keep their own state;
  // these are their instances (for openSettings() and the like) and what
  // the app needs to know of them: which dialog is open (Escape, arrow
  // keys) and the organize screen's undo prompt (reset on every screen
  // change).
  let settings = $state(null);
  let cleanup = $state(null);
  let search = $state(null);
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
    if (r) note = `Rescanned ${r.folders} folders, ${r.entries} entries` + (r.problems.length ? `; ${r.problems.length} problems: ${r.problems.join("; ")}` : "");
    await load();
  }

  async function addFolder() {
    const r = await run("Scanning…", "scan", { dir: addDir });
    if (r) {
      const problems = [
        ...r.errors.map(([path, member, e]) => `${member ? `${path}#${member}` : path}: ${e}`),
        ...r.sheet_problems.map(([path, e]) => `${path}: ${e}`),
      ];
      note = `Scanned ${r.hashed} entries in ${r.root}` + (problems.length ? `. ${problems.length} problem${problems.length === 1 ? "" : "s"}: ${problems.slice(0, 3).join("; ")}${problems.length > 3 ? "; …" : ""}` : "");
      adding = false;
      addDir = "";
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
    const r = await run(`Starting ${row.title}…`, "launch", { rowId: row.id });
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
    <button class="tool" onclick={() => search.openSearch()} disabled={!!busy} title="Identify games one by one: DAT title suggestions or your own title, and settle Wikidata matches">
      <span class="glyph">⌕</span>Search
    </button>
    <button class="tool" onclick={enrich} disabled={!!busy} title="Fetch metadata and art for the games you have on this platform">
      <span class="glyph">✦</span>Enrich
    </button>
    <button class="tool" onclick={() => cleanup.openCleanup()} disabled={!!busy} title="Repack identified games from the Source as one file each, named by the DAT, with art and details inside">
      <span class="glyph">✎</span>Cleanup and rename
    </button>
    <span class="spacer"></span>
    <button class="tool" onclick={() => settings.openSettings()} disabled={!!busy} title="Source and Library folders, cleanup options">
      <span class="glyph">⚙</span>Settings
    </button>
    <button class="tool" onclick={play} disabled={!row || !row.playable || !!busy} title="Start the selected game">
      <span class="glyph">▶</span>Play
    </button>
  </header>

  <SettingsDialog bind:this={settings} bind:open={settingsOpen} bind:note bind:showEmpty {call} />

  <CleanupDialog bind:this={cleanup} bind:open={cleanOpen} bind:busy bind:note {selected} {call} {load} />

  <SearchDialog bind:this={search} bind:open={searchOpen} bind:busy bind:note {selected} {call} {load} />

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
              {#if row.status === "by_you" || row.status === "derivative"}
                <button class="link" onclick={forget} disabled={!!busy} title="Undo naming these files: they show as unknown again">Forget</button>
              {/if}
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
    {/if}
    <DatsView bind:this={dats} active={view === "dats"} {busy} bind:note bind:error {call} {run} {load} />
    <OrganizeView active={view === "organize"} {busy} bind:note bind:confirmUndo {run} {load} />
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
  .thead, .tr { display: grid; grid-template-columns: minmax(0, 1fr) auto repeat(5, 2.4rem); align-items: center; gap: 0.25rem; padding: 0.3rem 0.6rem; }
  .thead { position: sticky; top: 0; background: var(--panel); font-size: 0.75rem; color: var(--muted); border-bottom: 1px solid var(--line); }
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
  :global(.app .facts) { display: grid; grid-template-columns: auto 1fr; gap: 0.2rem 0.75rem; margin: 0; align-content: start; font-size: 0.9rem; }
  .facts.wide { max-width: 40rem; }
  :global(.app dt) { color: var(--muted); }
  :global(.app dd) { margin: 0; }
  .detail h3 { font-size: 0.95rem; margin: 1rem 0 0.4rem; }
  .detail h3 .src { font-weight: 400; font-size: 0.8rem; }
  :global(.app .desc) { line-height: 1.55; margin: 0; }
  .gallery { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: flex-end; }
  .media .shot img { max-height: 10rem; max-width: min(100%, 18rem); background: #0002; }
  .media.empty span { height: 6rem; width: 8rem; }
  .viewer { cursor: zoom-out; }
  .viewer figure { display: flex; flex-direction: column; align-items: center; }
  .viewer img { max-width: 92vw; max-height: 82vh; object-fit: contain; box-shadow: 0 8px 30px #0008; }
  .viewer img.pixels { image-rendering: pixelated; max-width: none; max-height: none; }
  .viewer figcaption { color: #ddd; font-size: 0.85rem; padding-top: 0.5rem; }
  :global(.app .files) { list-style: none; padding: 0; margin: 0; font-size: 0.8rem; }
  :global(.app .files li) { display: flex; gap: 0.5rem; align-items: baseline; padding: 0.15rem 0; overflow: hidden; }
  /* rtl on the box and <bdi> inside: long paths are cut at the start, the
     file name stays visible, and the text itself keeps its order */
  :global(.app .fpath) { font-family: ui-monospace, monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; min-width: 0; }
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
  :global(.app .badge.by_you), .ind.by_you, :global(.app .badge.derivative), .ind.derivative { color: var(--accent); }
</style>
