<script>
  import PictureGrid from "./PictureGrid.svelte";
  import GameTable from "./GameTable.svelte";
  import SortButton from "./SortButton.svelte";
  import ViewSwitch from "./ViewSwitch.svelte";
  import { statusLabel } from "./labels.js";

  // The library's games: the filter, the view switch, the sort (for the
  // picture views; the list sorts by its headings) and the games as a list
  // or pictures. `table` is the view's instance (list or pictures), for
  // keyboard moves.
  let {
    lib,
    rows,
    games,
    platform,
    selected,
    isOpen,
    toggle,
    open,
    filter = $bindable(),
    listMode = $bindable(),
    listSort = $bindable(),
    table = $bindable(),
  } = $props();
</script>

<section class="list">
  <div class="viewbar">
    <input class="search" placeholder="Filter by title" bind:value={filter} />
    <ViewSwitch bind:mode={listMode} label="View" />
  </div>
  {#if listMode !== "list" && lib.rows.length}
    <p class="sortbar">
      Sort:
      <SortButton label="Title" key="title" bind:sort={listSort} />
      <SortButton label="Platform" key="platform" bind:sort={listSort} />
      <SortButton label="Year" key="year" bind:sort={listSort} />
      <SortButton label="Status" key="status" bind:sort={listSort} />
    </p>
  {/if}
  {#if lib.rows.length === 0}
    <div class="empty">
      <h2>Add your first folder</h2>
      <p>Download DATs on the DATs screen, then add a folder of ROMs with the button above.</p>
    </div>
  {:else if listMode !== "list"}
    <PictureGrid bind:this={table} {rows} mode={listMode} {selected} {isOpen} {open} {toggle} {statusLabel} />
  {:else}
    <GameTable bind:this={table} {rows} {selected} platformCol={platform === "*"} {isOpen} {toggle} {open} {statusLabel} bind:sort={listSort} />
  {/if}
  {#if lib.rows.length}
    <p class="foot">{platform === "" ? `${rows.filter((r) => r.main).length} not matched yet: Search can identify them` : `${rows.filter((r) => r.main).length} of ${games} games`}</p>
  {/if}
</section>

<style>
  .list { display: flex; flex-direction: column; min-height: 0; border-right: 1px solid var(--line); }
  .viewbar { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; margin: 0.5rem; }
  .viewbar .search { margin: 0; flex: 1; min-width: 8rem; }
  .list :global(.sortbar) { margin: 0 0.5rem 0.4rem; justify-content: flex-start; }
  .foot { margin: 0; padding: 0.3rem 0.6rem; font-size: 0.8rem; color: var(--muted); border-top: 1px solid var(--line); }
  .empty h2 { color: var(--text); }
</style>
