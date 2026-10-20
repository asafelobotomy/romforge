<script>
  // The manual reader's side panel (milestone 26): the collection (every
  // highlight and note, by page: jump to one, edit its note or colour,
  // delete it) and search results, with a way to read (OCR) the scanned
  // pages search can't see yet.
  let { tab = $bindable("collection"), marks, editing = $bindable(null), search, go, ocrAll = null, readAll, stopAll, edit, remove } = $props();

  const COLORS = ["yellow", "red", "green", "blue"];
  let draft = $state("");
  let draftColor = $state("yellow");

  function startEdit(m) {
    editing = m.id;
  }
  // Editing a mark (from the list or a pin on the page): its note in the box.
  $effect(() => {
    const m = marks.find((x) => x.id === editing);
    if (m) {
      draft = m.note;
      draftColor = m.color;
    }
  });
  async function saveEdit(m) {
    await edit(m.id, draft, draftColor);
    editing = null;
  }
</script>

<aside class="side">
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "collection"} class:active={tab === "collection"} onclick={() => (tab = "collection")}>Collection ({marks.length})</button>
    <button role="tab" aria-selected={tab === "search"} class:active={tab === "search"} onclick={() => (tab = "search")}>Search{search?.hits ? ` (${search.hits.length})` : ""}</button>
  </div>
  {#if tab === "collection"}
    {#if !marks.length}
      <p class="hint">Select text on a page to highlight it, or switch to Area and drag a box over a picture to add a note. They're kept here.</p>
    {/if}
    <ul>
      {#each marks as m (m.id)}
        <li class:on={editing === m.id}>
          <div class="head">
            <span class="chip {m.color}"></span>
            <button class="linkish" onclick={() => go(m.page)}>Page {m.page}</button>
            <span class="hint">{m.kind === "area" ? "area note" : "highlight"}</span>
            <span class="spacer"></span>
            <button class="linkish" onclick={() => startEdit(m)}>Edit</button>
            <button class="linkish" onclick={() => remove(m.id)}>Delete</button>
          </div>
          {#if m.text}<blockquote>{m.text}</blockquote>{/if}
          {#if editing === m.id}
            <div class="dots">{#each COLORS as c}<button class="chip {c}" class:sel={draftColor === c} aria-label={c} onclick={() => (draftColor = c)}></button>{/each}</div>
            <textarea bind:value={draft} rows="3" placeholder="Your note"></textarea>
            <div class="acts"><button onclick={() => (editing = null)}>Cancel</button><button class="primary" onclick={() => saveEdit(m)}>Save</button></div>
          {:else if m.note}
            <p class="note">{m.note}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if !search}
    <p class="hint">Type in the search box and press Enter.</p>
  {:else if search.busy}
    <p class="hint">Searching…</p>
  {:else}
    {#if ocrAll}
      <p class="hint">Reading the scans… {ocrAll.done} of {ocrAll.total} pages <button class="linkish" onclick={stopAll}>Stop</button></p>
    {:else if search.unread?.length}
      <p class="hint">
        {search.unread.length} page{search.unread.length === 1 ? " is a scan" : "s are scans"} not read yet, so search can't see {search.unread.length === 1 ? "it" : "them"}.
        <button class="linkish" onclick={() => readAll(search.unread)}>Read them (OCR)</button>
      </p>
    {/if}
    {#if search.hasText && !search.hits.length}
      <p class="hint">“{search.query}” isn't in this manual{search.unread?.length ? "'s text so far" : ""}.</p>
    {/if}
    <ul>
      {#each search.hits as h (h.page)}
        <li>
          <button class="hit" onclick={() => go(h.page)}><strong>Page {h.page}</strong> <span>{h.before}<mark>{h.match}</mark>{h.after}</span></button>
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .side { width: 19rem; flex: none; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--line); padding-left: 0.6rem; overflow-y: auto; }
  .tabs { display: flex; gap: 0.2rem; border-bottom: 1px solid var(--line); margin-bottom: 0.4rem; }
  .tabs button { border: none; border-bottom: 2px solid transparent; border-radius: 0; background: none; color: var(--muted); padding: 0.3rem 0.5rem; }
  .tabs button.active { color: var(--text); border-bottom-color: var(--accent); }
  ul { list-style: none; margin: 0; padding: 0; }
  li { padding: 0.45rem 0; border-top: 1px solid var(--line); display: grid; gap: 0.3rem; }
  li.on { background: var(--accent-bg); }
  .head { display: flex; align-items: center; gap: 0.4rem; }
  .spacer { flex: 1; }
  blockquote { margin: 0; padding-left: 0.5rem; border-left: 3px solid var(--line); font-size: 0.85rem; }
  .note { margin: 0; font-size: 0.85rem; white-space: pre-wrap; }
  textarea { width: 100%; box-sizing: border-box; font: inherit; resize: vertical; }
  .acts, .dots { display: flex; gap: 0.35rem; justify-content: flex-end; }
  .dots { justify-content: flex-start; }
  .chip { width: 0.9rem; height: 0.9rem; padding: 0; border-radius: 50%; border: 2px solid transparent; flex: none; }
  .chip.sel { border-color: var(--text); }
  .yellow { background: #f0b81d; }
  .red { background: #d8312f; }
  .green { background: #1f9b4f; }
  .blue { background: #3b5ab0; }
  .hit { display: block; width: 100%; text-align: left; border: none; background: none; padding: 0.2rem; font-size: 0.85rem; }
  .hit span { color: var(--muted); }
  mark { background: #f0b81d80; color: var(--text); }
  .linkish { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
</style>
