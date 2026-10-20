<script>
  import { invoke } from "@tauri-apps/api/core";

  // Search's "Name as suggested": the selected rows that need naming, each
  // with its strongest suggestion; the ticked ones are named by it (a
  // translation or hack as one when its name says so), never verified.
  // Only a name match or better can be ticked; weaker ones go to Search.
  // `call` reports errors on the status line; `load` reloads the library.
  let { open = $bindable(false), busy = $bindable(""), note = $bindable(""), call, load } = $props();
  let items = $state([]);
  let ticked = $state({});
  let error = $state("");

  const STRONG = ["patch_of", "likely_crc", "likely_compressed", "likely_modified", "suggested"];
  const strong = (it) => it.suggestion && STRONG.includes(it.suggestion.band);
  const label = (s) => s.derived ? `${s.derived.relation} of ${s.title}${s.derived.tag ? ` (${s.derived.tag})` : ""}` : s.title;
  const n = $derived(items.filter((it) => ticked[it.row_id]).length);

  export async function openNaming(only) {
    error = "";
    const q = ((await call("search_queue", { only })) ?? []).filter((it) => it.need === "identify");
    if (!q.length) {
      note = "Nothing selected needs naming: each is matched by hash or named by you.";
      return;
    }
    items = q;
    ticked = Object.fromEntries(q.map((it) => [it.row_id, strong(it)]));
    open = true;
  }

  async function name() {
    error = "";
    busy = `Naming ${n} games…`;
    try {
      const done = await invoke("search_accept", { only: items.filter((it) => ticked[it.row_id]).map((it) => it.row_id) });
      const named = done.filter((d) => d.named).length;
      const left = done.filter((d) => !d.named);
      note = `Named ${named} (identified by you, never verified)` + (left.length ? `; left ${left.length}: ${left.map((d) => `${d.file} (${d.why_not})`).join("; ")}` : "");
      open = false;
    } catch (e) {
      error = String(e);
    }
    busy = "";
    await load();
  }
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="name-title">
      <h2 id="name-title">Name as suggested</h2>
      <p class="hint">
        Each ticked file is named by its suggestion, as you would in Search: "identified by you", or a translation or hack of
        the game when its name says so. Nothing is verified, and Forget undoes it. Only a name match or better can be
        ticked; open Search for the rest.
      </p>
      <ul class="choices">
        {#each items as it (it.row_id)}
          <li class:weak={!strong(it)}>
            <input type="checkbox" bind:checked={ticked[it.row_id]} disabled={!strong(it) || !!busy} aria-label="Name {it.title}" />
            <span class="file" title={it.title}>{it.title}</span>
            {#if it.suggestion}
              <span class="to" title={it.suggestion.evidence.map((e) => `${e.agrees ? "+" : "−"} ${e.text}`).join("\n")}>
                → {label(it.suggestion)} <span class="sys">{it.suggestion.system}</span>
              </span>
              <span class="band">{it.suggestion.band.replaceAll("_", " ")}</span>
            {:else}
              <span class="to hint">no suggestion</span><span></span>
            {/if}
          </li>
        {/each}
      </ul>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button onclick={() => (open = false)} disabled={!!busy}>Close</button>
        <button class="primary" onclick={name} disabled={!n || !!busy}>{busy || `Name ${n}`}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .choices { list-style: none; padding: 0; margin: 0.4rem 0 0; max-height: 24rem; overflow-y: auto; }
  .choices li { display: grid; grid-template-columns: auto minmax(0, 1fr) minmax(0, 1.2fr) auto; gap: 0.6rem; align-items: center; padding: 0.3rem 0; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  .choices li.weak { color: var(--muted); }
  .file, .to { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sys, .band { color: var(--muted); font-size: 0.75rem; }
  .error { color: var(--bad); }
</style>
