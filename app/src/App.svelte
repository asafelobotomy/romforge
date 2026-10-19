<script>
  import { invoke } from "@tauri-apps/api/core";
  import { artSrc } from "./lib.js";
  import SettingsDialog from "./SettingsDialog.svelte";
  import EmulatorSettingsDialog from "./EmulatorSettingsDialog.svelte";
  import CleanupDialog from "./CleanupDialog.svelte";
  import SearchDialog from "./SearchDialog.svelte";
  import PatchDialog from "./PatchDialog.svelte";
  import ArtPicker from "./ArtPicker.svelte";
  import ScanPreview from "./ScanPreview.svelte";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import DatsView from "./DatsView.svelte";
  import OrganizeView from "./OrganizeView.svelte";
  import HomeDialog from "./HomeDialog.svelte";
  import Toolbar from "./Toolbar.svelte";
  import Sidebar from "./Sidebar.svelte";
  import LibraryPane from "./LibraryPane.svelte";
  import DetailPane from "./DetailPane.svelte";
  import ImageViewer from "./ImageViewer.svelte";
  import { remembered, remember } from "./sort.js";
  import { statusLabel } from "./labels.js";
  import { storedView, storeView, libraryRows } from "./listing.js";
  import { refreshNote, scanNote, cleanupNote, enrichNote } from "./notes.js";
  import { ui } from "./ui.svelte.js";

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
  let emuOpen = $state(false);
  let cleanup = $state(null);
  let search = $state(null);
  let patcher = $state(null);
  let patchOpen = $state(false);
  let artPicker = $state(null);
  let pickerOpen = $state(false);
  let dats = $state(null);
  let settingsOpen = $state(false);
  let cleanOpen = $state(false);
  let searchOpen = $state(false);
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

  async function run(label, cmd, args) {
    busy = label;
    note = "";
    const r = await call(cmd, args);
    busy = "";
    return r;
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
  }

  async function refresh() {
    const r = await run("Rescanning your folders…", "refresh", {});
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

  // Rename & Cleanup: the whole job (rename, repack, move or replace, as
  // Settings says) straight away; its dry run shows the plan first.
  async function renameCleanup() {
    const r = await run("Renaming and cleaning up…", "cleanup_apply", { only: null, from: null });
    if (r) note = cleanupNote(r);
    await load();
  }

  async function enrich() {
    const system = platform === "*" || platform === "" ? null : platform;
    const r = await run(`Fetching metadata and art${system ? ` for ${system}` : ""}…`, "enrich", { system });
    if (r) note = enrichNote(r);
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

  // Details: empty media boxes only when Settings says so.
  let showEmpty = $state(false);

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

  let table = $state(null);

  // Up/Down move through the list when no text field has focus.
  function listKeys(e) {
    const dialog = settingsOpen || emuOpen || cleanOpen || searchOpen || patchOpen || pickerOpen || previewOpen || homeOpen || ui.viewer;
    if (view !== "library" || dialog || !["ArrowUp", "ArrowDown"].includes(e.key)) return;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
    e.preventDefault();
    const i = rows.findIndex((r) => r.id === selected);
    let j = Math.max(0, Math.min(rows.length - 1, e.key === "ArrowDown" ? i + 1 : i < 0 ? 0 : i - 1));
    // Headings are passed over.
    const step = e.key === "ArrowDown" ? 1 : -1;
    while (rows[j]?.head && rows[j + step]) j += step;
    if (!rows[j] || rows[j].head) return;
    open(rows[j].id);
    table?.reveal(j);
  }

  load();
  // The logo Settings picks (and Details' empty boxes): read at start and
  // whenever Settings closes (the effect at the top).
  let logo = $state("");
  function readSettings() {
    call("get_settings", undefined, true).then((s) => {
      if (!s) return;
      showEmpty = s.show_empty_media;
      logo = s.logo ?? "";
    });
  }
  readSettings();
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && ui.viewer) ui.viewer = null;
    else if (e.key === "Escape" && settingsOpen) settingsOpen = false;
    else if (e.key === "Escape" && emuOpen) emuOpen = false;
    else if (e.key === "Escape" && cleanOpen) cleanOpen = false;
    else if (e.key === "Escape" && searchOpen) searchOpen = false;
    else if (e.key === "Escape" && patchOpen) patchOpen = false;
    else if (e.key === "Escape" && pickerOpen && !busy) pickerOpen = false;
    else listKeys(e);
  }}
/>

<div class="app">
  <Toolbar {logo} {busy} {canRefresh} {homeRoot} {row} {settings} {emulators} {cleanup} {search} {refresh} {scanFolder} {scanDryRun} {enrich} {renameCleanup} {play} />

  <SettingsDialog bind:this={settings} bind:open={settingsOpen} bind:note bind:showEmpty bind:homeRoot {call} {load} />
  <EmulatorSettingsDialog bind:this={emulators} bind:open={emuOpen} bind:note {call} />

  <CleanupDialog bind:this={cleanup} bind:open={cleanOpen} bind:busy bind:note {selected} {homeRoot} {call} {load} />

  <HomeDialog bind:this={home} bind:open={homeOpen} bind:root={homeRoot} bind:note {load} />

  <SearchDialog bind:this={search} bind:open={searchOpen} bind:busy bind:note {selected} {call} {load} />

  <PatchDialog bind:this={patcher} bind:open={patchOpen} bind:busy bind:note {call} {load} />
  <ScanPreview bind:this={previewer} bind:open={previewOpen} bind:busy {statusLabel} {scanned} />
  <ArtPicker bind:this={artPicker} bind:open={pickerOpen} bind:busy bind:note view={() => (ui.viewer = { kind: "hero", src: detail.art.hero })} reload={(id) => open(id)} />

  <ImageViewer {detail} {row} />


  <!-- One fixed-height line, so the controls below never move under the cursor. -->
  <p class="status" class:error={!!error && !busy} title={error || busy || note}>{busy || error || note}</p>

  <div class="body">
    <Sidebar {lib} {games} {view} bind:platform {icons} {iconStyle} {show} />

    {#if view === "library"}
      <LibraryPane {lib} {rows} {games} {platform} {selected} {isOpen} {toggle} {open} bind:filter bind:listMode bind:listSort bind:table />

      <DetailPane {row} {detail} {lib} {busy} {listSort} {showEmpty} {open} {forget} {applyPatchRow} {patcher} {artPicker} bind:error />
    {/if}
    <DatsView bind:this={dats} active={view === "dats"} {busy} bind:note bind:error {call} {run} {load} />
    <OrganizeView active={view === "organize"} {busy} bind:note bind:confirmUndo {homeRoot} openImport={() => cleanup.openCleanup()} {run} {load} />
  </div>
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100vh; }
  .status { margin: 0; padding: 0.2rem 0.75rem; min-height: 1.4em; color: var(--accent); font-size: 0.85rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .status.error { color: var(--bad); }

  .body { flex: 1; display: grid; grid-template-columns: 6.5rem minmax(0, 1.2fr) minmax(0, 1fr); min-height: 0; }
</style>
