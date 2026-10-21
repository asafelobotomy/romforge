<script>
  import { invoke } from "@tauri-apps/api/core";
  import SortButton from "./SortButton.svelte";
  import { sorted } from "./sort.js";
  // Always mounted, so the folders and the plan last while you look at other
  // screens; drawn only while `active`. `run` queues a long job (the
  // footer shows it); `load` reloads the library.
  // With a RomForge folder (`homeRoot`) Import does this job: the screen
  // says so and keeps only the undo of earlier batches; `openImport` opens
  // the Import plan.
  let { active, busy, note = $bindable(""), confirmUndo = $bindable(false), homeRoot = null, openImport, run, load } = $props();

  let from = $state("");
  let dest = $state("");
  let oneG1R = $state(false);
  let plan = $state(null);
  // The inputs `plan` was made from: Move uses exactly these.
  let planned = $state(null);

  // Planning only reads: it runs at once, not in the job queue.
  let planning = $state(false);
  let planError = $state("");
  async function planOrganize() {
    const inputs = { from, dest, oneG1r: oneG1R };
    planning = true;
    planError = "";
    try {
      plan = await invoke("organize_plan", inputs);
    } catch (e) {
      plan = null;
      planError = String(e);
    }
    planning = false;
    planned = plan ? inputs : null;
  }

  let moveSort = $state({ key: "from", dir: 1 });
  const moveGetters = { from: (m) => m.from, to: (m) => m.to };

  // Changing what to organize makes the plan on screen stale.
  function replan() {
    plan = null;
    planned = null;
  }

  async function applyOrganize() {
    if (!planned) return;
    const r = await run("Moving files…", "organize_apply", planned);
    if (r) note = r.batch_id ? `Batch #${r.batch_id}: moved ${r.moved}, failed ${r.failed.length}. Undo moves them back.` : "Nothing to move";
    replan();
    await load();
  }

  async function undoOrganize() {
    confirmUndo = false;
    const r = await run("Moving files back…", "organize_undo", {});
    if (r) note = `Batch #${r.batch_id}: restored ${r.restored}, failed ${r.failed.length}`;
    replan();
    await load();
  }
</script>

{#if active && homeRoot}
  <section class="wide">
    <p class="hint">
      Your RomForge folder takes care of this: <b>Import</b> moves identified games into it, one file each, named by
      the DAT and sorted by platform, with every version of a game kept and grouped. Organize is for setups without one.
    </p>
    <div class="bar">
      <button class="primary" onclick={openImport} disabled={!!busy}>Open Import</button>
      <span class="spacer"></span>
      {#if confirmUndo}
        <span>Move the last Organize batch back?</span>
        <button onclick={undoOrganize} disabled={!!busy}>Yes, move back</button>
        <button onclick={() => (confirmUndo = false)}>Cancel</button>
      {:else}
        <button onclick={() => (confirmUndo = true)} disabled={!!busy} title="Earlier Organize batches can still be undone">Undo last Organize batch…</button>
      {/if}
    </div>
  </section>
{:else if active}
  <section class="wide">
    <div class="bar">
      <input placeholder="Organize files in, /home/me/roms" bind:value={from} oninput={replan} />
      <input placeholder="Into, /home/me/roms-sorted" bind:value={dest} oninput={replan} />
      <label><input type="checkbox" bind:checked={oneG1R} onchange={replan} /> 1G1R</label>
      <button onclick={planOrganize} disabled={!from || !dest || planning}>{planning ? "Planning…" : "Plan"}</button>
      <button onclick={applyOrganize} disabled={!plan || !planned || !plan.moves.length || !!busy}>Move {plan ? plan.moves.length : ""} files</button>
      {#if confirmUndo}
        <span>Move the last batch back?</span>
        <button onclick={undoOrganize} disabled={!!busy}>Yes, move back</button>
        <button onclick={() => (confirmUndo = false)}>Cancel</button>
      {:else}
        <button onclick={() => (confirmUndo = true)} disabled={!!busy}>Undo last batch…</button>
      {/if}
    </div>
    {#if planError}<p class="err" role="alert">{planError}</p>{/if}
    {#if plan}
      <p class="meta">Nothing has moved yet. {plan.moves.length} files would move; {plan.skipped.length} stay where they are.</p>
      <table>
        <thead><tr>
          <th><SortButton label="From" key="from" bind:sort={moveSort} /></th>
          <th><SortButton label="To" key="to" bind:sort={moveSort} /></th>
        </tr></thead>
        <tbody>
          {#each sorted(plan.moves, moveSort, moveGetters) as m}
            <tr><td class="path"><bdi>{m.from}</bdi></td><td class="path"><bdi>{m.to}</bdi></td></tr>
          {/each}
          {#each plan.skipped as s}
            <tr class="stay"><td class="path"><bdi>{s.path}</bdi></td><td>stays: {s.reason}</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
{/if}

<style>
  .path { font-family: ui-monospace, monospace; font-size: 0.8rem; white-space: pre; overflow: hidden; text-overflow: ellipsis; max-width: 40vw; direction: rtl; text-align: left; }
  tr.stay td { color: var(--muted); }
</style>
