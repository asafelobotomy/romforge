<script>
  import { size } from "./lib.js";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { genres, ratings, reviews, regionFlags, stars } from "./facts.js";
  import { displayTitle, artLabel, mediaKinds, extdbLabel, edition, caption, released, played } from "./labels.js";
  import { ui } from "./ui.svelte.js";

  // Details' first tab: the pictures, an infobox of facts, the overview,
  // the other media and the files. `showEmpty`: empty media boxes too (a
  // setting).
  let { row, detail, showEmpty, artPicker } = $props();

  let f = $derived(detail.fields ?? {});

  // The fanart (and later other kinds) can be swapped for another the
  // sources have: games with a platform only.
  const canPick = (r) => !!r?.system && r.status !== "patch";

  function sized(kind, e) {
    ui.dims[kind] = [e.currentTarget.naturalWidth, e.currentTarget.naturalHeight];
  }
</script>

{#snippet shot(kind, cls)}
  <button class="shot {cls}" onclick={() => (ui.viewer = { kind, src: detail.art[kind] })} title="{caption(kind, ui.dims, detail)} — click for full size">
    <img src={detail.art[kind]} alt="{artLabel[kind]} of {row.title}" onload={(e) => sized(kind, e)} />
  </button>
{/snippet}
{#snippet pickable(kind, cls)}
  {#if canPick(row)}
    <button class="shot pick {cls}" onclick={() => artPicker.openPicker(row, kind, artLabel[kind], detail.art[kind])} title="{caption(kind, ui.dims, detail)} — click to choose another">
      <img src={detail.art[kind]} alt="{artLabel[kind]} of {row.title}" onload={(e) => sized(kind, e)} />
      <span class="change" aria-hidden="true">Change</span>
    </button>
  {:else}
    {@render shot(kind, cls)}
  {/if}
{/snippet}
<!-- The box above the fanart on the left; like an
     encyclopedia's infobox, the logo and the facts in a compact
     card on the right. Stacked, art first, when narrow. -->
<div class="lead">
  <aside class="infobox" aria-label="Facts">
    {#if detail.art.logo}
      <figure class="logo" title={caption("logo", ui.dims, detail)}>{@render pickable("logo", "")}</figure>
    {:else if canPick(row)}
      <button class="emptypick logo-pick" onclick={() => artPicker.openPicker(row, "logo", "Logo", null)}>No logo yet: find one…</button>
    {:else}
      <p class="ibtitle">{displayTitle(row)}</p>
    {/if}
    <dl class="facts info">
      {#if f.releaseyear}<div class="fact"><dt>Released</dt><dd title={f.releases ?? ""}>{released(f)}</dd></div>{/if}
      {#if f.releases?.includes(";")}<div class="fact"><dt>Releases</dt><dd class="releases">{#each f.releases.split("; ") as r}<span>{r}</span>{/each}</dd></div>{/if}
      {#if f.original_title}<div class="fact"><dt>Original title</dt><dd>{f.original_title}</dd></div>{/if}
      {#if detail.play}<div class="fact"><dt>Played</dt><dd>{played(detail.play)}</dd></div>{/if}
      {#if f.genre}
        {@const gs = genres(f.genre)}
        <div class="fact"><dt>Genre</dt><dd class="icons">{#each gs as g}<span class="genre" title={g.name} aria-label={g.name}>{@html g.svg}</span>{/each}{#if gs.length === 1}<span class="gname">{gs[0].name}</span>{/if}</dd></div>
      {/if}
      {#if f.users}<div class="fact"><dt>Players</dt><dd>{f.users}</dd></div>{/if}
      {#if f.game_mode}<div class="fact"><dt>Modes</dt><dd>{f.game_mode}</dd></div>{/if}
      {#snippet studio(role, text)}
        {@const logos = (detail.credits ?? []).filter((c) => c.role === role && c.picture && c.kind === "logo")}
        {#if logos.length}<span class="logos">{#each logos as c}<img class="studio" src={c.picture} alt={c.name} title={c.name} />{/each}</span>{:else}{text}{/if}
      {/snippet}
      {#if f.developer}<div class="fact"><dt>Developer</dt><dd>{@render studio("developer", f.developer)}</dd></div>{/if}
      {#if f.publisher}<div class="fact"><dt>Publisher</dt><dd>{@render studio("publisher", f.publisher)}</dd></div>{/if}
      {#if ratings(f).length}
        <div class="fact"><dt>Rating</dt><dd class="icons">{#each ratings(f) as r}<span class="esrb" title="{r.board}: {r.label}" aria-label="{r.board} rating {r.label}"><b>{r.code}</b><small>{r.board}</small></span>{/each}</dd></div>
      {/if}
      {#each reviews(f) as r}<div class="fact"><dt>{r.name}</dt><dd title={r.note}>{r.score}</dd></div>{/each}
      {#if f.rumble || f.analog}<div class="fact"><dt>Controls</dt><dd>{[f.analog && "Analog", f.rumble && "Rumble"].filter(Boolean).join(" · ")}</dd></div>{/if}
      {#if f.serial}<div class="fact"><dt>Serial</dt><dd>{f.serial}</dd></div>{/if}
      {#if f.community_rating}
        {@const st = stars(f.community_rating)}
        <div class="fact"><dt>Community</dt><dd>{#if st}<span class="stars" title="{st.value} out of {st.max}" aria-label="{st.value} out of {st.max} stars"><span class="on" style="width: {st.pct}%">★★★★★</span>★★★★★</span> <span class="score">{st.value}{st.votes ? ` · ${st.votes.toLocaleString()} votes` : ""}</span>{:else}{f.community_rating}{/if}</dd></div>
      {/if}
      {#if detail.derived}<div class="fact"><dt>{detail.derived.relation === "translation" ? "Translation of" : "Hack of"}</dt><dd>{detail.derived.base}{detail.derived.tag ? ` (${detail.derived.tag})` : ""}</dd></div>{/if}
      {#if detail.proof}<div class="fact"><dt>Named by</dt><dd title="Not verified: {detail.proof}, beside it, applied to your verified {detail.derived?.base} gives exactly this file">romforge, from {detail.proof}</dd></div>{/if}
      {#if regionFlags(detail.name).length || edition(detail.name, true)}
        <div class="fact"><dt>Edition</dt><dd class="icons">
          {#each regionFlags(detail.name) as r}{#if r.flag}<span class="flag" title={r.name} aria-label={r.name}>{r.flag}</span>{:else}<span class="gname">{r.name}</span>{/if}{/each}
          {#if edition(detail.name, true)}<span class="gname">{edition(detail.name, true)}</span>{/if}
        </dd></div>
      {/if}
      <div class="fact"><dt>Files</dt><dd>{row.files} · {size(row.size)}</dd></div>
      {#if row.folder}<div class="fact"><dt>Folder</dt><dd class="fpath" title="The game is this whole folder (or archive): Import and Play take all of it"><bdi>{row.folder}</bdi></dd></div>{/if}
    </dl>
  </aside>
  <div class="art">
    {#if detail.art.hero}
      <figure class="fanart">
        {#if canPick(row)}
          <button class="shot pick" onclick={() => artPicker.openPicker(row, "hero", "Fanart", detail.art.hero)} title="{caption('hero', ui.dims, detail)} — click to choose another fanart">
            <img src={detail.art.hero} alt="Fanart of {row.title}" onload={(e) => sized("hero", e)} />
            <span class="change" aria-hidden="true">Change fanart</span>
          </button>
        {:else}
          {@render shot("hero", "")}
        {/if}
      </figure>
    {:else if showEmpty || canPick(row)}
      <figure class="fanart empty">
        {#if canPick(row)}
          <button class="emptypick" onclick={() => artPicker.openPicker(row, "hero", "Fanart", null)}>No fanart yet: find one…</button>
        {:else}
          <span>No fanart. Keys for SteamGridDB, ScreenScraper or IGDB in Settings can fill it.</span>
        {/if}
        <figcaption>Fanart</figcaption>
      </figure>
    {/if}
    {#if detail.art.boxart}
      <figure class="cover">
        {@render pickable("boxart", "")}
      </figure>
    {:else if showEmpty || canPick(row)}
      <figure class="cover empty">
        {#if canPick(row)}
          <button class="emptypick" onclick={() => artPicker.openPicker(row, "boxart", "Box art", null)}>No box art yet: find one…</button>
        {:else}
          <span>No box art</span>
        {/if}
        <figcaption>Box art</figcaption>
      </figure>
    {/if}
  </div>
</div>

{#if f.description}
  {@const src = detail.field_sources?.description}
  <h3>Overview{#if src}<span class="hint src"> · from {extdbLabel[src] ?? src}{#if src === "wikipedia" && f.description_licence} ({f.description_licence}){/if}</span>{/if}</h3>
  <p class="desc">{f.description}</p>
  {#if src === "wikipedia" && f.description_url}
    <p class="hint credit"><button class="linkish" onclick={() => openUrl(f.description_url)}>Read more on Wikipedia</button>; text under {f.description_licence ?? "CC BY-SA 4.0"}, shortened to its summary.</p>
  {/if}
{/if}

{#if mediaKinds.some((k) => detail.art[k]) || showEmpty}
  <h3>Media</h3>
  <div class="gallery">
    {#each mediaKinds as k}
      {#if detail.art[k]}
        <figure class="media">
          {@render pickable(k, "")}
          <figcaption>{caption(k, ui.dims, detail)}</figcaption>
        </figure>
      {:else if showEmpty || canPick(row)}
        <figure class="media empty">
          {#if canPick(row)}
            <button class="emptypick" onclick={() => artPicker.openPicker(row, k, artLabel[k], null)}>Find {artLabel[k].toLowerCase()}…</button>
          {:else}
            <span>—</span>
          {/if}
          <figcaption>{artLabel[k]}</figcaption>
        </figure>
      {/if}
    {/each}
  </div>
{/if}
<h3>Files</h3>
<ul class="files">
  {#each detail.files as file}
    <li title={file.sha1}>
      <span class="badge {file.status === 'matched' ? 'verified' : file.status}">{file.status.replace("_", " ")}</span>
      <span class="fpath"><bdi>{file.path}</bdi></span>
      {#if file.game_id}
        {@const g = file.game_id}
        <span class="fid" title="What the game's own header or boot files say (evidence, not a match)">{[g.title, g.serial, g.region].filter(Boolean).join(" · ")}</span>
      {/if}
    </li>
  {:else}
    {#if row.folder}
      <li title="Named by ScummVM's list from these files' sizes and first bytes; they aren't hashed one by one">
        <span class="badge {row.status}">{row.files} files</span>
        <span class="fpath"><bdi>{row.folder}</bdi></span>
      </li>
    {/if}
  {/each}
</ul>

<style>
  /* Every image keeps its own aspect ratio and is never scaled past its
     own pixels in Details; click one for full size. */
  .shot { display: block; padding: 0; border: none; background: none; cursor: zoom-in; border-radius: 0; line-height: 0; }
  .shot img { display: block; max-width: 100%; height: auto; margin: 0 auto; }
  figure { margin: 0; }
  figcaption { font-size: 0.72rem; color: var(--muted); padding: 0.2rem 0.1rem 0; line-height: 1.3; }
  figure.empty span { display: grid; place-items: center; text-align: center; padding: 0.5rem; color: var(--muted); font-size: 0.8rem; border: 1px dashed var(--line); border-radius: 6px; }
  .fanart { margin-bottom: 0.75rem; }
  .fanart .shot { margin: 0 auto; }
  .shot.pick { position: relative; cursor: pointer; }
  .shot.pick .change { position: absolute; right: 0.5rem; bottom: 0.5rem; padding: 0.2rem 0.55rem; border-radius: 4px; background: #000a; color: #fff; font-size: 0.75rem; line-height: 1.4; opacity: 0; transition: opacity 0.15s; }
  .shot.pick:hover .change, .shot.pick:focus-visible .change { opacity: 1; }
  .emptypick { width: 100%; height: 3rem; border: 1px dashed var(--line); border-radius: 6px; background: none; color: var(--muted); cursor: pointer; }
  .emptypick:hover { color: var(--accent); border-color: var(--accent); }
  .logo-pick { margin: 0 0 0.45rem; height: 2.5rem; }
  .fanart .shot img { max-height: 16rem; border-radius: 6px; }
  .fanart.empty span { height: 5rem; }
  /* Portrait (most boxes) and landscape (SNES, N64) both fit: the cover
     takes the width its own aspect needs, up to 45% of the panel. */
  /* The top of Details: the box above the fanart on the left, an
     infobox (logo and facts) on the right; stacked, art first and the
     infobox a card of its own width, when narrow. */
  .lead { display: grid; grid-template-columns: minmax(0, 1fr); gap: 1rem; align-items: start; text-align: left; }
  .lead .art { display: flex; flex-direction: column; gap: 0.75rem; align-items: center; min-width: 0; }
  .lead .infobox { order: 1; width: 100%; max-width: 21rem; justify-self: center; box-sizing: border-box; }
  @container (min-width: 36rem) {
    .lead { grid-template-columns: minmax(0, 1fr) minmax(14rem, 19rem); }
    .lead .infobox { justify-self: stretch; }
  }
  .infobox { border: 1px solid var(--line); border-radius: 8px; background: var(--panel); padding: 0.55rem 0.7rem 0.35rem; font-size: 0.85rem; }
  .infobox .logo { display: flex; justify-content: center; padding: 0.2rem 0 0.55rem; }
  .infobox .logo .shot img { max-height: 4.5rem; }
  .ibtitle { margin: 0 0 0.45rem; font-weight: 600; text-align: center; }
  .facts.info { display: block; margin: 0; }
  .facts.info .fact { display: grid; grid-template-columns: 5.4rem minmax(0, 1fr); gap: 0.6rem; align-items: center; padding: 0.28rem 0; border-top: 1px solid var(--line); }
  .facts.info dd { min-width: 0; overflow-wrap: break-word; line-height: 1.3; }
  .infobox .genre { width: 1.5rem; height: 1.5rem; }
  .infobox .genre :global(svg) { width: 16px; height: 16px; }
  .infobox .esrb { width: 1.55rem; height: 2.05rem; }
  .infobox .esrb b { font-size: 0.9rem; }
  .infobox .esrb small { font-size: 0.38rem; }
  .infobox .stars { font-size: 0.9rem; }
  .infobox .flag { font-size: 1.1rem; }
  .infobox .studio { max-height: 1.8rem; }
  /* The box first, the fanart under it. */
  .art .cover { order: -1; }
  .art .fanart { margin: 0; width: 100%; }
  .art .fanart .shot img { max-height: 12rem; }
  .cover { flex: 0 1 auto; max-width: 100%; }
  .cover .shot img { border-radius: 4px; max-height: 15rem; margin: 0; }
  .cover.empty span { height: 14rem; }
  .facts dd.icons { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
  .genre { display: inline-grid; place-items: center; width: 1.9rem; height: 1.9rem; border-radius: 6px; background: var(--accent-bg); color: var(--accent); cursor: help; }
  .gname { color: var(--muted); font-size: 0.85rem; }
  .flag { font-size: 1.35rem; line-height: 1; cursor: help; font-family: "Noto Color Emoji", "Apple Color Emoji", "Segoe UI Emoji", sans-serif; }
  .logos { display: inline-flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  .studio { max-height: 2.2rem; max-width: 8rem; object-fit: contain; background: #fff; border-radius: 4px; padding: 0.15rem 0.3rem; }
  /* Drawn here, not the ESRB's own artwork: the letter in a white box with
     a black rim, as the rating marks look. */
  .facts dd.releases { display: flex; flex-direction: column; gap: 0.1rem; font-size: 0.85em; }
  .credit { margin-top: -0.4rem; }
  .esrb { display: inline-flex; flex-direction: column; align-items: center; justify-content: center; min-width: 2rem; padding: 0 0.15rem; box-sizing: border-box; height: 2.6rem; background: #fff; color: #000; border: 2px solid #000; outline: 1px solid #fff; border-radius: 2px; line-height: 1; cursor: help; }
  .esrb b { font-size: 1.15rem; font-family: Arial, Helvetica, sans-serif; font-weight: 900; }
  .esrb small { font-size: 0.45rem; font-weight: 700; letter-spacing: 0.03em; margin-top: 0.15rem; }
  .stars { position: relative; display: inline-block; color: var(--line); letter-spacing: 0.08em; font-size: 1.05rem; line-height: 1; }
  .stars .on { position: absolute; inset: 0 auto 0 0; overflow: hidden; white-space: nowrap; color: #f5b301; }
  .score { color: var(--muted); font-size: 0.85rem; }
  .logo .shot img { max-height: 6rem; max-width: 100%; margin: 0 auto; }
  .gallery { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: flex-end; justify-content: center; }
  .media .shot img { max-height: 10rem; max-width: min(100%, 18rem); background: #0002; }
  .media.empty span { height: 6rem; width: 8rem; }
  .fid { flex-shrink: 0; color: var(--muted); font-size: 0.75rem; }
</style>
