<script>
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { studioRoles, peopleRoles, cardsFor, devGroups } from "./credits.js";

  // Details' Development tab: studios, people, release and identification,
  // as cards where Wikidata names them and as lines otherwise.
  let { detail, error = $bindable() } = $props();
  let f = $derived(detail.fields ?? {});
  let groups = $derived(devGroups(f, detail));
</script>

{#snippet cards(roles)}
  {#each cardsFor(detail, roles) as [label, cs]}
    <p class="role">{label}{cs.length > 1 ? "s" : ""}</p>
    <div class="cards">
      {#each cs as c (c.qid + c.role)}
        <figure class="credit {c.kind ?? 'none'}">
          <button class="face" onclick={() => openUrl(`https://www.wikidata.org/wiki/${c.qid}`).catch((err) => (error = String(err)))} title="{c.name} on Wikidata">
            {#if c.picture}<img src={c.picture} alt={c.kind === "logo" ? `${c.name} logo` : c.name} />
            {:else}<span class="initials" aria-hidden="true">{c.name.split(/\s+/).map((w) => w[0]).join("").slice(0, 2)}</span>{/if}
          </button>
          <figcaption>
            {c.name}
            {#if c.source}<button class="linkish pic" onclick={() => openUrl(c.source).catch((err) => (error = String(err)))} title="The picture on Wikimedia Commons: its author and licence">picture: Commons</button>{/if}
          </figcaption>
        </figure>
      {/each}
    </div>
  {/each}
{/snippet}
{#each groups as [heading, rows]}
  {@const roles = heading === "Studios" ? studioRoles : heading === "Credits" ? peopleRoles : []}
  {#if rows.length || cardsFor(detail, roles).length}
    <section class="group">
      <h3>{heading}</h3>
      {@render cards(roles)}
      {#if rows.length}
        <dl class="facts wide">
          {#each rows as [k, v, url]}
            <dt>{k}</dt>
            <dd>{#if url}<a href={url} onclick={(e) => { e.preventDefault(); openUrl(url).catch((err) => (error = String(err))); }}>{v}</a>{:else}{v}{/if}</dd>
          {/each}
        </dl>
      {/if}
    </section>
  {/if}
{/each}
{#if !f.wikidata}
  <p class="hint">Not matched on Wikidata yet, so studios and credits are what libretro knows. Enrich looks it up; Search settles games it can't tell apart.</p>
{/if}

<style>
  figure { margin: 0; }
  figcaption { font-size: 0.72rem; color: var(--muted); padding: 0.2rem 0.1rem 0; line-height: 1.3; }
  .facts.wide { max-width: 40rem; margin: 0 auto; }
  .role { margin: 0.5rem 0 0.3rem; color: var(--muted); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .cards { display: flex; flex-wrap: wrap; gap: 0.9rem; justify-content: center; margin-bottom: 0.4rem; }
  .credit { width: 7.5rem; display: flex; flex-direction: column; align-items: center; }
  .credit .face { width: 6rem; height: 6rem; padding: 0.35rem; border: 1px solid var(--line); border-radius: 10px; background: #fff; display: grid; place-items: center; cursor: pointer; }
  .credit.photo .face { border-radius: 50%; padding: 0; overflow: hidden; }
  .credit img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .credit.photo img { width: 100%; height: 100%; object-fit: cover; object-position: top; }
  .credit.none .face { background: var(--accent-bg); }
  .initials { font-size: 1.6rem; font-weight: 600; color: var(--accent); }
  .credit figcaption { text-align: center; font-size: 0.85rem; color: var(--text); padding-top: 0.35rem; }
  .credit .pic { display: block; margin: 0.1rem auto 0; font-size: 0.7rem; color: var(--muted); }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; }
  .linkish:hover { text-decoration: underline; }
</style>
