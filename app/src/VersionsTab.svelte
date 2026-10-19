<script>
  import GameTable from "./GameTable.svelte";
  import PictureGrid from "./PictureGrid.svelte";
  import ViewSwitch from "./ViewSwitch.svelte";
  import { statusLabel } from "./labels.js";
  import { versionRows } from "./listing.js";
  import { ui } from "./ui.svelte.js";

  // Details' Versions tab: the shown game's whole group, its original
  // first, every heading open to start with; as a list or pictures.
  let { row, lib, listSort, open } = $props();
  const vIsOpen = (key) => ui.vexpanded[key] ?? true;
  const vToggle = (key) => (ui.vexpanded[key] = !vIsOpen(key));
  let vrows = $derived(versionRows(row, lib.rows, listSort, vIsOpen));
</script>

<ViewSwitch bind:mode={ui.versionsMode} label="View versions as" centered />
{#if vrows.length <= 1}
  <p class="hint">No other version of this game in your library: other regions, editions, translations, hacks and patches show here.</p>
{:else}
  <p class="meta">{vrows.filter((v) => !v.head).length} versions in your library · pick one to show it</p>
{/if}
{#if ui.versionsMode === "list"}
  <GameTable rows={vrows} selected={row.id} platformCol={false} isOpen={vIsOpen} toggle={vToggle} {open} {statusLabel} scroll={false} />
{:else}
  <PictureGrid rows={vrows} mode={ui.versionsMode} selected={row.id} isOpen={vIsOpen} {open} toggle={vToggle} {statusLabel} scroll={false} />
{/if}
