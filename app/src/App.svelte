<script>
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { artSrc } from "./lib.js";
  import SettingsDialog from "./SettingsDialog.svelte";
  import EmulatorSettingsDialog from "./EmulatorSettingsDialog.svelte";
  import NowPlaying from "./NowPlaying.svelte";
  import CleanupDialog from "./CleanupDialog.svelte";
  import SearchDialog from "./SearchDialog.svelte";
  import NameDialog from "./NameDialog.svelte";
  import PatchDialog from "./PatchDialog.svelte";
  import ArtPicker from "./ArtPicker.svelte";
  import ScanPreview from "./ScanPreview.svelte";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import DatsView from "./DatsView.svelte";
  import BiosView from "./BiosView.svelte";
  import OrganizeView from "./OrganizeView.svelte";
  import HomeDialog from "./HomeDialog.svelte";
  import Toolbar from "./Toolbar.svelte";
  import Sidebar from "./Sidebar.svelte";
  import LibraryPane from "./LibraryPane.svelte";
  import DetailPane from "./DetailPane.svelte";
  import ImageViewer from "./ImageViewer.svelte";
  import JobsBar from "./JobsBar.svelte";
  import JobsDialog from "./JobsDialog.svelte";
  import Fullscreen from "./fullscreen/Fullscreen.svelte";
  import { fs, enter as enterFs, leave as leaveFs, toggle as toggleFs } from "./fullscreen/fs.svelte.js";
  import { desktopPad } from "./desktopKeys.js";
  import { startGame } from "./play.js";
  import { edit, run as queue, reportTo } from "./jobs.svelte.js";
  import { remembered, remember } from "./sort.js";
  import { statusLabel } from "./labels.js";
  import { storedView, storeView, libraryRows } from "./listing.js";
  import { refreshNote, scanNote, cleanupNote, enrichNote } from "./notes.js";
  import { ui } from "./ui.svelte.js";
  import { listen } from "@tauri-apps/api/event";
  import { padAction } from "./padnav.js";
  import { click, key as selectKey, targets } from "./selection.js";

  // Which screen: the library (default), DATs, BIOS, or organize.
  let view = $state("library");
  let error = $state("");
  // A quick action a dialog is waiting on (long jobs are in the queue).
  let busy = $state("");
  let note = $state("");
  let jobsOpen = $state(false);
  reportTo((e) => (error = e), (n) => (note = n));

  let lib = $state({ platforms: [], rows: [] });
  let platform = $state("*"); // "*" all, "" unidentified, else system key
  let filter = $state("");
  let selected = $state(null); // row id: the one Details shows
  // The rows picked for the toolbar's jobs, and where a Shift run starts.
  let picked = $state.raw(new Set());
  let anchor = null;
  let detail = $state(null);
  // Refresh needs a folder from Settings (or the RomForge folder).
  let canRefresh = $state(false);
  let previewer = $state(null);
  let previewOpen = $state(false);
  // Settings may set or clear the Source, or change the icon theme: look
  // again when it closes (at start, load() and readSettings() below do).
  let settingsWasOpen = false;
  $effect(() => {
    if (settingsOpen) settingsWasOpen = true;
    else if (settingsWasOpen) {
      settingsWasOpen = false;
      call("can_refresh", undefined, true).then((v) => (canRefresh = v ?? false));
      loadIcons();
      readSettings();
    }
  });
  // The sidebar's platform icons (system key -> file), fetched once from
  // libretro's RetroArch assets; a platform without one keeps its badge.
  let icons = $state({});
  let iconStyle = $state("");
  function loadIcons() {
    call("platform_icons", undefined, true).then((m) => {
      icons = m?.icons ?? {};
      iconStyle = m?.theme ?? "";
    });
  }
  // The RomForge folder (milestone 13); null until it exists. With it,
  // Rename & Cleanup (Import into the RomForge folder when there is one).
  let homeRoot = $state(null);
  let home;
  let homeOpen = $state(false);
  $effect(() => {
    home?.check();
  });

  // The dialogs and the DATs and organize screens keep their own state;
  // these are their instances (for openSettings() and the like) and what
  // the app needs to know of them: which dialog is open (Escape, arrow
  // keys) and the organize screen's undo prompt (reset on every screen
  // change).
  let settings = $state(null);
  let emulators = $state(null);
  let nowPlaying = $state(null);
  let playing = $state(null);
  let emuOpen = $state(false);
  let cleanup = $state(null);
  let search = $state(null);
  let patcher = $state(null);
  let patchOpen = $state(false);
  let artPicker = $state(null);
  let pickerOpen = $state(false);
  let dats = $state(null);
  let bios = $state(null);
  let settingsOpen = $state(false);
  let cleanOpen = $state(false);
  let searchOpen = $state(false);
  let naming = $state(null);
  let namingOpen = $state(false);
  let confirmUndo = $state(false);

  // `quiet`: a reload after an action, which keeps the action's error on
  // the status line (it still shows its own).
  async function call(cmd, args, quiet = false) {
    try {
      if (!quiet) error = "";
      return await invoke(cmd, args);
    } catch (e) {
      error = String(e);
      return null;
    }
  }

  // A long job: queued behind the one running (footer, Jobs dialog);
  // resolves to its result, or null (failed, stopped or removed).
  function run(label, cmd, args, opts) {
    return queue(label, cmd, args, opts);
  }

  // The library's generation the page has: the list comes again only when
  // it changed.
  let libGeneration = null;
  async function load() {
    const got = await call("library_view", { known: libGeneration }, true);
    if (got?.view) {
      lib = got.view;
      libGeneration = got.generation;
    }
    canRefresh = (await call("can_refresh", undefined, true)) ?? false;
    loadIcons();
    if (selected && !lib.rows.some((r) => r.id === selected)) selected = null;
    const ids = new Set(lib.rows.map((r) => r.id));
    if ([...picked].some((id) => !ids.has(id))) picked = new Set([...picked].filter((id) => ids.has(id)));
    if (selected) await open(selected, true);
  }

  // A patch row's Apply: the patch dialog for the game it is for, with
  // this patch picked.
  async function applyPatchRow() {
    const p = detail.patches[0];
    const base = lib.rows.find((r) => r.id === row.base_id);
    const d = base && (await call("game_detail", { id: base.id }));
    if (d) await patcher.openPatch(base, d, { path: p.path, relation: p.relation });
  }

  async function open(id, quiet = false) {
    selected = id;
    const d = await call("game_detail", { id }, quiet);
    // Pictures come as file paths; the page loads them itself, as shown.
    if (d) {
      d.art = Object.fromEntries(Object.entries(d.art ?? {}).map(([k, p]) => [k, artSrc(p)]));
      for (const c of d.credits ?? []) c.picture = artSrc(c.picture);
    }
    // Another row was picked while this one loaded: its details win.
    if (selected !== id) return;
    if (d?.row?.id !== detail?.row?.id) ui.dims = {};
    detail = d;
  }

  function show(v) {
    view = v;
    error = "";
    note = "";
    confirmUndo = false;
    if (v === "dats") dats.loadDats();
    if (v === "bios") bios.loadBios();
  }

  async function refresh() {
    const r = await run("Rescanning your folders…", "refresh", {}, { key: "refresh" });
    if (r) note = refreshNote(r);
    await load();
  }

  // Scan folder: any folder, without making it a setting, except the
  // first one scanned while no Source is set, which becomes the Source.
  async function chooseFolder(title) {
    const f = await pickFolder({ directory: true, multiple: false, title });
    return typeof f === "string" ? f : null;
  }

  function scanned(r) {
    note = scanNote(r);
    return load();
  }

  async function scanFolder() {
    const dir = await chooseFolder("A folder to scan");
    if (!dir) return;
    const r = await run(`Scanning ${dir}…`, "scan_folder", { dir });
    if (r) await scanned(r);
  }

  async function scanDryRun() {
    const dir = await chooseFolder("A folder to scan (dry run: nothing is kept until you say so)");
    if (dir) await previewer.show(dir);
  }

  // Rename & Cleanup [apply now]: the whole job without showing the plan.
  // The primary toolbar button opens the dry-run dialog instead.
  async function renameCleanup() {
    const r = await run(`Renaming and cleaning up ${chosen.length} games…`, "cleanup_apply", { only: chosen, from: null });
    if (r) note = cleanupNote(r);
    await load();
  }

  async function enrich() {
    const r = await run(`Fetching metadata and art for ${chosen.length} games…`, "enrich", { system: null, only: chosen });
    if (r) note = enrichNote(r);
    await load();
  }

  // Undo a title picked in Search for the selected row.
  async function forget() {
    error = "";
    const onWait = (why) => (note = `Forget ${why}`);
    await edit("Forgetting the title…", "forget_identity", { rowId: selected }, { onWait }).then(
      () => (note = "Forgotten: the files show as unknown again"),
      (e) => (error = String(e)),
    );
    await load();
  }

  const playRow = (r, art) => startGame(r, art, { call, nowPlaying, setNote: (n) => (note = n) });
  function play() {
    const r = lib.rows.find((x) => x.id === selected);
    const mine = detail?.row?.id === r?.id ? detail.art : {};
    playRow(r, { box3d: mine.box3d, boxart: mine.boxart });
  }

  // Details: empty media boxes only when Settings says so.
  let showEmpty = $state(false);
  let regionFlags = $state(true);

  // The Series tab's sort and the Versions tab's view, kept for next time.
  $effect(() => remember("series", ui.seriesSort));
  $effect(() => storeView("versions", ui.versionsMode));

  // Which groups and headings of the list are open.
  let expanded = $state({});
  let listSort = $state(remembered("games", { key: "title", dir: 1 }));
  let listMode = $state(storedView("library"));
  $effect(() => storeView("library", listMode));
  $effect(() => remember("games", listSort));
  // Headings start open; groups start closed.
  const isOpen = (key) => expanded[key] ?? key.includes("|");
  function toggle(key) {
    expanded[key] = !isOpen(key);
  }
  let games = $derived(lib.rows.filter((r) => r.main && !r.unmatched).length);
  let rows = $derived(libraryRows(lib.rows, platform, filter, listSort, isOpen));
  let row = $derived(lib.rows.find((r) => r.id === selected) ?? null);
  // What the toolbar's jobs act on.
  let chosen = $derived(targets(picked, rows, lib.rows, isOpen));

  // A new selection (from a click or a key): Details follows its focus.
  function select(s) {
    picked = s.picked;
    anchor = s.anchor;
    if (s.focus != null && s.focus !== selected) open(s.focus);
    const j = rows.findIndex((r) => r.id === s.focus);
    if (j >= 0) table?.reveal(j);
  }
  // A version picked from Details unfolds its game, so the list shows it.
  function pick(id, e) {
    const r = lib.rows.find((x) => x.id === id);
    if (r?.group && !r.main && !isOpen(r.group)) expanded[r.group] = true;
    select(click({ picked, anchor, focus: selected }, rows, id, e));
  }

  let table = $state(null);

  // The selection keys (selection.js) when no text field has focus.
  function listKeys(e) {
    const dialog = settingsOpen || emuOpen || cleanOpen || searchOpen || namingOpen || patchOpen || pickerOpen || previewOpen || homeOpen || jobsOpen || ui.viewer;
    if (view !== "library" || dialog) return;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
    const s = selectKey({ picked, anchor, focus: selected }, rows, e);
    if (!s) return;
    e.preventDefault();
    select(s);
  }

  // Game pads: the fullscreen mode's when it is on, else the library's.
  const padLibrary = (a) => desktopPad(a, { view, rows, row, busy, platforms: lib.platforms, platform, setPlatform: (p) => (platform = p), pick, play, listKeys, settings, emulators });
  const unPad = listen("pad", (e) => {
    const p = e.payload;
    if (p.kind === "connected") note = `Game pad connected: ${p.name}`;
    else if (p.kind === "disconnected") note = `Game pad disconnected: ${p.name}`;
    else if (!playing && fs.on) ((fs.padSeen = true), fs.pad?.(p.action));
    else if (!playing) padAction(p.action, padLibrary);
  });
  onDestroy(() => unPad.then((f) => f()));

  load();
  // Details' empty boxes: read at start and whenever Settings closes (the
  // effect at the top).
  function readSettings() {
    call("get_settings", undefined, true).then((s) => {
      if (!s) return;
      showEmpty = s.show_empty_media;
      regionFlags = s.region_flags !== false;
    });
  }
  readSettings();
  // Straight into the fullscreen mode: `--fullscreen`, or Settings says so.
  Promise.all([call("start_options_now", undefined, true), call("get_settings", undefined, true)]).then(([o, s]) => (o?.fullscreen || s?.start_fullscreen) && enterFs());
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "F11") (e.preventDefault(), toggleFs());
    else if (fs.on) return;
    else if (e.key === "Escape" && ui.viewer) ui.viewer = null;
    else if (e.key === "Escape" && jobsOpen) jobsOpen = false;
    else if (e.key === "Escape" && settingsOpen) settingsOpen = false;
    else if (e.key === "Escape" && emuOpen) emuOpen = false;
    else if (e.key === "Escape" && cleanOpen) cleanOpen = false;
    else if (e.key === "Escape" && searchOpen) searchOpen = false;
    else if (e.key === "Escape" && namingOpen) namingOpen = false;
    else if (e.key === "Escape" && patchOpen) patchOpen = false;
    else if (e.key === "Escape" && pickerOpen && !busy) pickerOpen = false;
    else if (e.key === "Escape" && previewOpen) previewOpen = false;
    else if (e.key === "Escape" && homeOpen) homeOpen = false;
    else listKeys(e);
  }}
