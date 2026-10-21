<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFile } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { basename } from "./lib.js";
  import { run } from "./jobs.svelte.js";

  // Milestone 19: apply a translation or hack you fetched to the selected
  // game. romforge makes a new copy (a version of the game); the original
  // stays as it is.
  // Making the copy is a job in the queue (footer): the dialog closes.
  let { open = $bindable(false), note = $bindable(""), call, load } = $props();
  let busy = $state("");

  let row = $state(null);
  let links = $state([]);
  let beside = $state([]);
  let patch = $state(null);
  let hack = $state(false);
  let tag = $state("");
  let plan = $state(null);
  let patchError = $state("");

  // `pick`: a patch to start with ({ path, relation }).
  export async function openPatch(r, detail, pick = null) {
    row = r;
    links = detail?.patch_links ?? [];
    patch = null;
    plan = null;
    hack = false;
    tag = "";
    patchError = "";
    open = true;
    beside = (await call("patches_beside", { rowId: r.id })) ?? [];
    if (pick) await choose(pick.path, pick.relation);
  }

  async function choose(path, relation) {
    patch = path;
    tag = "";
    if (relation) hack = relation === "hack";
    await planPatch();
  }

  // How romforge knows a patch is for this game.
  const how = {
    source: "made from its ROM",
    names: "names its file",
    beside: "beside it, same name",
    title: "named after the game",
    readme: "its readme names the game",
    folder: "in its folder",
  };

  async function browse() {
    const f = await pickFile({
      multiple: false,
      title: "A patch you downloaded",
      filters: [{ name: "Patches", extensions: ["ips", "bps", "ups", "ppf", "xdelta", "vcdiff", "zip"] }],
    });
    if (typeof f === "string") await choose(f);
  }

  async function planPatch() {
    patchError = "";
    plan = null;
    if (!patch) return;
    try {
      plan = await invoke("patch_plan", { rowId: row.id, patch, hack, tag: tag.trim() || null });
      if (!tag) tag = plan.tag;
    } catch (e) {
      patchError = String(e).replace(/^(cleanup|DAT): /, "");
    }
  }

  async function apply() {
    const args = { rowId: row.id, patch, hack, tag: tag.trim() || null };
    open = false;
    const r = await run(`Patching ${plan.title}…`, "patch_apply", args);
    if (r) note = `Made ${r.title} (batch #${r.batch_id}; Undo in Import / Cleanup removes it)`;
    await load();
  }

</script>

{#if open && row}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && (open = false)}>
    <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="patch-title">
      <h2 id="patch-title">Patch {row.label ?? row.title}</h2>
      <p class="hint">
        A translation or hack makes a new copy of the game, listed as a version of it; your copy stays as it is. romforge
        doesn't download patches: find one, save it, then pick it here (IPS, BPS, UPS, PPF, xdelta, or a zip holding one).
      </p>
      <section class="setting">
        <span class="flabel">Find a patch</span>
        <div class="pick">
          {#each links as l}<button class="link" onclick={() => openUrl(l.url)}>{l.label}</button>{/each}
        </div>
      </section>
      <section class="setting">
        <span class="flabel">Patch</span>
        {#if beside.length}
          <p class="hint">Found for it:</p>
          <div class="pick">
            {#each beside as p}<button onclick={() => choose(p.path, p.relation)} class:primary={patch === p.path} title="{p.path} ({p.kind}, {p.relation}; {how[p.paired_by] ?? p.paired_by})">{p.name === basename(p.path) ? p.name : `${basename(p.path)} › ${p.name}`}</button>{/each}
          </div>
        {/if}
        <div class="pick">
          <input value={patch ?? ""} readonly placeholder="No patch chosen" aria-label="Patch file" />
          <button onclick={browse} disabled={!!busy}>Choose…</button>
        </div>
      </section>
      {#if patch}
        <section class="setting">
          <label class="toggle"><input type="radio" name="rel" checked={!hack} onchange={() => { hack = false; planPatch(); }} /> Translation</label>
          <label class="toggle"><input type="radio" name="rel" checked={hack} onchange={() => { hack = true; planPatch(); }} /> Hack</label>
          <label for="patch-tag">Name it</label>
          <input id="patch-tag" bind:value={tag} onchange={planPatch} placeholder="English v1.1" />
        </section>
      {/if}
      {#if plan}
        <dl class="kv">
          <dt>New game</dt><dd>{plan.title}</dd>
          <dt>Patch</dt><dd>{plan.patch_name} ({plan.kind})</dd>
          <dt>Applied to</dt><dd>{plan.applies_to}</dd>
          <dt>Written to</dt><dd class="fpath"><bdi>{plan.target}</bdi></dd>
        </dl>
        {#each plan.notes as n}<p class="hint">{n}</p>{/each}
      {/if}
      {#if patchError}<p class="err" role="alert">{patchError}</p>{/if}
      <div class="actions">
        <span class="spacer"></span>
        <button onclick={() => (open = false)} disabled={!!busy}>Close</button>
        <button class="primary" onclick={apply} disabled={!plan || !!busy}>{busy || "Make the patched copy"}</button>
      </div>
    </div>
  </div>
{/if}
