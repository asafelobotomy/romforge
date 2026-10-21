<script>
  import SortButton from "./SortButton.svelte";
  import { artSrc } from "./lib.js";
  import { listLabel } from "./listLabel.js";

  // Games as a table: title, platform logo (`platformCol`: All games —
  // each platform's release stays its own row, logo to the right of the
  // name), year, and what has been gathered; versions fold out under
  // headings. With `scroll` (the library) only the rows in view are drawn;
  // without it (Details' Versions) every row is. `sort` is bindable.
  // `picked`: the selected rows (else just `selected`, which has the focus).
  // `regionFlags`: version rows show region/language as emojis (Settings).
  // `icons`: system key -> platform icon file (same as the sidebar).
  let {
    rows,
    selected,
    picked = null,
    platformCol = true,
    isOpen,
    toggle,
    open,
    statusLabel,
    regionFlags = true,
    icons = {},
    iconStyle = "",
    sort = $bindable(),
    scroll = true,
  } = $props();

  // Row height in px: must match .tr below.
  const ROW = 32;
  const HEAD = 28;
  let scroller = $state(null);
  let scrollTop = $state(0);
  let viewH = $state(600);
  let failed = $state(new Set());
  const first = $derived(scroll ? Math.max(0, Math.floor(scrollTop / ROW) - 10) : 0);
  const last = $derived(scroll ? Math.min(rows.length, Math.ceil((scrollTop + viewH) / ROW) + 10) : rows.length);

  /// Scroll row `j` into view (keyboard moves).
  export function reveal(j) {
    if (!scroller || !scroll) return;
    const top = j * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW + HEAD > scroller.scrollTop + viewH) scroller.scrollTop = top + ROW + HEAD - viewH;
  }

  const mark = (s) =>
    s === "verified" ? "✓" : s === "by_you" || s === "derivative" ? "✎" : s === "detected" ? "◆" : s === "unknown" ? "?" : s === "missing" ? "○" : s === "patch" ? "✚" : "~";

  // Platform logo for a main row in All games (versions share the parent's).
  function platformIcon(r) {
    if (!platformCol || r.head || !r.main || !r.system) return null;
    const path = icons[r.system];
    if (path && !failed.has(path)) return { path, src: artSrc(path), name: r.system_name || r.system };
    return { path: null, src: null, name: r.system_name || r.system, short: r.system };
  }
</script>

