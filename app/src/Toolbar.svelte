<script>
  import SplitButton from "./SplitButton.svelte";
  import Wordmark from "./Wordmark.svelte";

  // The buttons along the top; the jobs themselves are App's, and the
  // dialogs' instances open theirs. Search, Enrich, Rename & Cleanup and
  // Convert take only the games selected in the list (`chosen`, row ids);
  // Refresh and Scan folder work on folders.
  let { busy, playing, canRefresh, homeRoot, row, chosen, settings, emulators, cleanup, search, naming, refresh, scanFolder, scanDryRun, enrich, renameCleanup, play } = $props();
  const n = $derived(chosen.length);
  const games = $derived(n === 1 ? "the selected game" : `the ${n} selected games`);
  const pickFirst = "Select games in the list first (Ctrl+A selects everything listed)";
</script>

<header class="toolbar">
  <span class="brand"><Wordmark height="1.75rem" /></span>
  <button class="tool" onclick={refresh} disabled={!!busy || !canRefresh} title={canRefresh ? "Rescan the Source and Library folders from Settings (and the RomForge folder)" : "Set a Source folder first: Scan folder makes the first folder you scan your Source"}>
    <span class="glyph">⟳</span>Refresh
  </button>
  <SplitButton
    glyph="＋"
    label="Scan folder"
    title="Scan any folder (it doesn't become a setting; the very first one becomes your Source)"
    disabled={!!busy}
    onclick={scanFolder}
    items={[{ label: "Scan folder [dry run]", title: "See what a folder holds first, then scan it or not", onclick: scanDryRun }]}
  />
  <SplitButton
    glyph="⌕"
    label="Search"
    title={n ? `Identify ${games} one by one: DAT title suggestions or your own title, and settle Wikidata matches` : pickFirst}
    disabled={!!busy || !n}
    onclick={() => search.openSearch(chosen)}
    items={[{ label: "Name as suggested…", title: "Name the selected files by their suggestions at once (translations as translations); you see the list first", onclick: () => naming.openNaming(chosen) }]}
  />
  <button class="tool" onclick={enrich} disabled={!!busy || !n} title={n ? `Fetch metadata and art for ${games}` : pickFirst}>
    <span class="glyph">✦</span>Enrich
  </button>
  <SplitButton
    glyph="✎"
    label="Rename & Cleanup"
    title={n ? `Rename, repack and ${homeRoot ? "import" : "move"} ${games} now (those identified), as Settings says (originals to the trash, kept, or replaced in place); Undo is in the dry run` : pickFirst}
    disabled={!!busy || !n}
    onclick={renameCleanup}
    items={[{ label: "Rename & Cleanup [dry run]", title: "See the plan first, then do it or not (Undo and Empty trash are here too)", onclick: () => cleanup.openCleanup(null, "cleanup", chosen) }]}
  />
  <button class="tool" onclick={() => cleanup.openCleanup(null, "convert", chosen)} disabled={!!busy || !n} title={n ? `Turn the disc images of ${games} into CHD where they are (shows what it would do first)` : pickFirst}>
    <span class="glyph">⇲</span>Convert
  </button>
  <span class="spacer"></span>
  <button class="tool" onclick={() => settings.openSettings()} disabled={!!busy} title="Source and Library folders, cleanup options">
    <span class="glyph">⚙</span>Settings
  </button>
  <button class="tool" onclick={() => emulators.openEmulatorSettings()} disabled={!!busy} title="Settings for every emulator at once, and for each installed one">
    <span class="glyph">⛭</span>Emulator Settings
  </button>
  <button class="tool" onclick={play} disabled={!row || !row.playable || !!busy || !!playing} title={playing ? "A game is running: stop it first" : "Start the selected game"}>
    <span class="glyph">▶</span>Play
  </button>
</header>

<style>
  .brand { align-self: center; margin: 0 0.75rem 0 0.25rem; }
  .toolbar { display: flex; gap: 0.25rem; padding: 0.4rem 0.75rem; border-bottom: 1px solid var(--line); background: var(--panel); }
  .tool { display: flex; flex-direction: column; align-items: center; border: none; padding: 0.2rem 0.6rem; font-size: 0.8rem; gap: 0.1rem; }
  .tool:hover:not(:disabled) { background: var(--accent-bg); }
  .glyph { font-size: 1.25rem; line-height: 1.2rem; }
</style>
