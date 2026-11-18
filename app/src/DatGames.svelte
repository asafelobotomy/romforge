<script>
  import SortButton from "./SortButton.svelte";

  // A DAT's games: only the rows in view are drawn (MAME's list has more
  // than 40,000), between two spacers that keep the scrollbar true.
  let { games, sort = $bindable() } = $props();

  // Row height in px: must match `tbody tr` below.
  const ROW = 30;
  let scrollTop = $state(0);
  let viewH = $state(600);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  const last = $derived(Math.min(games.length, Math.ceil((scrollTop + viewH) / ROW) + 10));
</script>

<div class="scroller" bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
  <table>
    <thead><tr>
      <th><SortButton label="Have" key="have" bind:sort title="Sort by have / missing" /></th>
      <th><SortButton label="Game" key="name" bind:sort /></th>
      <th class="num"><SortButton label="ROMs" key="roms" bind:sort /></th>
    </tr></thead>
    <tbody>
      {#if first}<tr class="spacer" style="height: {first * ROW}px"><td colspan="3"></td></tr>{/if}
      {#each games.slice(first, last) as g (g.game_id)}
        <tr>
          <td><span class="badge {g.have ? 'verified' : 'unknown'}">{g.have ? "have" : "missing"}</span></td>
          <td class="name" title={g.name}>{g.name}</td>
          <td class="num">{g.roms_have}/{g.roms_total}</td>
        </tr>
      {/each}
      {#if last < games.length}<tr class="spacer" style="height: {(games.length - last) * ROW}px"><td colspan="3"></td></tr>{/if}
    </tbody>
  </table>
</div>

<style>
  .scroller { max-height: 70vh; overflow-y: auto; }
  table { width: 100%; }
  thead th { position: sticky; top: 0; background: var(--bg); z-index: 1; }
  tbody tr:not(.spacer) { height: 30px; }
  .name { max-width: 0; width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .spacer td { padding: 0; border: none; }
</style>