<div class="table" class:scroll class:allsys={platformCol} bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
  <div class="thead">
    <span>{#if sort}<SortButton label="Title" key="title" bind:sort />{:else}Title{/if}</span>
    {#if platformCol}<span class="plat" title="Platform">{#if sort}<SortButton label="Plat." key="platform" bind:sort title="Sort by platform" />{:else}Plat.{/if}</span>{/if}
    <span class="yr">{#if sort}<SortButton label="Year" key="year" bind:sort title="Sort by release year (from Enrich)" />{:else}Year{/if}</span>
    <span class="ind">{#if sort}<SortButton label="ID" key="status" bind:sort title="Sort by status: verified first" />{:else}ID{/if}</span>
    <span class="ind" title="Metadata">Info</span>
    <span class="ind" title="Box art">Box</span>
    <span class="ind" title="Title screen">Title</span>
    <span class="ind" title="In-game screenshot">Snap</span>
  </div>
  <div style="height: {first * ROW}px"></div>
  {#each rows.slice(first, last) as r (r.id)}
    <div class="trow">
      {#if r.head}
        <button class="tr cat" style="padding-left: {0.7 + r.depth * 1.1}rem" onclick={() => toggle(r.key)} aria-expanded={isOpen(r.key)}>
          <span class="title">{isOpen(r.key) ? "▾" : "▸"} {r.label}<span class="nver"> · {r.n}</span></span>
          {#if platformCol}<span class="plat"></span>{/if}
        </button>
      {:else}
        {#if r.main && r.versions > 1 && r.group && toggle}
          <button
            class="twisty"
            onclick={() => toggle(r.group)}
            aria-expanded={isOpen(r.group)}
            title="{r.versions - 1} more: other regions, editions, translations, hacks and patches"
          >{isOpen(r.group) ? "▾" : "▸"}</button>
        {/if}
        {@const plat = platformIcon(r)}
        <button class="tr" class:sel={picked ? picked.has(r.id) : r.id === selected} class:focus={r.id === selected} class:version={!r.main} class:missing={r.status === "missing"} onclick={(e) => open(r.id, e)}>
          <span class="title" style={r.depth ? `padding-left: ${0.7 + r.depth * 1.1}rem` : undefined} title={r.main ? r.title : undefined}>{listLabel(r, regionFlags)}{#if r.main && r.versions > 1}<span class="nver"> · {r.versions}</span>{/if}</span>
          {#if platformCol}
            <span class="plat" title={plat?.name}>
              {#if plat?.src}
                <img class="plogo" class:logo={iconStyle === "logo"} src={plat.src} alt={plat.name} onerror={() => (failed = new Set(failed).add(plat.path))} />
              {:else if plat?.short}
                <span class="pbadge">{plat.short}</span>
              {/if}
            </span>
          {/if}
          <span class="yr">{r.year ?? ""}</span>
          <span class="ind {r.status}" title={statusLabel[r.status]}>{mark(r.status)}</span>
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

<style>
  .table.scroll { flex: 1; overflow-y: auto; }
  .thead, .tr { display: grid; grid-template-columns: minmax(0, 1fr) 2.8rem repeat(5, 2.4rem); align-items: center; gap: 0.25rem; padding: 0.3rem 0.6rem; }
  .allsys .thead, .allsys .tr { grid-template-columns: minmax(0, 1fr) 1.6rem 2.8rem repeat(5, 2.4rem); }
  .yr { color: var(--muted); font-size: 0.8rem; text-align: center; font-variant-numeric: tabular-nums; }
  .thead { position: sticky; top: 0; background: var(--panel); font-size: 0.75rem; color: var(--muted); border-bottom: 1px solid var(--line); text-align: left; z-index: 2; }
  .trow { position: relative; text-align: left; }
  .twisty { position: absolute; left: 0; top: 0; height: 32px; width: 1.2rem; padding: 0; border: none; border-radius: 0; color: var(--muted); z-index: 1; background: none; }
  .tr { width: 100%; height: 32px; box-sizing: border-box; padding-top: 0; padding-bottom: 0; border: none; border-radius: 0; border-bottom: 1px solid var(--line); text-align: left; background: none; user-select: none; }
  .tr:hover { background: var(--accent-bg); }
  .tr.sel { background: var(--accent-bg); box-shadow: inset 3px 0 var(--accent); }
  .tr.focus { outline: 1px dotted var(--accent); outline-offset: -1px; }
  .title { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tr .title { padding-left: 0.7rem; }
  .tr.version .title { padding-left: 1.8rem; }
  .tr.cat .title { color: var(--muted); font-size: 0.85rem; font-weight: 600; padding-left: 0; }
  .tr.missing .title { font-style: italic; color: var(--muted); }
  .plat { display: grid; place-items: center; min-width: 0; }
  .plogo { width: 1.35rem; height: 1.35rem; object-fit: contain; }
  .plogo.logo { padding: 1px; background: #f4f4f4; border-radius: 2px; box-sizing: border-box; }
  .pbadge { font-size: 0.55rem; font-weight: 700; color: var(--muted); letter-spacing: -0.02em; max-width: 100%; overflow: hidden; text-overflow: ellipsis; }
  .nver { color: var(--muted); font-size: 0.8rem; }
  .ind { text-align: center; color: var(--muted); }
  .ind.yes, .ind.verified { color: var(--ok); }
  .ind.incomplete, .ind.candidate, .ind.duplicate { color: var(--warn); }
  .ind.unknown, .ind.wrong_system { color: var(--bad); }
  .ind.by_you, .ind.derivative, .ind.detected { color: var(--accent); }
</style>
