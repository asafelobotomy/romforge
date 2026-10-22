<script>
  import { invoke } from "@tauri-apps/api/core";

  // Settings → Steam (stage 2, step 7): romforge as one non-Steam game,
  // started in the fullscreen mode, with its own art and a Steam Deck
  // controller template. What it writes is shown first; nothing is written
  // while Steam runs (it rewrites its list of shortcuts when it closes).
  let status = $state(null);
  let account = $state("");
  let plan = $state(null);
  let busy = $state(false);
  let message = $state("");
  let error = $state("");

  async function refresh() {
    status = await invoke("steam_status").catch(() => null);
    if (status && !status.users.some((u) => u.account === account)) account = status.users[0]?.account ?? "";
  }
  refresh();

  let added = $derived(status?.added.find(([a]) => a === account)?.[1] ?? null);
  const say = (e) => String(e).replace(/^settings: /, "");

  async function preview() {
    error = "";
    message = "";
    try {
      plan = await invoke("steam_plan", { account });
    } catch (e) {
      error = say(e);
    }
  }
  async function add() {
    busy = true;
    error = "";
    try {
      await invoke("steam_add", { account });
      plan = null;
      message = "Added. Start Steam: romforge is in your library. In its controller settings, pick the \"romforge (Deck)\" template.";
    } catch (e) {
      error = say(e);
    }
    busy = false;
    refresh();
  }
  async function remove() {
    busy = true;
    error = "";
    try {
      message = (await invoke("steam_remove", { account })) ? "Taken out of Steam." : "romforge wasn't in this Steam library.";
    } catch (e) {
      error = say(e);
    }
    busy = false;
    refresh();
  }
</script>

<section class="setting">
  <span class="flabel">romforge in Steam</span>
  <p class="hint">On a Steam Deck (or in Steam's Big Picture) romforge is a game in your Steam library: it opens in the fullscreen mode, and starts your games itself. Its art and a controller template for the Deck come with it.</p>
  {#if !status}
    <p class="hint">Looking for Steam…</p>
  {:else if !status.root}
    <p class="hint">Steam isn't installed here.</p>
  {:else if !status.users.length}
    <p class="hint">No one has signed in to Steam here yet.</p>
  {:else}
    {#if status.users.length > 1}
      <label for="steam-user">Steam user</label>
      <select id="steam-user" bind:value={account} onchange={() => (plan = null)}>
        {#each status.users as u}<option value={u.account}>{u.name}{u.recent ? " (most recent)" : ""}</option>{/each}
      </select>
    {/if}
    <p>{added ? `In this Steam library (starts ${added}).` : "Not in this Steam library yet."}</p>
    {#if status.running}
      <p class="hint warn">Steam is running: close it first. It rewrites its list of games when it closes, so anything added now would be lost.</p>
    {/if}
    {#if plan}
      <div class="plan">
        <p>{plan.replaces ? "Replace romforge's entry" : "Add romforge"}: <code>{plan.exe} {plan.launch_options}</code>. Your {plan.others} other shortcut{plan.others === 1 ? "" : "s"} stay as they are; the list is backed up first.</p>
        <ul>{#each plan.files as f}<li><code>{f}</code></li>{/each}</ul>
        <div class="bar">
          <button onclick={add} disabled={busy || status.running}>Add</button>
          <button onclick={() => (plan = null)}>Cancel</button>
        </div>
      </div>
    {:else}
      <div class="bar">
        <button onclick={preview} disabled={busy}>{added ? "Update in Steam…" : "Add to Steam…"}</button>
        {#if added}<button onclick={remove} disabled={busy || status.running}>Remove from Steam</button>{/if}
        <button onclick={refresh} disabled={busy}>Check again</button>
      </div>
    {/if}
  {/if}
  {#if message}<p class="hint">{message}</p>{/if}
  {#if error}<p class="hint bad">{error}</p>{/if}
</section>

<style>
  .plan { padding: 0.6rem 0.8rem; border: 1px solid var(--line); border-radius: 6px; background: var(--bg); }
  .plan ul { margin: 0.3rem 0; padding-left: 1.1rem; font-size: 0.8rem; }
  .warn { color: var(--warn); }
  .bad { color: var(--bad); }
</style>
