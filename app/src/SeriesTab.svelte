<script>
  import SortButton from "./SortButton.svelte";
  import { seriesParts } from "./series.js";
  import { ui } from "./ui.svelte.js";

  // Details' Series tab: the game's series (or franchise), the ones you
  // have linked to them.
  let { detail, lib, open } = $props();
  let f = $derived(detail.fields ?? {});
</script>

{#if detail.series.length}
  {@const sp = seriesParts(detail.series, ui.seriesSort)}
  <p class="meta">
    {f.series ? `${f.series} · from Wikidata` : `${f.franchise} franchise · the games romforge has metadata for`}
    · {sp.total} game{sp.total === 1 ? "" : "s"} · you have {sp.have}
  </p>
  <p class="sortbar">
    Sort:
    <SortButton label="Series order" key="order" bind:sort={ui.seriesSort} title="Numbered entries by their number, the rest by year" />
    <SortButton label="Year" key="year" bind:sort={ui.seriesSort} />
    <SortButton label="Title" key="title" bind:sort={ui.seriesSort} />
    <SortButton label="Yours first" key="have" bind:sort={ui.seriesSort} title="The games you have first" />
  </p>
  {#each [[sp.numbered.length ? "Main series" : "", sp.numbered], [sp.numbered.length ? "Other games in the series" : "Games", sp.others]] as [heading, es]}
    {#if es.length}
      {#if heading}<h3>{heading}<span class="nver"> · {es.length}</span></h3>{/if}
      <ol class="serieslist">
        {#each es as e (e.title)}
          <li class:current={e.current} class:owned={e.mine.length}>
            <span class="ord">{e.ordinal ?? ""}</span>
            <span class="stitle">
              {#if e.mine.length && !e.current}
                {@const r = lib.rows.find((x) => x.system === e.mine[0].system && x.title === e.mine[0].title)}
                {#if r}<button class="linkish" onclick={() => open(r.id)} title="Show this game">{e.title}</button>{:else}{e.title}{/if}
              {:else}{e.title}{/if}
              {#if e.current}<span class="badge">this game</span>{/if}
            </span>
            <span class="syear">{e.year ?? "—"}</span>
            <span class="shave">
              {#each e.mine as m}<span class="badge verified" title="In your library on {lib.platforms.find((p) => p.system === m.system)?.name ?? m.system}">{lib.platforms.find((p) => p.system === m.system)?.short ?? m.system}</span>{/each}
            </span>
          </li>
        {/each}
      </ol>
    {/if}
  {/each}
{:else}
  <p class="hint">No series known for this game yet. Enrich fills it from Wikidata (series) or libretro (franchise).</p>
{/if}

<style>
  .nver { color: var(--muted); font-size: 0.8rem; }
  .serieslist { list-style: none; padding: 0; margin: 0 auto; max-width: 40rem; font-size: 0.9rem; }
  .serieslist li { display: grid; grid-template-columns: 2.2rem minmax(0, 1fr) 3.2rem 7rem; gap: 0.5rem; align-items: center; padding: 0.35rem 0.4rem; border-bottom: 1px solid var(--line); text-align: left; }
  .serieslist li.current { background: var(--accent-bg); font-weight: 600; }
  .serieslist li:not(.owned) .stitle { color: var(--muted); }
  .serieslist .ord { color: var(--muted); font-variant-numeric: tabular-nums; text-align: right; }
  .serieslist .syear { color: var(--muted); font-variant-numeric: tabular-nums; text-align: center; }
  .serieslist .shave { display: flex; gap: 0.2rem; flex-wrap: wrap; justify-content: flex-end; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; }
  .linkish:hover { text-decoration: underline; }
</style>
