<script>
  import { openUrl } from "@tauri-apps/plugin-opener";

  // Details' Links tab: the game's own pages where Wikidata has their IDs,
  // searches where not.
  let { detail, error = $bindable() } = $props();
</script>

{#if detail.links.length}
  {#each [[true, "Game pages", "This game's own page, from the IDs Wikidata keeps."], [false, "Searches", "No ID known yet: these search the site for the title."]] as [direct, heading, hint]}
    {@const ls = detail.links.filter((l) => l.direct === direct)}
    {#if ls.length}
      <section class="group">
        <h3>{heading}</h3>
        <p class="hint">{hint}</p>
        <ul class="links">
          {#each ls as l}
            <li>
              <a href={l.url} onclick={(e) => { e.preventDefault(); openUrl(l.url).catch((err) => (error = String(err))); }}>{l.label}</a>
              <span class="hint host">{new URL(l.url).host}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/each}
{:else}
  <p class="hint">Identify this file first to get links.</p>
{/if}

<style>
  .links { list-style: none; padding: 0; margin: 0; }
  .links li { display: flex; gap: 0.6rem; align-items: baseline; justify-content: center; padding: 0.25rem 0; }
  .links a { color: var(--accent); min-width: 9rem; }
  .links .host { font-size: 0.8rem; }
</style>
