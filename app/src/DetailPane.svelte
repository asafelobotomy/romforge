<script>
  import DetailsTab from "./DetailsTab.svelte";
  import DevelopmentTab from "./DevelopmentTab.svelte";
  import ManualTab from "./ManualTab.svelte";
  import SeriesTab from "./SeriesTab.svelte";
  import VersionsTab from "./VersionsTab.svelte";
  import LinksTab from "./LinksTab.svelte";
  import GameSettings from "./GameSettings.svelte";
  import { displayTitle, statusLabel } from "./labels.js";
  import { ui } from "./ui.svelte.js";
  import { invoke } from "@tauri-apps/api/core";

  // A BIOS file's row: plan importing it on the BIOS page.
  async function toBios(row) {
    const paths = await invoke("firmware_in_library", { fileId: Number(row.id.slice(2)) }).catch(() => []);
    if (paths.length) ui.biosPaths = paths;
  }

  // Details of the picked game: its name, status and what can be done
  // with it, over its tabs.
  let { row, detail, lib, busy, listSort, showEmpty, regionFlags = true, open, forget, applyPatchRow, patcher, artPicker, note = $bindable(""), error = $bindable() } = $props();
</script>

<section class="detail">
  {#if !row || !detail}
    <div class="empty"><p>Pick a game to see its details.</p></div>
  {:else}
    <div class="dhead">
      <h1>{displayTitle(row)}</h1>
      {#if row.label && row.label !== row.title}<p class="meta">Set <code>{row.title}</code></p>{:else if row.name?.title && row.name.title !== row.title}<p class="meta" title={row.title}>{row.title}</p>{/if}
      <p class="meta">
        {row.system_name ?? "Unidentified"} · <span class="badge {row.status}">{statusLabel[row.status]}</span>
        {#if row.status === "by_you" || row.status === "derivative"}
          <button class="link" onclick={forget} disabled={!!busy} title="Undo naming these files: they show as unknown again">Forget</button>
        {/if}
        {#if detail.patch_links?.length && (row.status === "verified" || row.status === "by_you")}
          <button class="link" onclick={() => patcher.openPatch(row, detail)} disabled={!!busy} title="Apply a translation or hack you downloaded: a new copy, your original untouched">{detail.patches?.length ? `Patch… (${detail.patches.length} found)` : "Patch…"}</button>
        {/if}
        {#if row.status === "patch" && detail.patches?.[0]}
          {#if row.base_id}
            <button class="link" onclick={applyPatchRow} disabled={!!busy} title="Make a patched copy of the game it is for; your original untouched">Apply…</button>
          {:else}
            <span class="hint">{row.system ? "Add the game it is for to apply it" : "No game found for it"}</span>
          {/if}
        {/if}
        {#if row.status === "firmware"}
          <span class="hint">{row.bios} for {row.system_name}, by its hash in RetroBIOS's list</span>
          <button class="link" onclick={() => toBios(row)} disabled={!!busy} title="Put it where this platform's emulators find it (a plan first; your file isn't moved unless Settings says so)">Import to BIOS…</button>
        {/if}
        {#if row.packed}<span class="hint" title="ScummVM can't play a game from an archive">Still zipped: Rename &amp; Cleanup unpacks it into a folder to play</span>{/if}
        {#if row.likely_system}<span class="hint" title="By its file extension only: Search can name it">Looks like {row.likely_system}</span>{/if}
        {#if row.status === "missing"}<span class="hint">You have only versions of it{detail.patches?.length ? ` (${detail.patches.length} patch${detail.patches.length > 1 ? "es" : ""} found for it)` : ""}</span>{/if}
        {#if detail.patched}<span class="badge" title="romforge made it from {detail.patched.patch} ({detail.patched.kind}) on {detail.patched.created_at}">patched from {detail.patched.patch}</span>{/if}
        {#if row.cue_differs && row.cue_fits}<span class="badge" title="All tracks match; the cue sheet was rewritten but names exactly these tracks">cue rewritten</span>{:else if row.cue_differs}<span class="badge" title="All tracks match; the cue sheet differs from the DAT's">cue differs</span>{/if}
      </p>
    </div>
    <div class="tabs" role="tablist">
      {#each [["details", "Details"], ["manual", "Manual"], ["development", "Development"], ["series", "Series"], ["versions", "Versions"], ["links", "Links"], ["settings", "Settings"]] as [k, label]}
        <button role="tab" aria-selected={ui.tab === k} class:active={ui.tab === k} onclick={() => (ui.tab = k)}>{label}</button>
      {/each}
    </div>

    {#if ui.tab === "details"}
      <DetailsTab {row} {detail} {showEmpty} {artPicker} />
    {:else if ui.tab === "manual"}
      {#if row.system && row.playable}<ManualTab {row} bind:note bind:error />{:else}<p class="hint">Manuals are for identified games.</p>{/if}
    {:else if ui.tab === "development"}
      <DevelopmentTab {detail} bind:error />
    {:else if ui.tab === "versions"}
      <VersionsTab {row} {lib} {listSort} {open} {regionFlags} />
    {:else if ui.tab === "series"}
      <SeriesTab {detail} {lib} {open} />
    {:else if ui.tab === "settings"}
      {#if row.system && row.playable}<GameSettings {row} bind:note bind:error />{:else}<p class="hint">Settings are for identified games.</p>{/if}
    {:else}
      <LinksTab {detail} bind:error />
    {/if}
  {/if}
</section>

<style>
  h1 { font-size: 1.25rem; margin: 0; font-weight: 600; }
  /* Details: centred, its facts on the left of the cover. */
  .detail { overflow-y: auto; padding: 0.75rem 1rem 1.5rem; text-align: center; container-type: inline-size; }
  .detail .tabs { justify-content: center; }
  .dhead .meta { margin: 0.2rem 0 0.5rem; color: var(--muted); }
  .tabs { display: flex; gap: 0.25rem; border-bottom: 1px solid var(--line); margin-bottom: 0.75rem; }
  .tabs button { border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 0.35rem 0.7rem; color: var(--muted); }
  .tabs button.active { color: var(--text); border-bottom-color: var(--accent); }
  /* What every tab shares. Every picture with words under it sits centred
     over them. */
  .detail :global(figure) { display: flex; flex-direction: column; align-items: center; }
  .detail :global(figcaption) { text-align: center; }
  .detail :global(.facts.info dt) { text-align: left; font-weight: 600; font-size: 0.8rem; }
  .detail :global(.facts) { justify-content: center; }
  .detail :global(.facts dt) { text-align: right; }
  .detail :global(.facts dd) { text-align: left; }
  .detail :global(h3) { font-size: 0.95rem; margin: 1rem 0 0.4rem; }
  .detail :global(h3 .src) { font-weight: 400; font-size: 0.8rem; }
  .detail :global(.files li) { justify-content: center; }
  /* The Development and Links tabs' groups (after h3, which they override). */
  .detail :global(.group h3) { margin-top: 0.5rem; }
  .detail :global(.group + .group) { border-top: 1px solid var(--line); margin-top: 0.75rem; }
  .detail :global(.group .hint) { margin: 0 0 0.3rem; }
  button.link { background: none; border: none; padding: 0 0.3rem; color: var(--accent); text-decoration: underline; cursor: pointer; font-size: 0.8rem; }
</style>
