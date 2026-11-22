<script>
  import GameTable from "./GameTable.svelte";
  import PictureGrid from "./PictureGrid.svelte";
  import ViewSwitch from "./ViewSwitch.svelte";
  import { statusLabel } from "./labels.js";
  import { versionRows } from "./listing.js";
  import { ui } from "./ui.svelte.js";
  import { invoke } from "@tauri-apps/api/core";
  import { versionName } from "./versions.js";

  // Details' Versions tab: the shown game's whole group, its original
  // first, every heading open to start with; as a list or pictures.
  let { row, lib, listSort, open, regionFlags = true } = $props();
  const vIsOpen = (key) => ui.vexpanded[key] ?? true;
  const vToggle = (key) => (ui.vexpanded[key] = !vIsOpen(key));
  let vrows = $derived(versionRows(row, lib.rows, listSort, vIsOpen));

  // The version Play always runs for this game (Play's "Always run"), if
  // one is kept; "Ask each time" forgets it.
  let always = $state(null);
  let choosable = $derived(!!row.group && lib.rows.filter((m) => m.group === row.group && m.playable).length > 1);
  $effect(() => {
    const id = row.id;
    void ui.alwaysRun;
    if (!row.group) return void (always = null);
    invoke("always_run", { rowId: id }).then((v) => row.id === id && (always = v), () => (always = null));
  });
  async function forget() {
    await invoke("set_always_run", { rowId: row.id, version: null }).catch(() => {});
    ui.alwaysRun++;
  }
</script>

<ViewSwitch bind:mode={ui.versionsMode} label="View versions as" centered />
{#if vrows.length <= 1}
  <p class="hint">No other version of this game in your library: other regions, editions, translations, hacks and patches show here.</p>
{:else}
  <p class="meta">{vrows.filter((v) => !v.head).length} versions · pick one to show it</p>
  {#if always}
    <p class="always">Play always runs <strong>{versionName(always)}</strong> <button onclick={forget}>Ask each time</button></p>
  {:else if choosable}
    <p class="hint">Play asks which version to start. Tick "Always run" there to keep one.</p>
  {/if}
{/if}
{#if ui.versionsMode === "list"}
  <GameTable rows={vrows} selected={row.id} platformCol={false} isOpen={vIsOpen} toggle={vToggle} {open} {statusLabel} {regionFlags} scroll={false} />
{:else}
  <PictureGrid rows={vrows} mode={ui.versionsMode} selected={row.id} isOpen={vIsOpen} {open} toggle={vToggle} {statusLabel} {regionFlags} scroll={false} />
{/if}

<style>
  .always { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; justify-content: center; margin: 0.2rem 0 0.6rem; font-size: 0.85rem; }
</style>
