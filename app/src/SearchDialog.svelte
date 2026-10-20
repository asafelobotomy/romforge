<script>
  import { invoke } from "@tauri-apps/api/core";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";

  // `call` is the app's invoke that reports errors on the status line;
  // `load` reloads the library.
  let { open = $bindable(false), busy = $bindable(""), note = $bindable(""), selected, call, load } = $props();

  // Search dialog, after tinyMediaManager's: one queue item at a time, a
  // scraper and search text on top, results | picture | details, then what
  // to fetch once a result is taken.
  let queue = $state([]);
  let qi = $state(0);
  let qDetail = $state(null);
  let scraper = $state("dat");
  let sText = $state("");
  let sSystem = $state("");
  let results = $state([]);
  let picked = $state(-1);
  let preview = $state(null);
  let searching = $state(false);
  let ownTitle = $state("");
  // "own": a title of your own; "translation" / "hack": of the DAT game
  // picked in the results, `ownTitle` then being its tag.
  let ownAs = $state("own");
  let qError = $state("");
  let allSystems = $state([]);
  const fetchItems = [
    ["wikidata", "Wikidata details"],
    ["boxart", "Box art"],
    ["title", "Title screen"],
    ["snap", "Screenshot"],
    ["box3d", "3D box"],
    ["hero", "Hero"],
    ["logo", "Logo"],
    ["icon", "Icon"],
  ];
  let fetchOn = $state(["wikidata", "boxart", "title", "snap"]);
  let noOverwrite = $state(true);
  let searchSeq = 0;

  // `only`: the games selected in the list.
  export async function openSearch(only) {
    qError = "";
    queue = (await call("search_queue", { only })) ?? [];
    if (!queue.length) {
      note = "Nothing selected needs identifying: each is matched by hash or named by you, and Wikidata is settled.";
      return;
    }
    if (!allSystems.length) allSystems = (await call("all_systems")) ?? [];
    const start = queue.findIndex((q) => q.row_id === selected);
    qi = start >= 0 ? start : 0;
    open = true;
    await loadItem();
  }

  async function loadItem() {
    qError = "";
    const item = queue[qi];
    if (!item) return;
    scraper = item.need === "wikidata" ? "wikidata" : "dat";
    sSystem = item.system ?? "";
    sText = "";
    ownTitle = "";
    results = [];
    preview = null;
    searching = true;
    const d = await call("game_detail", { id: item.row_id });
    // Stepped on to another item while this one loaded.
    if (queue[qi]?.row_id !== item.row_id) return;
    qDetail = d;
    await doSearch();
  }

  async function doSearch() {
    const seq = ++searchSeq;
    qError = "";
    searching = true;
    results = [];
    picked = -1;
    preview = null;
    try {
      const r = await invoke("search_find", { rowId: queue[qi].row_id, scraper, text: sText || null, system: sSystem || null });
      if (seq !== searchSeq) return;
      results = r;
      if (r.length) await pick(0);
    } catch (e) {
      if (seq === searchSeq) qError = String(e);
    }
    if (seq === searchSeq) searching = false;
  }

  async function pick(i) {
    picked = i;
    preview = null;
    const found = results[i];
    const p = await call("search_preview", { rowId: queue[qi].row_id, found });
    if (results[picked] === found) preview = p;
  }

  async function step(d) {
    qi = Math.max(0, Math.min(queue.length - 1, qi + d));
    await loadItem();
  }

  // After an answer: drop the item and go on with the next one.
  async function answered(msg) {
    note = msg;
    queue = queue.filter((_, i) => i !== qi);
    await load();
    if (!queue.length) {
      open = false;
      note = msg + " That was the last one.";
      return;
    }
    qi = Math.min(qi, queue.length - 1);
    await loadItem();
  }

  async function take(found) {
    qError = "";
    busy = `Taking ${found.title}…`;
    try {
      const r = await invoke("search_choose", {
        rowId: queue[qi].row_id,
        found,
        system: sSystem || null,
        art: fetchOn.filter((k) => k !== "wikidata"),
        wikidata: fetchOn.includes("wikidata"),
        overwrite: !noOverwrite,
      });
      busy = "";
      const e = r.enrich;
      const fetched = e.art_saved ? `; ${e.art_saved} images saved` : "";
      const what = found.scraper === "wikidata" ? `matched to ${found.id}` : "identified by you";
      await answered(`${found.title}: ${what}${fetched}.` + (e.errors?.length ? ` Fetching: ${e.errors.join("; ")}` : ""));
    } catch (e) {
      busy = "";
      qError = String(e);
    }
  }

  function takeOwn() {
    const base = results[picked];
    if (ownAs !== "own" && base?.scraper === "dat") {
      take({ ...base, derived: { relation: ownAs, base: base.title, tag: ownTitle.trim() || null } });
      return;
    }
    take({ scraper: "dat", id: ownTitle.trim(), title: ownTitle.trim(), system: sSystem || null, year: null, score: 1, description: null });
  }

  function toggleFetch(k) {
    fetchOn = fetchOn.includes(k) ? fetchOn.filter((x) => x !== k) : [...fetchOn, k];
  }

  function reveal() {
    const f = qDetail?.files?.[0]?.path;
    if (f) revealItemInDir(f.split("#")[0]).catch((e) => (qError = String(e)));
  }
