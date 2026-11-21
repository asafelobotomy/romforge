<script>
  import { invoke } from "@tauri-apps/api/core";

  // Settings → uQ search: on the uQ desktop, typing a game's name in uQ's
  // launcher finds it in your library. A package sets that up itself; for
  // the AppImage, which can't install into /usr, it is a copy of the
  // provider and two small files in your own data folder, added here (and
  // taken away here) only when you say so.
  let status = $state(null);
  let confirming = $state(false);
  let busy = $state(false);
  let message = $state("");
  let error = $state("");

  async function refresh() {
    status = await invoke("uq_search_status").catch(() => null);
  }
  refresh();
  const say = (e) => String(e).replace(/^settings: /, "");

  async function add() {
    busy = true;
    error = "";
    message = "";
    try {
      await invoke("uq_search_add");
      confirming = false;
      message = "Added. uQ finds your games by name within a minute; open its launcher and type one.";
    } catch (e) {
      error = say(e);
    }
    busy = false;
    refresh();
  }
  async function remove() {
    busy = true;
    error = "";
    message = "";
    try {
      message = (await invoke("uq_search_remove")) ? "Taken away." : "There was nothing of romforge's to take away.";
    } catch (e) {
      error = say(e);
    }
    busy = false;
    refresh();
  }
</script>

<section class="setting">
  <span class="flabel">romforge in uQ's search</span>
  <p class="hint">On the uQ desktop, type a game's name in the launcher and press Enter to start it the way Play does. The launcher also shows them while romforge is closed.</p>
  {#if !status}
    <p class="hint">Looking…</p>
  {:else if status.packaged}
    <p>Already set up by romforge's package. Nothing to add.</p>
  {:else if !status.source}
    <p class="hint">This build of romforge has no search provider to add (the AppImage and the packages carry one).</p>
  {:else}
    <p>{status.added ? (status.stale ? "Added (an older copy: it is brought up to date when romforge starts, or add it again)." : "Added.") : "Not added."}</p>
    {#if confirming}
      <div class="plan">
        <p>This writes three files in your own folders, nothing outside them. uQ starts the copy when its launcher opens, and it quits after a minute:</p>
        <ul>{#each status.files as f}<li><code>{f}</code></li>{/each}</ul>
        <div class="bar">
          <button onclick={add} disabled={busy}>Add</button>
          <button onclick={() => (confirming = false)}>Cancel</button>
        </div>
      </div>
    {:else}
      <div class="bar">
        {#if !status.added}<button onclick={() => (confirming = true)} disabled={busy}>Add to uQ search…</button>{/if}
        {#if status.added}<button onclick={remove} disabled={busy}>Remove from uQ search</button>{/if}
        <button onclick={refresh} disabled={busy}>Check again</button>
      </div>
    {/if}
  {/if}
  {#if message}<p class="hint">{message}</p>{/if}
  {#if error}<p class="hint bad">{error}</p>{/if}
</section>

<style>
  .plan { padding: 0.6rem 0.8rem; border: 1px solid var(--line); border-radius: var(--radius-control); background: var(--bg); }
  .plan ul { margin: 0.3rem 0; padding-left: 1.1rem; font-size: 0.8rem; }
  .bad { color: var(--bad); }
</style>
