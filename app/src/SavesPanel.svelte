<script>
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { run } from "./jobs.svelte.js";

  // "Your saves" on an emulator's tab (milestone 27): saves it made in the
  // places it used before Saves/, moved into Saves/<system>/ when asked.
  // The core plans, moves and journals; this only shows and asks.
  let { emulator, label, note = $bindable(""), error = $bindable("") } = $props();

  let plan = $state(null);
  let working = $state(false);
  const moving = $derived(plan ? plan.moves.filter((m) => !m.taken) : []);
  const taken = $derived(plan ? plan.moves.filter((m) => m.taken) : []);

  async function load() {
    plan = await invoke("gather_saves_plan", { emulator }).catch((e) => {
      error = String(e);
      return null;
    });
  }
  $effect(() => {
    emulator;
    load();
  });

  async function gather() {
    const n = moving.length;
    const ok = await ask(
      `Move ${n} save file${n === 1 ? "" : "s"} of ${label}'s into RomForge's Saves folder? ${label} then saves there. Undo puts them back.`,
      { title: "Gather saves into Saves", kind: "warning" },
    );
    if (!ok) return;
    working = true;
    const done = await run(`Gathering ${label}'s saves…`, "gather_saves", { emulator });
    if (done) note = `${done[1]} of ${label}'s save files are in Saves/ now.`;
    working = false;
    await load();
  }

  async function undo() {
    const ok = await ask(`Put ${label}'s last gathered saves back where they were?`, { title: "Undo gathering", kind: "warning" });
    if (!ok) return;
    working = true;
    const n = await run(`Putting ${label}'s saves back…`, "undo_gather_saves", { emulator });
    if (n != null) note = `${n} of ${label}'s save files are back where they were.`;
    working = false;
    await load();
  }
</script>

{#if plan}
  <section class="setting">
    <span class="flabel">Your saves</span>
    {#if moving.length}
      <p class="hint">
        {label} has saves from before romforge kept them in Saves/; it keeps using them where they are until they move:
      </p>
      <ul class="items">
        {#each moving as m}<li><bdi>{m.from}</bdi> → <bdi>{m.to}</bdi></li>{/each}
      </ul>
    {:else}
      <p class="hint">{label} saves in RomForge's Saves folder.</p>
    {/if}
    {#if taken.length}
      <p class="hint">{taken.length} stay where they are: a file of the same name is in Saves/ already.</p>
    {/if}
    {#if plan.unplaced.length}
      <p class="hint">{plan.unplaced.length} stay where they are: their game isn't in your library.</p>
    {/if}
    {#if moving.length && plan.problem}<p class="hint">Can't move them now: {plan.problem}.</p>{/if}
    <div class="pick">
      {#if moving.length}<button onclick={gather} disabled={working || !!plan.problem}>Move into Saves…</button>{/if}
      <button onclick={undo} disabled={working}>Undo the last move</button>
    </div>
  </section>
{/if}

<style>
  .items { margin: 0.2rem 0 0.4rem 1rem; padding: 0; font-size: 0.8rem; max-height: 10rem; overflow-y: auto; }
  .items li { margin: 0.15rem 0; overflow-wrap: anywhere; }
</style>