/>

<div class="app">
  <Toolbar {busy} {playing} {canRefresh} {homeRoot} {row} {chosen} {settings} {emulators} {cleanup} {search} {naming} {refresh} {scanFolder} {scanDryRun} {enrich} {renameCleanup} {play} fullscreen={enterFs} />

  <SettingsDialog bind:this={settings} bind:open={settingsOpen} bind:note bind:showEmpty bind:regionFlags bind:homeRoot {call} {load} />
  <EmulatorSettingsDialog bind:this={emulators} bind:open={emuOpen} bind:note {call} />

  <CleanupDialog bind:this={cleanup} bind:open={cleanOpen} bind:note {homeRoot} {call} {load} />

  <HomeDialog bind:this={home} bind:open={homeOpen} bind:root={homeRoot} bind:note {load} />

  <NameDialog bind:this={naming} bind:open={namingOpen} bind:note {call} {load} />
  <SearchDialog bind:this={search} bind:open={searchOpen} bind:busy bind:note {selected} {call} {load} />

  <PatchDialog bind:this={patcher} bind:open={patchOpen} bind:note {call} {load} />
  <ScanPreview bind:this={previewer} bind:open={previewOpen} {statusLabel} {scanned} />
  <ArtPicker bind:this={artPicker} bind:open={pickerOpen} bind:busy bind:note view={(kind, src) => (ui.viewer = { kind, src })} reload={(id) => open(id)} />

  <ImageViewer {detail} {row} />


  <NowPlaying bind:this={nowPlaying} bind:playing bind:note bind:error stayFullscreen={fs.on} ended={() => selected && open(selected, true)} />

  <div class="body">
    <Sidebar {lib} {games} {view} bind:platform {icons} {iconStyle} {show} />

    {#if view === "library"}
      <LibraryPane {lib} {rows} {games} {platform} {selected} {picked} chosen={chosen.length} {isOpen} {toggle} open={pick} {regionFlags} {icons} {iconStyle} bind:filter bind:listMode bind:listSort bind:table />

      <DetailPane {row} {detail} {lib} {busy} {listSort} {showEmpty} {regionFlags} open={pick} {forget} {applyPatchRow} {patcher} {artPicker} bind:note bind:error />
    {/if}
    <DatsView bind:this={dats} active={view === "dats"} {busy} bind:note bind:error {call} {run} {load} />
    <BiosView bind:this={bios} active={view === "bios"} {busy} bind:note bind:error {run} />
    <OrganizeView active={view === "organize"} {busy} bind:note bind:confirmUndo {homeRoot} openImport={() => cleanup.openCleanup()} {run} {load} />
  </div>

  <JobsBar note={busy || note} error={busy ? "" : error} openJobs={() => (jobsOpen = true)} />
  <JobsDialog bind:open={jobsOpen} />
</div>
<!-- Outside .app: none of the desktop's shared styles reach it. -->
{#if fs.on}<Fullscreen {lib} {icons} {iconStyle} {playing} play={playRow} desktop={(id) => (leaveFs(), (view = "library"), pick(id))} />{/if}

<style>
  .app { display: flex; flex-direction: column; height: 100vh; }

  .body { flex: 1; display: grid; grid-template-columns: 6.5rem minmax(0, 1.2fr) minmax(0, 1fr); min-height: 0; }
</style>