</script>

{#if open && queue[qi]}
  {@const item = queue[qi]}
  {@const chosen = results[picked]}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog search-dialog" role="dialog" aria-modal="true" aria-labelledby="search-title">
      <div class="qhead">
        <h2 id="search-title">Search</h2>
        <span class="qnav">
          <button class="mini" onclick={() => step(-1)} disabled={qi === 0 || !!busy} aria-label="Previous">‹</button>
          <span class="hint">{qi + 1} of {queue.length}</span>
          <button class="mini" onclick={() => step(1)} disabled={qi >= queue.length - 1 || !!busy} aria-label="Next">›</button>
        </span>
      </div>
      <div class="spath">
        <span class="fpath" title={qDetail?.files?.[0]?.path}><bdi>{qDetail?.files?.[0]?.path ?? item.title}</bdi></span>
        <span class="badge {item.status}">{item.need === "wikidata" ? (item.wikidata === "ambiguous" ? "Wikidata: several match" : "Wikidata: no match") : item.status.replace("_", " ")}</span>
        <button class="mini" onclick={reveal} title="Show in the file manager" aria-label="Show in the file manager">📂</button>
      </div>

      <div class="sform">
        <label for="scraper">Scraper</label>
        <div class="pick">
          <select id="scraper" bind:value={scraper} onchange={doSearch}>
            <option value="dat">DAT titles (offline)</option>
            <option value="wikidata">Wikidata</option>
          </select>
          <input bind:value={sText} placeholder={qDetail ? (item.need === "identify" ? qDetail.row.title.split("#").pop() : item.title) : ""} aria-label="Search text" onkeydown={(e) => e.key === "Enter" && doSearch()} />
          <button onclick={doSearch} disabled={searching}>⌕ Search</button>
        </div>
        <label for="splat">Platform</label>
        <div class="pick">
          <select id="splat" bind:value={sSystem} onchange={doSearch}>
            <option value="">{scraper === "dat" ? "Any that fits the file" : "Pick one…"}</option>
            {#each allSystems as s}<option value={s.key}>{s.name}</option>{/each}
          </select>
        </div>
      </div>

      <div class="sgrid">
        <div class="stable" role="listbox" aria-label="Search results">
          <div class="srow shead"><span>Search result</span><span>Year</span><span>Platform</span><span>ID</span><span title="How well it fits">%</span></div>
          {#each results as r, i}
            <div
              class="srow"
              class:sel={i === picked}
              role="option"
              aria-selected={i === picked}
              tabindex="0"
              onclick={() => pick(i)}
              ondblclick={() => take(r)}
              onkeydown={(e) => e.key === "Enter" && take(r)}
            >
              <span class="ctitle" title={r.title}>{r.title}</span>
              <span>{r.year ?? ""}</span>
              <span class="sys">{r.system ?? ""}</span>
              <span class="sys" title={r.band ? `${r.description}: never counted as verified` : ""}>{r.scraper === "wikidata" ? r.id : (r.description?.split(":")[0] ?? "")}</span>
              <span class="score">{Math.round(r.score * 100)}%</span>
            </div>
          {:else}
            <p class="hint empty">{searching ? "Searching…" : scraper === "dat" ? "No DAT game fits it: not by name, header, serial or CRC." : "Wikidata knows nothing by that name."}</p>
          {/each}
        </div>
        <div class="sposter">
          {#if preview?.image}<img src={preview.image} alt="Box art of {preview.title}" />{:else}<span class="hint">{chosen ? (preview ? "No box art" : "…") : ""}</span>{/if}
        </div>
        <div class="sinfo">
          {#if chosen}
            <h3 class="sub">{chosen.title}{chosen.year ? ` (${chosen.year})` : ""}</h3>
            {#if preview?.system_name}<p class="meta">{preview.system_name}</p>{/if}
            {#if chosen.evidence?.length}
              <p class="meta">{chosen.description} (a suggestion, never verified)</p>
              <ul class="evidence">
                {#each chosen.evidence as e}<li class:agrees={e.agrees}><span aria-hidden="true">{e.agrees ? "+" : "−"}</span> {e.text}</li>{/each}
              </ul>
            {/if}
            {#if preview?.description}<p class="desc">{preview.description}</p>{/if}
            {#if preview?.facts?.length}
              <dl class="facts">
                {#each preview.facts as [k, v]}<dt>{k}</dt><dd>{v}</dd>{/each}
              </dl>
            {/if}
          {/if}
          <div class="own">
            <label for="own">Not listed?</label>
            <div class="pick">
              <select bind:value={ownAs} aria-label="Name it as">
                <option value="own">My own title</option>
                <option value="translation" disabled={chosen?.scraper !== "dat"}>A translation of the selected game</option>
                <option value="hack" disabled={chosen?.scraper !== "dat"}>A hack of the selected game</option>
              </select>
              {#if ownAs === "own" || chosen?.scraper !== "dat"}
                <input id="own" bind:value={ownTitle} placeholder="Your own title" />
                <button onclick={takeOwn} disabled={!ownTitle.trim() || !sSystem || !!busy} title={sSystem ? "Name it yourself: shown as identified by you, never as verified" : "Pick a platform first"}>Use</button>
              {:else}
                <input id="own" bind:value={ownTitle} placeholder="What it says (e.g. English v1.1), optional" />
                <button onclick={takeOwn} disabled={!!busy} title={`As a ${ownAs} of ${chosen.title}: identified by you, never verified`}>Use</button>
              {/if}
            </div>
          </div>
        </div>
      </div>

      <section class="setting">
        <span class="flabel">Fetch after choosing</span>
        <div class="chips">
          {#each fetchItems as [k, label]}
            <button class="chip" class:on={fetchOn.includes(k)} aria-pressed={fetchOn.includes(k)} onclick={() => toggleFetch(k)}>
              {label}{#if fetchOn.includes(k)} <span aria-hidden="true">×</span>{/if}
            </button>
          {/each}
        </div>
        <label class="toggle"><input type="checkbox" bind:checked={noOverwrite} /> Do not overwrite existing data</label>
      </section>

      {#if qError}<p class="err" role="alert">{qError}</p>{/if}
      <div class="actions">
        <button onclick={() => step(1)} disabled={qi >= queue.length - 1 || !!busy}>Skip</button>
        <span class="spacer"></span>
        <button onclick={() => (open = false)} disabled={!!busy}>Cancel</button>
        <button class="primary" onclick={() => take(chosen)} disabled={!chosen || !!busy}>{busy || "OK"}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .qnav { display: flex; gap: 0.4rem; align-items: center; }
  .mini { padding: 0.1rem 0.45rem; }
  .spath { display: flex; gap: 0.5rem; align-items: center; margin-bottom: 0.6rem; }
  .spath .fpath { flex: 1; font-size: 0.85rem; }
  .sform { display: grid; grid-template-columns: auto 1fr; gap: 0.4rem 0.6rem; align-items: center; margin-bottom: 0.6rem; }
  .sform label { font-weight: 600; font-size: 0.9rem; }
  .sform .pick select { max-width: 16rem; }
  .sgrid { display: grid; grid-template-columns: minmax(0, 1.3fr) 9.5rem minmax(0, 1fr); gap: 0.75rem; border-top: 1px solid var(--line); padding-top: 0.6rem; }
  .stable { border: 1px solid var(--line); border-radius: 4px; height: 17rem; overflow-y: auto; font-size: 0.85rem; }
  .srow { display: grid; grid-template-columns: minmax(0, 1fr) 3rem 5.5rem 5.5rem 2.6rem; gap: 0.4rem; padding: 0.2rem 0.5rem; cursor: default; align-items: baseline; }
  .srow:nth-child(even):not(.shead) { background: color-mix(in srgb, var(--line) 35%, transparent); }
  .srow.sel { background: var(--accent-bg); }
  .srow:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .shead { position: sticky; top: 0; background: var(--panel); color: var(--muted); border-bottom: 1px solid var(--line); font-size: 0.8rem; }
  .stable .empty { padding: 0.5rem; }
  .sposter { height: 13rem; display: grid; place-items: center; border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
  .sposter img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .sinfo { display: flex; flex-direction: column; gap: 0.35rem; min-width: 0; max-height: 17rem; overflow-y: auto; }
  .sinfo .desc { font-size: 0.85rem; color: var(--muted); }
  .sinfo .facts { gap: 0.15rem 0.6rem; font-size: 0.8rem; }
  .sinfo .facts dd { overflow-wrap: anywhere; }
  .own { margin-top: auto; padding-top: 0.4rem; }
  .own label { font-size: 0.85rem; font-weight: 600; }
  .flabel { font-weight: 600; font-size: 0.9rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.3rem; margin: 0.4rem 0; }
  .chip { padding: 0.1rem 0.5rem; border-radius: 3px; font-size: 0.8rem; opacity: 0.6; }
  .chip.on { opacity: 1; background: var(--accent-bg); }
  @media (max-width: 52rem) {
    .sgrid { grid-template-columns: 1fr; }
    .sposter { height: 10rem; }
  }
  .qhead { display: flex; justify-content: space-between; align-items: baseline; }
  .score { width: 2.6rem; color: var(--muted); font-size: 0.8rem; text-align: right; }
  .evidence { list-style: none; padding: 0; margin: 0.2rem 0 0.5rem; font-size: 0.8rem; color: var(--muted); }
  .evidence li.agrees { color: inherit; }
  .evidence span { display: inline-block; width: 1em; }
  .pick select { max-width: 14rem; }
</style>
