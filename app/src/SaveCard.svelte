<script>
  import { size, thumbSrc } from "./lib.js";
  import { ago } from "./labels.js";

  // One save on the Saves page: its game's box art (in the shape of its
  // box) and what is known about the save. A save matched to a game opens
  // it in the library; one not matched shows its file type instead.
  let { save: s, openGame } = $props();

  const COVER_H = 144;
  const how = { serial: "matched by serial", file_name: "matched by file name" };
  let broken = $state(false);
  const src = $derived(s.cover && !broken ? thumbSrc(s.cover, COVER_H) : null);
  const width = $derived(Math.round((COVER_H * (s.box_aspect || 72)) / 100));
  const name = $derived(String(s.file).split("/").pop());
  const folder = $derived(String(s.file).split("/").slice(0, -1).join("/"));
  const kind = $derived(name.includes(".") ? name.split(".").pop().toUpperCase() : "SAVE");
</script>

<article class="card" class:linked={!!s.row}>
  <svelte:element
    this={s.row ? "button" : "div"}
    class="cover"
    style="width: {width}px; height: {COVER_H}px"
    onclick={s.row ? () => openGame(s.row) : undefined}
    title={s.row ? `Show ${s.game} in the library` : undefined}
    role={s.row ? undefined : "img"}
    aria-label={s.row ? undefined : `${kind} save`}
  >
    {#if src}
      <img {src} alt="Box art of {s.game}" onerror={() => (broken = true)} />
    {:else}
      <span class="kind">{kind}</span>
    {/if}
  </svelte:element>
  <div class="about">
    {#if s.row}
      <button class="game linkish" onclick={() => openGame(s.row)} title="Show it in the library">{s.game}</button>
    {:else if s.game}
      <span class="game">{s.game}</span>
    {:else}
      <span class="game none" title={s.serial ? `Holds ${s.serial}, which names no single game here` : ""}>Not matched</span>
    {/if}
    <span class="file" title={s.path}><bdi>{name}</bdi>{#if folder}<span class="hint">{` in ${folder}/`}</span>{/if}</span>
    <span class="meta">
      {size(s.size)}{#if s.modified}{" · "}<span title={new Date(s.modified * 1000).toLocaleString()}>{ago(s.modified)}</span>{/if}
    </span>
    {#if s.matched_by}<span class="hint how">{how[s.matched_by]}</span>{/if}
  </div>
</article>

<style>
  .card { display: flex; gap: 0.7rem; padding: 0.6rem; border: 1px solid var(--line); border-radius: 8px; background: var(--panel, transparent); min-width: 0; }
  .cover { flex: none; padding: 0; border: none; border-radius: 4px; overflow: hidden; display: grid; place-items: center; background: var(--accent-bg, #8883); }
  button.cover { cursor: pointer; }
  button.cover:hover { outline: 2px solid var(--accent); }
  .cover img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .kind { font-size: 0.8rem; font-weight: 700; letter-spacing: 0.05em; color: var(--muted); }
  .about { display: flex; flex-direction: column; gap: 0.25rem; min-width: 0; font-size: 0.82rem; }
  .game { font-weight: 700; font-size: 0.92rem; line-height: 1.2; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .game.none { font-weight: 400; color: var(--muted); }
  .file { overflow-wrap: anywhere; }
  .meta { color: var(--muted); }
  .how { font-size: 0.75rem; }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font: inherit; font-weight: 700; }
  .linkish:hover { text-decoration: underline; }
</style>
