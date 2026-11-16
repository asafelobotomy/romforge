<script>
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { run } from "./jobs.svelte.js";

  // Save states left in the old States/ folder (before they lived in
  // Saves/<system>/states/): emulators keep writing there until they move.
  // The core plans, moves and journals; this only shows and asks.
  let { count, done, note = $bindable(""), error = $bindable("") } = $props();
  let working = $state(false);
  // Undo is offered after a move made here.
  let moved = $state(false);

  async function move() {
    const plan = await invoke("merge_states_plan").catch((e) => ((error = String(e)), null));
    if (!plan) return;
    if (plan.problem) {
      error = `Can't move them now: ${plan.problem}.`;
      return;
    }
    const n = plan.moves.filter((m) => !m.taken).length;
    const taken = plan.moves.length - n;
    const ok = await ask(
      `Move ${n} file${n === 1 ? "" : "s"} from States/ into each system's Saves/<system>/states/?` +
        (taken ? ` ${taken} stay, a file of the same name is there already.` : "") +
        " Undo puts them back.",
      { title: "Move save states into Saves", kind: "warning" },
    );
    if (!ok) return;
    working = true;
    const r = await run("Moving save states into Saves…", "merge_states", {});
    if (r) note = `${r[1]} save state file${r[1] === 1 ? "" : "s"} moved into Saves/.`;
    if (r) moved = true;
    working = false;
    await done();
  }

  async function undo() {
    const ok = await ask("Put the last moved save states back into States/?", { title: "Undo the move", kind: "warning" });
    if (!ok) return;
    working = true;
    const n = await run("Putting save states back…", "undo_merge_states", {});
    if (n != null) note = `${n} save state file${n === 1 ? "" : "s"} back in States/.`;
    if (n != null) moved = false;
    working = false;
    await done();
  }
</script>

{#if count || moved}
<div class="banner">
  {#if count}
    <span>{count} save state{count === 1 ? " is" : "s are"} still in the old States/ folder; emulators keep writing there until {count === 1 ? "it moves" : "they move"}.</span>
    <button onclick={move} disabled={working}>Move into Saves…</button>
  {/if}
  {#if moved}<button class="link" onclick={undo} disabled={working}>Undo the move</button>{/if}
</div>
{/if}

<style>
  .banner { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; font-size: 0.85rem; margin: 0.2rem 0 0.5rem; }
  .link { border: none; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
</style>
