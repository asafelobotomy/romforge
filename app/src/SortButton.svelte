<script>
  // A column heading (or a choice in a sort bar) that sorts its list:
  // click to sort by it, click again to reverse.
  let { label, key, sort = $bindable(), title = "" } = $props();
  const active = $derived(sort?.key === key);
  function click() {
    sort = active ? { key, dir: -sort.dir } : { key, dir: 1 };
  }
</script>

<button
  class="sortby"
  class:active
  onclick={click}
  title={title || `Sort by ${label.toLowerCase()}`}
  aria-label={active ? `${label}, sorted ${sort.dir > 0 ? "ascending" : "descending"}` : `Sort by ${label.toLowerCase()}`}
>{label}<span class="arrow" aria-hidden="true">{active ? (sort.dir > 0 ? " ▲" : " ▼") : ""}</span></button>

<style>
  .sortby { border: none; background: none; padding: 0; font: inherit; color: inherit; cursor: pointer; text-align: inherit; white-space: nowrap; }
  .sortby:hover { color: var(--text); text-decoration: underline; }
  .sortby.active { color: var(--text); }
  .arrow { font-size: 0.7em; }
</style>
